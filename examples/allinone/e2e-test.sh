#!/bin/bash
# e2e-test.sh — smoke test automatizado para all-in-one (bash + curl + jq)
# Uso: ./e2e-test.sh
# Configura con: IXMATI_HOST, IXMATI_API_PORT, IXMATI_MQTT_PORT, IXMATI_API_KEY, IXMATI_STORE
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/shell-helpers.sh"

PASS=0
FAIL=0

echo "=== Ixmati All-in-One E2E Test ==="
echo "  API:   ${API_BASE}"
echo "  Store: ${IXMATI_STORE}"
echo "  Key:   ${IXMATI_API_KEY}"
echo ""

# 1. Health check
echo "[1] Health check"
health=$(ixmati_health 2>/dev/null)
assert_json "GET /health → OK" "['overall']" "OK" "$health"

# 2. Write sin auth → 401
echo "[2] POST /write sin auth → 401"
code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${API_BASE}/write" \
    -H "Content-Type: application/json" \
    -d '{"op":"upsert","store":"'"${IXMATI_STORE}"'","entity":"e2e","key":"noauth","version":1,"ts":"2026-01-01T00:00:00Z","idempotency_key":"00000000-0000-0000-0000-000000000001","ack_mode":"accepted","payload":{}}')
assert_eq "POST /write sin auth → 401" "401" "$code"

# 3. Write accepted
echo "[3] POST /write → ACCEPTED"
IDEM1=$(uuidgen)
resp=$(ixmati_write "$IXMATI_STORE" "e2e" "e2e-1" 1 "accepted" "$IDEM1" '{"total":1500,"estado":"pendiente"}')
assert_json "POST /write → ACCEPTED" "['status']" "ACCEPTED" "$resp"

# 4. Write committed
echo "[4] POST /write committed → ACCEPTED"
IDEM2=$(uuidgen)
resp=$(ixmati_write "$IXMATI_STORE" "e2e" "e2e-2" 1 "committed" "$IDEM2" '{"total":2500,"estado":"confirmado"}')
assert_json "POST /write committed → ACCEPTED" "['status']" "ACCEPTED" "$resp"

# 5. Status query → APPLIED
echo "[5] GET /writes/{store}/{key} → APPLIED"
sleep 3
for idem in "$IDEM1" "$IDEM2"; do
    deadline=$(($(date +%s) + 15))
    applied=false
    while [ "$(date +%s)" -lt "$deadline" ]; do
        status_resp=$(ixmati_status "$IXMATI_STORE" "$idem")
        st=$(echo "$status_resp" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','PENDING'))" 2>/dev/null || echo "PENDING")
        if [ "$st" = "APPLIED" ]; then
            applied=true
            break
        fi
        sleep 0.5
    done
    if $applied; then
        green "  [PASS] status $idem → APPLIED"
        PASS=$((PASS + 1))
    else
        red "  [FAIL] status $idem → PENDING after 15s"
        FAIL=$((FAIL + 1))
    fi
done

# 6. MQTT event
echo "[6] MQTT event recibido"
if python3 -c "import paho.mqtt.client" 2>/dev/null; then
    IDEM3=$(uuidgen)
    ixmati_write "$IXMATI_STORE" "e2e" "e2e-mqtt" 1 "accepted" "$IDEM3" '{"data":"mqtt-test"}'
    mqtt_result=$(python3 -c "
import json, time, uuid, paho.mqtt.client as mqtt, sys

received = []
def on_message(c, u, msg):
    received.append(json.loads(msg.payload.decode()))

c = mqtt.Client(client_id=f'e2e-mqtt-{uuid.uuid4().hex[:8]}')
c.on_message = on_message
c.connect('${IXMATI_HOST}', ${IXMATI_MQTT_PORT})
c.subscribe('ixmati/evt/${IXMATI_STORE}/#', qos=1)
c.loop_start()
deadline = time.monotonic() + 10
while time.monotonic() < deadline and not received:
    time.sleep(0.05)
c.loop_stop()
c.disconnect()
print(len(received))
" 2>/dev/null)
    if [ "${mqtt_result:-0}" -ge 1 ]; then
        green "  [PASS] evento MQTT recibido"
        PASS=$((PASS + 1))
    else
        red "  [FAIL] no se recibió evento MQTT (timeout 10s)"
        FAIL=$((FAIL + 1))
    fi
else
    yellow "  [SKIP] paho-mqtt no instalado"
fi

# 7. Idempotency
echo "[7] Idempotency: mismo key → no duplica"
IDEM_DUP="ixmati-e2e-dup-$(date +%s)"
for i in 1 2; do
    ixmati_write "$IXMATI_STORE" "e2e" "dup-key" 1 "accepted" "$IDEM_DUP" '{"data":"dup"}' > /dev/null
    sleep 0.3
done
sleep 2
status_resp=$(ixmati_status "$IXMATI_STORE" "$IDEM_DUP")
st=$(echo "$status_resp" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','ERROR'))" 2>/dev/null || echo "ERROR")
assert_eq "Idempotency key → APPLIED" "APPLIED" "$st"

# 8. Health endpoint public
echo "[8] GET /health sin auth → OK (endpoint público)"
health_public=$(curl -s "${API_BASE}/health" -H "Authorization: invalid-key")
assert_json "GET /health sin auth válida → OK" "['overall']" "OK" "$health_public"

show_results
