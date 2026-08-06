#!/usr/bin/env python3
"""load_test.py — prueba de carga de reads para evaluar modos de cache

Uso:
  python3 load_test.py --url http://127.0.0.1:30000 \
    --auth "Bearer smoke-test-key" \
    --concurrency 50 --reads 1000
"""

import argparse
import concurrent.futures
import json
import sys
import time
import urllib.error
import urllib.request


def read_one(url, store, entity, key, auth):
    t0 = time.monotonic()
    req = urllib.request.Request(
        f"{url}/read?store={store}&entity={entity}&key={key}"
    )
    req.add_header("Authorization", auth)
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            body = json.loads(resp.read())
        latency_ms = (time.monotonic() - t0) * 1000
        return latency_ms, body.get("source", "?"), resp.status
    except urllib.error.HTTPError as e:
        latency_ms = (time.monotonic() - t0) * 1000
        return latency_ms, f"http_{e.code}", e.code
    except Exception as e:
        latency_ms = (time.monotonic() - t0) * 1000
        return latency_ms, f"error_{type(e).__name__}", 0


def run_load_test(url, auth, store, entity, keys, concurrency, total_reads):
    latencies = []
    sources = {}
    errors = 0

    t0_total = time.monotonic()
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:
        futures = []
        for i in range(total_reads):
            key = keys[i % len(keys)]
            futures.append(
                pool.submit(read_one, url, store, entity, key, auth)
            )

        for fut in concurrent.futures.as_completed(futures):
            try:
                lat, src, code = fut.result()
                latencies.append(lat)
                sources[src] = sources.get(src, 0) + 1
                if code not in (200, 404):
                    errors += 1
            except Exception:
                errors += 1

    elapsed_total = time.monotonic() - t0_total
    latencies.sort()

    n = len(latencies)
    if n == 0:
        return {"concurrency": concurrency, "total": total_reads, "errors": total_reads}

    p50 = latencies[n // 2]
    p99 = latencies[min(int(n * 0.99), n - 1)]
    p999 = latencies[min(int(n * 0.999), n - 1)]
    max_lat = latencies[-1]
    avg = sum(latencies) / n
    throughput = total_reads / elapsed_total if elapsed_total > 0 else 0
    cache_hits = sources.get("cache", 0)
    sqlite_fallbacks = sources.get("sqlite", 0)

    return {
        "concurrency": concurrency,
        "total": total_reads,
        "elapsed_s": round(elapsed_total, 2),
        "p50_ms": round(p50, 3),
        "p99_ms": round(p99, 3),
        "p999_ms": round(p999, 3),
        "max_ms": round(max_lat, 3),
        "avg_ms": round(avg, 3),
        "throughput_reads_s": round(throughput, 1),
        "errors": errors,
        "cache_hits": cache_hits,
        "sqlite_fallbacks": sqlite_fallbacks,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:30000")
    parser.add_argument("--auth", default="Bearer smoke-test-key")
    parser.add_argument("--store", default="default")
    parser.add_argument("--entity", default="load")
    parser.add_argument("--concurrency", type=int, required=True)
    parser.add_argument("--reads", type=int, default=1000)
    parser.add_argument("--keys-file", default=None)
    args = parser.parse_args()

    if args.keys_file:
        with open(args.keys_file) as f:
            keys = [line.strip() for line in f if line.strip()]
    else:
        keys = [f"k{i}" for i in range(min(args.reads, 100))]

    result = run_load_test(
        args.url,
        args.auth,
        args.store,
        args.entity,
        keys,
        args.concurrency,
        args.reads,
    )
    print(json.dumps(result))


if __name__ == "__main__":
    main()
