#!/bin/bash
# shell-helpers.sh — funciones reutilizables para explorar Ixmati
# Uso: source shell-helpers.sh
#
# Variables de entorno:
#   IXMATI_HOST       Host API (default: localhost)
#   IXMATI_API_PORT   Puerto API (default: 30080)
#   IXMATI_MQTT_PORT  Puerto MQTT (default: 30200)
#   IXMATI_API_KEY    API key (default: smoke-test-key)
#   IXMATI_STORE      Store name (default: default)

export IXMATI_HOST="${IXMATI_HOST:-localhost}"
export IXMATI_API_PORT="${IXMATI_API_PORT:-30080}"
export IXMATI_MQTT_PORT="${IXMATI_MQTT_PORT:-30200}"
export IXMATI_API_KEY="${IXMATI_API_KEY:-smoke-test-key}"
export IXMATI_STORE="${IXMATI_STORE:-default}"

API_BASE="http://${IXMATI_HOST}:${IXMATI_API_PORT}"
AUTH_HEADER="Authorization: Bearer ${IXMATI_API_KEY}"

red()   { echo -e "\033[31m$1\033[0m"; }
green() { echo -e "\033[32m$1\033[0m"; }
yellow(){ echo -e "\033[33m$1\033[0m"; }
bold()  { echo -e "\033[1m$1\033[0m"; }

# --- API helpers ---

ixmati_health() {
    curl -s "${API_BASE}/health" | python3 -m json.tool 2>/dev/null || \
        curl -s "${API_BASE}/health"
}

ixmati_write() {
    local store="${1:-$IXMATI_STORE}"
    local entity="${2:-test}"
    local key="${3:-$(uuidgen | tr '[:upper:]' '[:lower:]')}"
    local version="${4:-1}"
    local ack_mode="${5:-accepted}"
    local idem_key="${6:-$(uuidgen)}"
    local payload="${7:-{\"data\":\"test\"}}"

    curl -s -X POST "${API_BASE}/write" \
        -H "Content-Type: application/json" \
        -H "$AUTH_HEADER" \
        -d "{
            \"op\": \"upsert\",
            \"store\": \"$store\",
            \"entity\": \"$entity\",
            \"key\": \"$key\",
            \"version\": $version,
            \"ts\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
            \"idempotency_key\": \"$idem_key\",
            \"ack_mode\": \"$ack_mode\",
            \"payload\": $payload
        }"
}

ixmati_status() {
    local store="${1:-$IXMATI_STORE}"
    local idem_key="$2"
    curl -s "${API_BASE}/writes/${store}/${idem_key}" \
        -H "$AUTH_HEADER"
}

ixmati_read() {
    local store="${1:-$IXMATI_STORE}"
    local entity="${2:-test}"
    local key="$3"
    curl -s "${API_BASE}/read?store=${store}&entity=${entity}&key=${key}" \
        -H "$AUTH_HEADER"
}

ixmati_metrics() {
    curl -s "${API_BASE}/metrics"
}

# --- MQTT helpers ---

ixmati_subscribe() {
    local topic="${1:-ixmati/evt/${IXMATI_STORE}/#}"
    local host="${IXMATI_HOST}"
    local port="${IXMATI_MQTT_PORT}"

    bold "Suscrito a ${topic} en ${host}:${port} (Ctrl+C para salir)"

    if command -v mosquitto_sub &>/dev/null; then
        mosquitto_sub -h "$host" -p "$port" -t "$topic" -q 1 -v
    elif command -v python3 &>/dev/null; then
        python3 -c "
import json, sys, time, uuid, paho.mqtt.client as mqtt
try:
    import paho.mqtt.client as mqtt
except ImportError:
    print('Instala paho-mqtt: pip install paho-mqtt', file=sys.stderr)
    sys.exit(1)

def on_connect(c, _u, _f, rc, _p):
    if rc == 0:
        c.subscribe('${topic}', qos=1)
        print(f'Conectado y suscrito a ${topic}', flush=True)

def on_message(_c, _u, msg):
    try:
        payload = json.loads(msg.payload.decode())
        print(f'[{payload.get(\"event_type\", \"?\")}] {json.dumps(payload, indent=2)}', flush=True)
    except Exception as e:
        print(f'[raw] {msg.payload.decode()}', flush=True)

c = mqtt.Client(client_id=f'ixmati-sub-{uuid.uuid4().hex[:8]}')
c.on_connect = on_connect
c.on_message = on_message
c.connect('${host}', ${port})
c.loop_forever()
" 2>/dev/null
    else
        red "Ni mosquitto_sub ni python3+paho-mqtt disponibles"
        echo "  Instala: apt install mosquitto-clients"
    fi
}

# --- assert helpers ---

assert_eq() {
    local label="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        green "  [PASS] $label"
        PASS=$((PASS + 1))
    else
        red "  [FAIL] $label"
        echo "    expected: $expected"
        echo "    actual:   $actual"
        FAIL=$((FAIL + 1))
    fi
}

assert_contains() {
    local label="$1" needle="$2" haystack="$3"
    if echo "$haystack" | grep -q "$needle"; then
        green "  [PASS] $label"
        PASS=$((PASS + 1))
    else
        red "  [FAIL] $label"
        echo "    buscando: $needle"
        echo "    en:       $haystack"
        FAIL=$((FAIL + 1))
    fi
}

assert_json() {
    local label="$1" query="$2" expected="$3" json_str="$4"
    local actual
    actual=$(echo "$json_str" | python3 -c "import sys,json; print(json.load(sys.stdin)${query})" 2>/dev/null || echo "PARSE_ERROR")
    assert_eq "$label" "$expected" "$actual"
}

# --- init counters ---
PASS=0
FAIL=0

show_results() {
    echo ""
    bold "=== Resultado: ${PASS}/$((PASS + FAIL)) tests ==="
    if [ "$FAIL" -gt 0 ]; then
        red "Algunos tests fallaron."
        return 1
    else
        green "Todos los tests pasaron!"
        return 0
    fi
}
