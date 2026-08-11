#!/usr/bin/env python3
"""Fixed-rate HTTP read load for cache-aside and materialized projections."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import math
import time
import urllib.error
import urllib.request
from collections import Counter
from dataclasses import dataclass


@dataclass
class Result:
    status: int | None
    latency_ms: float | None
    error: str | None
    valid_payload: bool


def percentile(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    rank = max(1, math.ceil(pct / 100 * len(ordered)))
    return ordered[rank - 1]


def request_once(url: str, timeout: float, expected: str) -> Result:
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(url, timeout=timeout) as response:
            payload = json.loads(response.read())
            status = response.status
    except urllib.error.HTTPError as error:
        error.read()
        return Result(error.code, (time.perf_counter() - started) * 1000, None, False)
    except (TimeoutError, urllib.error.URLError, OSError, json.JSONDecodeError) as error:
        return Result(None, None, type(error).__name__, False)

    if expected == "cache":
        valid = payload.get("found") is True and payload.get("source") == "cache"
    else:
        valid = payload.get("found") is True and payload.get("projection") == expected
    return Result(status, (time.perf_counter() - started) * 1000, None, valid)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("url")
    parser.add_argument("--expected", choices=("cache", "usuarios_materializados", "pedidos_con_usuario"), required=True)
    parser.add_argument("--rate", type=float, required=True)
    parser.add_argument("--duration", type=float, default=30.0)
    parser.add_argument("--concurrency", type=int, default=200)
    parser.add_argument("--timeout", type=float, default=5.0)
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
            in_flight.add(executor.submit(request_once, args.url, args.timeout, args.expected))
            submitted += 1
            next_submit += 1.0 / args.rate
        for future in concurrent.futures.as_completed(in_flight):
            results.append(future.result())

    latencies = [result.latency_ms for result in results if result.latency_ms is not None]
    statuses = Counter(str(result.status) for result in results if result.status is not None)
    errors = Counter(result.error for result in results if result.error is not None)
    invalid = sum(not result.valid_payload for result in results if result.status == 200)
    print(json.dumps({
        "generator": "python-read-rate-load",
        "rate_controlled": True,
        "url": args.url,
        "expected": args.expected,
        "target_rate": args.rate,
        "duration_seconds": args.duration,
        "concurrency": args.concurrency,
        "submitted": submitted,
        "completed": len(results),
        "throughput_per_second": len(results) / max(args.duration, 0.001),
        "client_saturated_ticks": client_saturated,
        "status_codes": dict(sorted(statuses.items())),
        "invalid_success_payloads": invalid,
        "errors": dict(sorted(errors.items())),
        "latency_ms": {
            "p50": percentile(latencies, 50),
            "p90": percentile(latencies, 90),
            "p99": percentile(latencies, 99),
            "max": max(latencies, default=0.0),
        },
    }, sort_keys=True))
    return 0 if not errors and invalid == 0 and statuses.get("200", 0) == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
