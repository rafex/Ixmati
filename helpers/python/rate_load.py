#!/usr/bin/env python3
"""Fixed-rate HTTP load generator for committed writes.

The standard-library implementation keeps the load test reproducible on the
Debian test host without depending on wrk2 or a package manager. It creates a
fresh idempotency key for every request and reports enough data to distinguish
server saturation from client-side concurrency saturation.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import math
import random
import time
import urllib.error
import urllib.request
import uuid
from collections import Counter
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Result:
    status: int | None
    latency_ms: float | None
    error: str | None


def percentile(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    rank = max(1, math.ceil(pct / 100 * len(ordered)))
    return ordered[rank - 1]


class Reservoir:
    """Bounded latency sample suitable for multi-hour runs."""

    def __init__(self, capacity: int = 10000) -> None:
        self.capacity = capacity
        self.values: list[float] = []
        self.seen = 0
        self.random = random.Random(0)

    def add(self, value: float) -> None:
        self.seen += 1
        if len(self.values) < self.capacity:
            self.values.append(value)
            return
        index = self.random.randrange(self.seen)
        if index < self.capacity:
            self.values[index] = value


def request_once(url: str, timeout: float, api_key: str, store: str, entity: str) -> Result:
    request_id = uuid.uuid4().hex
    payload = {
        "op": "upsert",
        "store": store,
        "entity": entity,
        "key": f"rate-load-{request_id}",
        "version": 1,
        "ts": "2026-01-01T00:00:00Z",
        "idempotency_key": f"rate-load-{request_id}",
        "ack_mode": "committed",
        "payload": {"generator": "rate_load", "request_id": request_id},
    }
    body = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        url,
        data=body,
        method="POST",
        headers={
            "Authorization": f"ApiKey {api_key}",
            "Content-Type": "application/json",
        },
    )
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            response.read()
            status = response.status
    except urllib.error.HTTPError as error:
        error.read()
        status = error.code
    except (TimeoutError, urllib.error.URLError, OSError) as error:
        return Result(None, None, type(error).__name__)
    elapsed_ms = (time.perf_counter() - started) * 1000
    return Result(status, elapsed_ms, None)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("url")
    parser.add_argument("--rate", type=float, required=True)
    parser.add_argument("--duration", type=float, default=30.0)
    parser.add_argument("--concurrency", type=int, default=200)
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--api-key", default="ix-default-key")
    parser.add_argument("--store", default="default")
    parser.add_argument("--entity", default="rate-load")
    parser.add_argument(
        "--sample-interval",
        type=float,
        default=10.0,
        help="seconds between cumulative JSONL snapshots (0 disables snapshots)",
    )
    parser.add_argument(
        "--snapshot-file",
        type=Path,
        help="append bounded-memory cumulative snapshots as JSON lines",
    )
    parser.add_argument(
        "--reservoir-size",
        type=int,
        default=10000,
        help="maximum latency samples retained for final percentiles",
    )
    args = parser.parse_args()

    if args.rate <= 0 or args.duration <= 0 or args.concurrency <= 0:
        parser.error("rate, duration y concurrency deben ser mayores que cero")

    if args.sample_interval < 0 or args.reservoir_size <= 0:
        parser.error("sample-interval debe ser >= 0 y reservoir-size > 0")

    deadline = time.perf_counter() + args.duration
    next_submit = time.perf_counter()
    next_snapshot = next_submit + args.sample_interval
    submitted = 0
    client_saturated = 0
    completed = 0
    reservoir = Reservoir(args.reservoir_size)
    statuses: Counter[str] = Counter()
    errors: Counter[str] = Counter()
    window_completed = 0
    window_started = time.perf_counter()
    in_flight: set[concurrent.futures.Future[Result]] = set()

    if args.snapshot_file:
        args.snapshot_file.parent.mkdir(parents=True, exist_ok=True)

    def consume(result: Result) -> None:
        nonlocal completed, window_completed
        completed += 1
        window_completed += 1
        if result.status is not None:
            statuses[str(result.status)] += 1
        if result.error is not None:
            errors[result.error] += 1
        if result.latency_ms is not None:
            reservoir.add(result.latency_ms)

    def snapshot(now: float, force: bool = False) -> None:
        nonlocal next_snapshot, window_completed, window_started
        if args.sample_interval <= 0 or (not force and now < next_snapshot):
            return
        elapsed_window = max(now - window_started, 0.001)
        data = {
            "target_rate": args.rate,
            "elapsed_seconds": now - (deadline - args.duration),
            "window_seconds": elapsed_window,
            "window_completed": window_completed,
            "window_throughput_per_second": window_completed / elapsed_window,
            "completed": completed,
            "submitted": submitted,
            "client_saturated_ticks": client_saturated,
            "status_codes": dict(sorted(statuses.items())),
            "errors": dict(sorted(errors.items())),
        }
        if args.snapshot_file:
            with args.snapshot_file.open("a", encoding="utf-8") as stream:
                stream.write(json.dumps(data, sort_keys=True) + "\n")
        window_completed = 0
        window_started = now
        next_snapshot = now + args.sample_interval

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as executor:
        while time.perf_counter() < deadline:
            now = time.perf_counter()
            if now < next_submit:
                time.sleep(next_submit - now)
                continue

            done = {future for future in in_flight if future.done()}
            for future in done:
                consume(future.result())
            in_flight -= done
            snapshot(now)

            if len(in_flight) >= args.concurrency:
                client_saturated += 1
                done_future = next(concurrent.futures.as_completed(in_flight))
                in_flight.remove(done_future)
                consume(done_future.result())

            in_flight.add(
                executor.submit(
                    request_once,
                    args.url,
                    args.timeout,
                    args.api_key,
                    args.store,
                    args.entity,
                )
            )
            submitted += 1
            next_submit += 1.0 / args.rate

        for future in concurrent.futures.as_completed(in_flight):
            consume(future.result())

    snapshot(time.perf_counter(), force=True)

    elapsed = max(args.duration, 0.001)
    output = {
        "generator": "python-rate-load",
        "rate_controlled": True,
        "target_rate": args.rate,
        "duration_seconds": args.duration,
        "concurrency": args.concurrency,
        "submitted": submitted,
        "completed": completed,
        "throughput_per_second": completed / elapsed,
        "client_saturated_ticks": client_saturated,
        "status_codes": dict(sorted(statuses.items())),
        "errors": dict(sorted(errors.items())),
        "latency_ms": {
            "p50": percentile(reservoir.values, 50),
            "p90": percentile(reservoir.values, 90),
            "p99": percentile(reservoir.values, 99),
            "max": max(reservoir.values, default=0.0),
        },
        "latency_reservoir": {
            "capacity": reservoir.capacity,
            "seen": reservoir.seen,
            "retained": len(reservoir.values),
        },
    }
    print(json.dumps(output, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
