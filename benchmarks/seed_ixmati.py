#!/usr/bin/env python3
"""Seed equivalent users and orders through the Ixmati durable API."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import time
import urllib.error
import urllib.request


def write(url: str, api_key: str, store: str, entity: str, key: str, payload: dict) -> str:
    body = json.dumps({
        "op": "upsert", "store": store, "entity": entity, "key": key,
        "version": 1, "ts": "2026-01-01T00:00:00Z",
        "idempotency_key": f"benchmark-seed-{store}-{key}",
        "ack_mode": "committed", "payload": payload,
    }).encode()
    request = urllib.request.Request(
        f"{url}/write", data=body, method="POST",
        headers={"Authorization": f"ApiKey {api_key}", "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            response.read()
            return str(response.status)
    except urllib.error.HTTPError as error:
        error.read()
        return str(error.code)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("url")
    parser.add_argument("--api-key", default="ix-default-key")
    parser.add_argument("--users", type=int, default=1000)
    parser.add_argument("--orders", type=int, default=10000)
    parser.add_argument("--concurrency", type=int, default=32)
    args = parser.parse_args()
    jobs = []
    for index in range(args.users):
        key = f"usr_{index:06d}"
        jobs.append(("usuarios", "usuario", key, {
            "usuario_id": key, "nombre": f"Usuario {index}", "email": f"usuario{index}@example.test",
        }))
    for index in range(args.orders):
        key = f"ped_{index:06d}"
        jobs.append(("pedidos", "pedido", key, {
            "pedido_id": key, "usuario_id": f"usr_{index % args.users:06d}",
            "total": round(100 + (index % 1000) * 1.25, 2), "estado": "pendiente",
        }))
    started = time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        statuses = list(pool.map(
            lambda job: write(args.url, args.api_key, *job), jobs
        ))
    counts = {}
    for status in statuses:
        counts[status] = counts.get(status, 0) + 1
    result = {"users": args.users, "orders": args.orders, "elapsed_seconds": time.perf_counter() - started, "status_codes": counts}
    print(json.dumps(result, sort_keys=True))
    return 0 if set(counts).issubset({"200", "202"}) else 1


if __name__ == "__main__":
    raise SystemExit(main())
