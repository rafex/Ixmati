#!/usr/bin/env python3
"""ANSI terminal view for the same /state endpoint used by the web dashboard."""
import argparse
import json
import time
from urllib.request import urlopen

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:30450/state")
    parser.add_argument("--duration", type=int, default=60)
    args = parser.parse_args()
    end = time.time() + args.duration + 5
    while time.time() < end:
        try:
            with urlopen(args.url, timeout=2) as response:
                data = json.loads(response.read())
            print("\033[2J\033[H", end="")
            print("Ixmati Java live — demo concurrente en contenedores")
            print("Aviso: ambos lados comparten recursos; no es benchmark aislado.")
            print("elapsed={:.0f}s clients={} api_health={}".format(
                data.get("elapsed_seconds", 0), len(data.get("clients", [])),
                data.get("services", {}).get("api_health")))
            print("{:<20}{:>10}{:>12}{:>10}{:>10}{:>10}{:>10}{:>10}{:>10}".format(
                "lado", "writes", "committed", "pending", "errors", "busy", "p50", "p95", "p99"))
            for name in ("direct", "ixmati"):
                side = data.get("sides", {}).get(name, {})
                print("{:<20}{:>10.0f}{:>12.0f}{:>10.0f}{:>10.0f}{:>10.0f}{:>10.0f}{:>10.0f}{:>10.0f}".format(
                    name, side.get("writes_sent", 0), side.get("writes_committed", 0),
                    side.get("pending", 0), side.get("write_errors", 0), side.get("sqlite_busy", 0),
                    side.get("p50_ms", 0), side.get("p95_ms", 0), side.get("p99_ms", 0)))
            print("\nIxmati metrics:", json.dumps(data.get("ixmati_metrics", {}), sort_keys=True))
            print("Direct files:", json.dumps(data.get("direct_files", {}), sort_keys=True))
        except Exception as error:
            print("dashboard unavailable:", error)
        time.sleep(1)

if __name__ == "__main__":
    main()
