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
import time
import urllib.error
import urllib.request
import uuid
from collections import Counter
from dataclasses import dataclass


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
    args = parser.parse_args()

    if args.rate <= 0 or args.duration <= 0 or args.concurrency <= 0:
        parser.error("rate, duration y concurrency deben ser mayores que cero")

    deadline = time.perf_counter() + args.duration
    next_submit = time.perf_counter()
    submitted = 0
    client_saturated = 0
    results: list[Result] = []
    in_flight: set[concurrent.futures.Future[Result]] = set()

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as executor:
        while time.perf_counter() < deadline:
            now = time.perf_counter()
            if now < next_submit:
                time.sleep(next_submit - now)
                continue

            done = {future for future in in_flight if future.done()}
            for future in done:
                results.append(future.result())
            in_flight -= done

            if len(in_flight) >= args.concurrency:
                client_saturated += 1
                done_future = next(concurrent.futures.as_completed(in_flight))
                in_flight.remove(done_future)
                results.append(done_future.result())

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
            results.append(future.result())

    latencies = [result.latency_ms for result in results if result.latency_ms is not None]
    statuses = Counter(str(result.status) for result in results if result.status is not None)
    errors = Counter(result.error for result in results if result.error is not None)
    elapsed = max(args.duration, 0.001)
    output = {
        "generator": "python-rate-load",
        "rate_controlled": True,
        "target_rate": args.rate,
        "duration_seconds": args.duration,
        "concurrency": args.concurrency,
        "submitted": submitted,
        "completed": len(results),
        "throughput_per_second": len(results) / elapsed,
        "client_saturated_ticks": client_saturated,
        "status_codes": dict(sorted(statuses.items())),
        "errors": dict(sorted(errors.items())),
        "latency_ms": {
            "p50": percentile(latencies, 50),
            "p90": percentile(latencies, 90),
            "p99": percentile(latencies, 99),
            "max": max(latencies, default=0.0),
        },
    }
    print(json.dumps(output, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
