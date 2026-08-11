#!/usr/bin/env python3
"""Reproducible direct-database and HTTP benchmark runner."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import math
import random
import sqlite3
import threading
import time
import urllib.parse
import urllib.request
import uuid
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent
SQLITE_SCHEMA = (ROOT / "schema" / "sqlite.sql").read_text()
POSTGRES_SCHEMA = (ROOT / "schema" / "postgres.sql").read_text()


@dataclass
class OperationResult:
    latency_ms: float
    error: str | None = None


class HttpStatusError(Exception):
    """HTTP response classified as a benchmark result instead of a traceback."""

    def __init__(self, status: int) -> None:
        super().__init__(f"HTTP {status}")
        self.status = status


_thread_connections = threading.local()


def percentile(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    return ordered[max(1, math.ceil(pct / 100 * len(ordered))) - 1]


def user_payload(index: int) -> dict[str, Any]:
    return {
        "usuario_id": f"usr_{index:06d}",
        "nombre": f"Usuario {index}",
        "email": f"usuario{index}@example.test",
    }


def order_payload(index: int, user_count: int = 10000) -> dict[str, Any]:
    return {
        "pedido_id": f"ped_{index:06d}",
        "usuario_id": f"usr_{index % user_count:06d}",
        "total": round(100 + (index % 1000) * 1.25, 2),
        "estado": "pendiente",
    }


def sqlite_connection(path: str) -> sqlite3.Connection:
    conn = sqlite3.connect(path, timeout=5.0, check_same_thread=False)
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA synchronous=NORMAL")
    conn.execute("PRAGMA busy_timeout=5000")
    return conn


def postgres_connection(dsn: str):
    try:
        import psycopg
    except ImportError as error:
        raise SystemExit(
            "PostgreSQL requiere psycopg: usa uv run --with 'psycopg[binary]==3.2.9'"
        ) from error
    conn = psycopg.connect(dsn)
    conn.execute("SET synchronous_commit = on")
    return conn


def worker_connection(engine: str, target: str) -> Any:
    """Reuse one direct-database connection per benchmark worker thread."""
    current = getattr(_thread_connections, "value", None)
    if current and current[0] == engine and current[1] == target:
        return current[2]
    conn = sqlite_connection(target) if engine == "sqlite" else postgres_connection(target)
    _thread_connections.value = (engine, target, conn)
    return conn


def init_database(engine: str, target: str) -> None:
    if engine == "sqlite":
        path = Path(target)
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.exists():
            path.unlink()
        conn = sqlite_connection(target)
        conn.executescript(SQLITE_SCHEMA)
        conn.commit()
        conn.close()
        return
    conn = postgres_connection(target)
    conn.execute(POSTGRES_SCHEMA)
    conn.commit()
    conn.close()


def seed_database(engine: str, target: str, users: int, orders: int) -> None:
    if engine == "sqlite":
        conn = sqlite_connection(target)
        conn.executemany(
            "INSERT INTO payload_usuarios(entity,key,version,payload) VALUES(?,?,1,?)",
            [("usuario", f"usr_{i:06d}", json.dumps(user_payload(i))) for i in range(users)],
        )
        conn.executemany(
            "INSERT INTO payload_pedidos(entity,key,version,payload) VALUES(?,?,1,?)",
            [("pedido", f"ped_{i:06d}", json.dumps(order_payload(i, users))) for i in range(orders)],
        )
        conn.commit()
        conn.close()
        return
    conn = postgres_connection(target)
    with conn.cursor() as cur:
        cur.executemany(
            "INSERT INTO payload_usuarios(entity,key,version,payload) VALUES(%s,%s,1,%s::jsonb)",
            [("usuario", f"usr_{i:06d}", json.dumps(user_payload(i))) for i in range(users)],
        )
        cur.executemany(
            "INSERT INTO payload_pedidos(entity,key,version,payload) VALUES(%s,%s,1,%s::jsonb)",
            [("pedido", f"ped_{i:06d}", json.dumps(order_payload(i, users))) for i in range(orders)],
        )
    conn.commit()
    conn.close()


def write_sqlite(conn: sqlite3.Connection, key: str, version: int, idem: str) -> None:
    payload = json.dumps({"pedido_id": key, "usuario_id": "usr_000001", "version": version})
    with conn:
        if conn.execute("SELECT 1 FROM _idempotency WHERE idempotency_key=?", (idem,)).fetchone():
            return
        conn.execute(
            "INSERT INTO payload_pedidos(entity,key,version,payload) VALUES('pedido',?,?,?) "
            "ON CONFLICT(entity,key) DO UPDATE SET version=excluded.version,payload=excluded.payload,updated_at=datetime('now')",
            (key, version, payload),
        )
        conn.execute(
            "INSERT INTO _idempotency(idempotency_key,store,entity,key,version) VALUES(?,?,?,?,?)",
            (idem, "pedidos", "pedido", key, version),
        )
        conn.execute(
            "INSERT INTO _outbox(event_id,event_type,store,entity,key,version,payload) VALUES(?,?,?,?,?,?,?)",
            (str(uuid.uuid4()), "pedido.actualizado", "pedidos", "pedido", key, version, payload),
        )


def write_postgres(conn: Any, key: str, version: int, idem: str) -> None:
    payload = {"pedido_id": key, "usuario_id": "usr_000001", "version": version}
    with conn.transaction(), conn.cursor() as cur:
            cur.execute("SELECT 1 FROM _idempotency WHERE idempotency_key=%s", (idem,))
            if cur.fetchone():
                return
            cur.execute(
                "INSERT INTO payload_pedidos(entity,key,version,payload) VALUES('pedido',%s,%s,%s::jsonb) "
                "ON CONFLICT(entity,key) DO UPDATE SET version=EXCLUDED.version,payload=EXCLUDED.payload,updated_at=now()",
                (key, version, json.dumps(payload)),
            )
            cur.execute(
                "INSERT INTO _idempotency(idempotency_key,store,entity,key,version) VALUES(%s,%s,%s,%s,%s)",
                (idem, "pedidos", "pedido", key, version),
            )
            cur.execute(
                "INSERT INTO _outbox(event_id,event_type,store,entity,key,version,payload) VALUES(%s,%s,%s,%s,%s,%s,%s::jsonb)",
                (str(uuid.uuid4()), "pedido.actualizado", "pedidos", "pedido", key, version, json.dumps(payload)),
            )


def direct_operation(conn: Any, engine: str, operation: str, key_index: int, sequence: int) -> None:
    key = f"ped_{key_index % 10000:06d}"
    if operation == "read_point":
        query = "SELECT payload FROM payload_pedidos WHERE entity='pedido' AND key=?" if engine == "sqlite" else "SELECT payload FROM payload_pedidos WHERE entity='pedido' AND key=%s"
        conn.execute(query, (key,)).fetchone()
    elif operation == "read_join":
        if engine == "sqlite":
            conn.execute(
                "SELECT p.payload,u.payload FROM payload_pedidos p JOIN payload_usuarios u "
                "ON json_extract(p.payload,'$.usuario_id')=u.key WHERE p.entity='pedido' AND p.key=?",
                (key,),
            ).fetchone()
        else:
            conn.execute(
                "SELECT p.payload,u.payload FROM payload_pedidos p JOIN payload_usuarios u "
                "ON p.payload->>'usuario_id'=u.key WHERE p.entity='pedido' AND p.key=%s",
                (key,),
            ).fetchone()
    elif operation == "idempotency":
        idem = "benchmark-fixed-idempotency"
        if engine == "sqlite":
            with conn:
                conn.execute("SELECT 1 FROM _idempotency WHERE idempotency_key=?", (idem,)).fetchone()
        else:
            with conn.transaction():
                conn.execute("SELECT 1 FROM _idempotency WHERE idempotency_key=%s", (idem,)).fetchone()
    elif operation in {"write", "update"}:
        key = f"bench_{sequence:012d}" if operation == "write" else key
        idem = f"benchmark-{operation}-{sequence}-{uuid.uuid4().hex}"
        if engine == "sqlite":
            write_sqlite(conn, key, 1 if operation == "write" else sequence + 2, idem)
        else:
            write_postgres(conn, key, 1 if operation == "write" else sequence + 2, idem)
    else:
        raise ValueError(f"operación desconocida: {operation}")


def http_operation(url: str, operation: str, key_index: int, sequence: int, api_key: str) -> None:
    headers = {"Authorization": f"ApiKey {api_key}"}
    if operation == "read_point":
        query = urllib.parse.urlencode({"store": "pedidos", "entity": "pedido", "key": f"ped_{key_index % 10000:06d}"})
        request = urllib.request.Request(f"{url}/read?{query}", headers=headers)
    elif operation == "read_join":
        query = urllib.parse.urlencode({"projection": "pedidos_con_usuario", "key": f"ped_{key_index % 10000:06d}"})
        request = urllib.request.Request(f"{url}/read?{query}", headers=headers)
    elif operation in {"write", "update"}:
        key = f"bench_{sequence:012d}" if operation == "write" else f"ped_{key_index % 10000:06d}"
        payload = {
            "op": "upsert", "store": "pedidos", "entity": "pedido", "key": key,
            "version": 1 if operation == "write" else sequence + 2,
            "ts": "2026-01-01T00:00:00Z", "idempotency_key": f"benchmark-{uuid.uuid4().hex}",
            "ack_mode": "committed", "payload": {"pedido_id": key, "usuario_id": "usr_000001", "version": 1},
        }
        request = urllib.request.Request(
            f"{url}/write", data=json.dumps(payload).encode(), method="POST",
            headers={**headers, "Content-Type": "application/json"},
        )
    else:
        raise ValueError(f"operación HTTP desconocida: {operation}")
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            if response.status not in (200, 202):
                raise HttpStatusError(response.status)
            response.read()
    except urllib.error.HTTPError as error:
        error.read()
        raise HttpStatusError(error.code) from error


def execute_batch(engine: str, target: str, operation: str, indices: list[int], sequence: int, api_key: str) -> list[OperationResult]:
    started = time.perf_counter()
    try:
        if engine == "http":
            for offset, key_index in enumerate(indices):
                http_operation(target, operation, key_index, sequence + offset, api_key)
        else:
            conn = worker_connection(engine, target)
            for offset, key_index in enumerate(indices):
                direct_operation(conn, engine, operation, key_index, sequence + offset)
            if engine == "postgres":
                conn.commit()
        elapsed = (time.perf_counter() - started) * 1000
        return [OperationResult(elapsed) for _ in indices]
    except Exception as error:  # noqa: BLE001 - benchmark must classify all driver failures
        if engine == "postgres":
            try:
                conn.rollback()
            except (UnboundLocalError, AttributeError):
                pass
        elapsed = (time.perf_counter() - started) * 1000
        label = f"http_{error.status}" if isinstance(error, HttpStatusError) else type(error).__name__
        return [OperationResult(elapsed, label) for _ in indices]


def run_window(args: argparse.Namespace, record: bool) -> tuple[list[OperationResult], int]:
    deadline = time.perf_counter() + args.duration
    next_submit = time.perf_counter()
    sequence = 0
    saturated = 0
    results: list[OperationResult] = []
    futures: set[concurrent.futures.Future[list[OperationResult]]] = set()
    rng = random.Random(args.seed)
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as executor:
        while time.perf_counter() < deadline:
            now = time.perf_counter()
            if now < next_submit:
                time.sleep(next_submit - now)
                continue
            done = {future for future in futures if future.done()}
            for future in done:
                results.extend(future.result())
            futures -= done
            if len(futures) >= args.concurrency:
                saturated += 1
                future = next(concurrent.futures.as_completed(futures))
                futures.remove(future)
                results.extend(future.result())
            operation = args.operation
            if operation == "mixed":
                operation = "read_point" if rng.random() < 0.8 else "write"
            count = args.batch_size if operation == "write" else 1
            indices = [sequence + i for i in range(count)]
            futures.add(executor.submit(execute_batch, args.engine, args.target, operation, indices, sequence, args.api_key))
            sequence += count
            next_submit += count / args.rate
        for future in concurrent.futures.as_completed(futures):
            results.extend(future.result())
    return (results if record else []), saturated


def load(args: argparse.Namespace) -> int:
    if args.warmup > 0:
        warmup = argparse.Namespace(**vars(args))
        warmup.duration = args.warmup
        run_window(warmup, record=False)
    results, saturated = run_window(args, record=True)
    latencies = [result.latency_ms for result in results]
    errors = Counter(result.error for result in results if result.error)
    successful = len(results) - sum(errors.values())
    output = {
        "engine": args.engine, "target": args.target, "operation": args.operation,
        "target_rate": args.rate, "duration_seconds": args.duration, "warmup_seconds": args.warmup,
        "cache_state": args.cache_state,
        "concurrency": args.concurrency, "batch_size": args.batch_size,
        "submitted_operations": len(results), "successful_operations": successful,
        "throughput_per_second": successful / max(args.duration, 0.001),
        "client_saturated_ticks": saturated, "errors": dict(sorted(errors.items())),
        "latency_ms": {"p50": percentile(latencies, 50), "p95": percentile(latencies, 95), "p99": percentile(latencies, 99), "max": max(latencies, default=0)},
        "valid_rate_controlled": saturated == 0,
    }
    print(json.dumps(output, sort_keys=True))
    return 0 if not errors else 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    for engine in ("sqlite", "postgres"):
        sub = subparsers.add_parser(f"init-{engine}")
        sub.add_argument("target")
        sub.set_defaults(func=lambda args, e=engine: init_database(e, args.target))
        sub = subparsers.add_parser(f"seed-{engine}")
        sub.add_argument("target")
        sub.add_argument("--users", type=int, default=10000)
        sub.add_argument("--orders", type=int, default=100000)
        sub.set_defaults(func=lambda args, e=engine: seed_database(e, args.target, args.users, args.orders))
    sub = subparsers.add_parser("load")
    sub.add_argument("--engine", choices=("sqlite", "postgres", "http"), required=True)
    sub.add_argument("--target", required=True)
    sub.add_argument("--operation", choices=("read_point", "read_join", "write", "update", "idempotency", "mixed"), required=True)
    sub.add_argument("--rate", type=float, required=True)
    sub.add_argument("--duration", type=float, default=30)
    sub.add_argument("--warmup", type=float, default=15)
    sub.add_argument("--cache-state", choices=("cold-first-pass", "warm"), default="warm")
    sub.add_argument("--concurrency", type=int, default=16)
    sub.add_argument("--batch-size", type=int, default=1)
    sub.add_argument("--api-key", default="ix-default-key")
    sub.add_argument("--seed", type=int, default=20260811)
    sub.set_defaults(func=load)
    return parser.parse_args()


if __name__ == "__main__":
    parsed = parse_args()
    result = parsed.func(parsed)
    raise SystemExit(result if isinstance(result, int) else 0)
