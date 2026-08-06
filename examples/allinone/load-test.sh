#!/bin/bash
# load-test.sh — prueba de carga comparativa de modos de cache
#
# Ejecutar en el bastion:
#   cd ~/Ixmati/examples/allinone && ./load-test.sh
#
# Config: MODES array, CONCURRENCY_LEVELS
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RESULTS_FILE="$SCRIPT_DIR/test-results/LOAD-TEST.md"
RAW_FILE="$SCRIPT_DIR/test-results/load-test-results.json"

MODES=("direct" "socket" "mqtt")
BACKENDS=("sqlite" "redb" "redb")
CONCURRENCY_LEVELS=(1 10 50 100 200 500 1000)
N_WRITES=100
N_READS=1000
API_URL="http://127.0.0.1:30000"
AUTH="Bearer smoke-test-key"
STORE="default"
ENTITY="load"

mkdir -p "$SCRIPT_DIR/test-results"

# Clean state
podman rm -f ixmati-allinone 2>/dev/null || true

ALL_RESULTS="["

for i in "${!MODES[@]}"; do
    mode="${MODES[$i]}"
    backend="${BACKENDS[$i]}"

    echo ""
    echo "============================================================"
    echo "  Testing: CACHE_READ_MODE=$mode  CACHE_BACKEND=$backend"
    echo "============================================================"

    # Remove old DB for clean state
    rm -f /home/rafex/.local/share/containers/storage/volumes/ixmati-allinone-data/_data/stores/default.db 2>/dev/null || true

    # Start container
    podman rm -f ixmati-allinone 2>/dev/null || true
    podman run -d --name ixmati-allinone --network=host \
        -e CACHE_BACKEND="$backend" \
        -e CACHE_READ_MODE="$mode" \
        -e CACHE_DIR=/var/lib/ixmati/cache \
        -e IXMATI_API_KEYS=smoke-test-key \
        -e STORE_NAME="$STORE" \
        -e SQLITE_PATH=/var/lib/ixmati/stores/default.db \
        localhost/ixmati-allinone:local

    echo "  Waiting for health check..."
    for _ in $(seq 1 30); do
        if curl -sf "$API_URL/health" >/dev/null 2>&1; then
            echo "  Health OK"
            break
        fi
        sleep 1
    done

    # Seed data: write 100 keys
    echo "  Seeding $N_WRITES writes..."
    python3 -c "
import json, time, uuid, urllib.request

API = '$API_URL'
AUTH = '$AUTH'
STORE = '$STORE'
ENTITY = '$ENTITY'

for i in range($N_WRITES):
    ik = str(uuid.uuid4())
    cmd = {
        'op': 'upsert', 'store': STORE, 'entity': ENTITY,
        'key': f'k{i}', 'version': 1,
        'ts': __import__('time').strftime('%Y-%m-%dT%H:%M:%SZ', __import__('time').gmtime()),
        'idempotency_key': f'seed-{i}-{ik}',
        'ack_mode': 'accepted',
        'payload': {'i': i, 'data': f'load-test-key-{i}'}
    }
    req = urllib.request.Request(
        f'{API}/write',
        data=json.dumps(cmd).encode(),
        headers={'Content-Type': 'application/json', 'Authorization': AUTH},
        method='POST'
    )
    try:
        with urllib.request.urlopen(req, timeout=5) as r:
            pass
    except:
        pass
print('Seeded $N_WRITES writes')
" 2>/dev/null

    echo "  Waiting for writes to be APPLIED (5s)..."
    sleep 5

    # Write keys file for load test
    KEYS_FILE=$(mktemp)
    python3 -c "
for i in range($N_WRITES):
    print(f'k{i}')
" > "$KEYS_FILE"

    # Run load test for each concurrency level
    for conc in "${CONCURRENCY_LEVELS[@]}"; do
        echo "  Concurrency=$conc..."
        result=$(python3 "$SCRIPT_DIR/python/load_test.py" \
            --url "$API_URL" \
            --auth "$AUTH" \
            --store "$STORE" \
            --entity "$ENTITY" \
            --concurrency "$conc" \
            --reads "$N_READS" \
            --keys-file "$KEYS_FILE" 2>/dev/null)

        if [ -n "$result" ]; then
            # Inject mode and backend into result
            labeled=$(echo "$result" | python3 -c "
import json, sys
d = json.load(sys.stdin)
d['mode'] = '$mode'
d['backend'] = '$backend'
print(json.dumps(d))
" 2>/dev/null)

            if [ -n "$labeled" ]; then
                if [ "$ALL_RESULTS" != "[" ]; then
                    ALL_RESULTS+=","
                fi
                ALL_RESULTS+="$labeled"
            fi

            p50=$(echo "$result" | python3 -c "import json,sys; print(json.load(sys.stdin)['p50_ms'])" 2>/dev/null || echo "?")
            p99=$(echo "$result" | python3 -c "import json,sys; print(json.load(sys.stdin)['p99_ms'])" 2>/dev/null || echo "?")
            thr=$(echo "$result" | python3 -c "import json,sys; print(json.load(sys.stdin)['throughput_reads_s'])" 2>/dev/null || echo "?")
            errs=$(echo "$result" | python3 -c "import json,sys; print(json.load(sys.stdin).get('errors',0))" 2>/dev/null || echo "?")
            echo "    p50=${p50}ms  p99=${p99}ms  thr=${thr}/s  errors=$errs"
        fi
    done

    rm -f "$KEYS_FILE"

    # Health check
    health=$(curl -s "$API_URL/health" 2>/dev/null)
    overall=$(echo "$health" | python3 -c "import json,sys; print(json.load(sys.stdin)['overall'])" 2>/dev/null || echo "?")
    echo "  Post-load health: $overall"

    # Tear down
    podman rm -f ixmati-allinone 2>/dev/null || true
done

ALL_RESULTS+="]"

# Save raw results
echo "$ALL_RESULTS" | python3 -m json.tool > "$RAW_FILE" 2>/dev/null || echo "$ALL_RESULTS" > "$RAW_FILE"
echo "Raw results saved to $RAW_FILE"

# Generate Markdown report
python3 -c "
import json, sys

with open('$RAW_FILE') as f:
    results = json.load(f)

print('# Ixmati Load Test — Comparativa de Modos de Cache')
print()
print(f'> Date: $(date -u +%Y-%m-%d) | Writes: $N_WRITES | Reads per level: $N_READS')
print()
print('## Results')
print()
print('| Mode | Backend | Concurrency | p50 ms | p99 ms | p999 ms | reads/s | Errors | Cache Hits | SQLite Falls |')
print('|------|---------|-------------|--------|--------|---------|---------|--------|------------|--------------|')

for r in results:
    p50 = r.get('p50_ms', '?')
    p99 = r.get('p99_ms', '?')
    p999 = r.get('p999_ms', '?')
    thr = r.get('throughput_reads_s', '?')
    errs = r.get('errors', '?')
    hits = r.get('cache_hits', '?')
    misses = r.get('sqlite_fallbacks', '?')
    print(f\"| {r['mode']} | {r['backend']} | {r['concurrency']} | {p50} | {p99} | {p999} | {thr} | {errs} | {hits} | {misses} |\")

print()
print('## Key Insight')
print()
print('¿A qué concurrencia SQLite WAL (direct) degrada y se empareja con socket/mqtt?')
print()
print('El overhead de socket (~100µs) y mqtt (~2ms) es constante. Direct (28µs) debería mantenerse')
print('hasta que la contención de locks en SQLite WAL fuerce serialización de lecturas.')
" > "$RESULTS_FILE"

echo "Report saved to $RESULTS_FILE"
