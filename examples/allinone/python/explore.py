#!/usr/bin/env python3
"""
explore.py — explorador interactivo del all-in-one Ixmati

Uso:
    python3 explore.py
    SMOKE_HOST=192.168.3.175 SMOKE_API_PORT=30080 python3 explore.py

Requiere: pip install -r python/requirements.txt
"""

import json
import os
import sys
import time
import uuid
import urllib.request
import urllib.error
from typing import Any

HOST = os.environ.get("IXMATI_HOST", os.environ.get("SMOKE_HOST", "localhost"))
API_PORT = int(os.environ.get("IXMATI_API_PORT", os.environ.get("SMOKE_API_PORT", "30080")))
MQTT_PORT = int(os.environ.get("IXMATI_MQTT_PORT", os.environ.get("SMOKE_MQTT_PORT", "30200")))
API_KEY = os.environ.get("IXMATI_API_KEY", os.environ.get("SMOKE_API_KEY", "smoke-test-key"))
STORE = os.environ.get("IXMATI_STORE", os.environ.get("SMOKE_STORE", "default"))

API_BASE = f"http://{HOST}:{API_PORT}"
AUTH = f"Bearer {API_KEY}"


def ok(msg: str) -> None:
    print(f"  \033[32m✓\033[0m {msg}")


def warn(msg: str) -> None:
    print(f"  \033[33m⚠\033[0m {msg}")


def die(msg: str) -> None:
    print(f"  \033[31m✗\033[0m {msg}")


def api_get(path: str) -> dict:
    req = urllib.request.Request(f"{API_BASE}{path}")
    with urllib.request.urlopen(req, timeout=5) as r:
        return json.loads(r.read())


def api_post(path: str, data: dict) -> dict:
    body = json.dumps(data).encode()
    req = urllib.request.Request(
        f"{API_BASE}{path}",
        data=body,
        headers={
            "Content-Type": "application/json",
            "Authorization": AUTH,
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=5) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        error_body = e.read().decode("utf-8", errors="replace").strip()
        return {"error": True, "code": e.code, "body": error_body}


def health() -> None:
    print("\n--- Health Check ---")
    h = api_get("/health")
    print(json.dumps(h, indent=2))
    if h.get("overall") == "OK":
        ok("Todos los componentes OK")
    else:
        warn("Algún componente degradado")


def do_write() -> None:
    print("\n--- Write Command ---")
    store = input(f"  store [{STORE}]: ").strip() or STORE
    entity = input("  entity [test]: ").strip() or "test"
    key = input("  key [auto]: ").strip() or f"explore-{uuid.uuid4().hex[:8]}"
    ack = input("  ack_mode [accepted]: ").strip() or "accepted"
    payload_str = input('  payload [{"data":"hello"}]: ').strip() or '{"data":"hello"}'

    try:
        payload = json.loads(payload_str)
    except json.JSONDecodeError:
        payload = {"data": payload_str}

    ik = str(uuid.uuid4())
    cmd = {
        "op": "upsert",
        "store": store,
        "entity": entity,
        "key": key,
        "version": 1,
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "idempotency_key": ik,
        "ack_mode": ack,
        "payload": payload,
    }

    resp = api_post("/write", cmd)
    if resp.get("error"):
        die(f"HTTP {resp['code']}: {resp['body']}")
    else:
        print(f"  Response: {json.dumps(resp, indent=2)}")
        ok(f"Write ACCEPTED — idempotency_key: {ik}")


def check_status() -> None:
    print("\n--- Check Write Status ---")
    store = input(f"  store [{STORE}]: ").strip() or STORE
    ik = input("  idempotency_key: ").strip()
    if not ik:
        print("  idempotency_key requerido")
        return

    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        try:
            resp = api_get(f"/writes/{store}/{ik}")
            print(f"  {json.dumps(resp, indent=2)}")
            if resp.get("status") == "APPLIED":
                ok("Write APPLIED")
                return
            print("  ... esperando APPLIED")
            time.sleep(1)
        except Exception as e:
            die(f"Error: {e}")
            return
    warn("Timeout: write no aplicado tras 15s")


def subscribe_events() -> None:
    print("\n--- Subscribe to Events (Ctrl+C para salir) ---")
    try:
        import paho.mqtt.client as mqtt
    except ImportError:
        die("paho-mqtt no instalado: pip install paho-mqtt")
        return

    store = input(f"  store [{STORE}]: ").strip() or STORE
    topic = f"ixmati/evt/{store}/#"

    received: list[dict] = []

    def on_message(_c: Any, _u: Any, msg: Any) -> None:
        try:
            evt = json.loads(msg.payload.decode())
            received.append(evt)
            print(f"\n[{time.strftime('%H:%M:%S')}] {msg.topic}")
            print(json.dumps(evt, indent=2))
        except Exception:
            print(f"\n[raw] {msg.payload.decode()}")

    client = mqtt.Client(client_id=f"explore-sub-{uuid.uuid4().hex[:8]}")
    client.on_message = on_message
    client.connect(HOST, MQTT_PORT)
    client.subscribe(topic, qos=1)
    ok(f"Suscrito a {topic}")
    print("  Esperando eventos... Ctrl+C para salir")
    try:
        client.loop_forever()
    except KeyboardInterrupt:
        client.disconnect()
        print(f"\n  {len(received)} eventos recibidos")


def stress_test() -> None:
    print("\n--- Stress Test ---")
    try:
        n = int(input("  Número de writes [100]: ").strip() or "100")
    except ValueError:
        n = 100

    ok_ids = []
    fail_ids = []
    t0 = time.monotonic()

    from concurrent.futures import ThreadPoolExecutor, as_completed

    def _write_one(i: int) -> tuple[int, bool]:
        ik = f"stress-{i}"
        cmd = {
            "op": "upsert",
            "store": STORE,
            "entity": "stress",
            "key": f"s{i}",
            "version": 1,
            "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "idempotency_key": ik,
            "ack_mode": "accepted",
            "payload": {"i": i},
        }
        try:
            resp = api_post("/write", cmd)
            return i, resp.get("status") == "ACCEPTED"
        except Exception:
            return i, False

    with ThreadPoolExecutor(max_workers=10) as pool:
        futures = [pool.submit(_write_one, i) for i in range(n)]
        for fut in as_completed(futures):
            i, ok_result = fut.result()
            if ok_result:
                ok_ids.append(i)
            else:
                fail_ids.append(i)

    elapsed = time.monotonic() - t0
    rate = n / elapsed if elapsed > 0 else 0

    print(f"  Enviados: {n}")
    print(f"  Aceptados: {len(ok_ids)}")
    print(f"  Fallados:  {len(fail_ids)}")
    print(f"  Tiempo:    {elapsed:.2f}s")
    print(f"  Rate:      {rate:.1f} writes/s")

    if len(fail_ids) == 0:
        ok(f"Todos aceptados")
    else:
        warn(f"{len(fail_ids)} fallos")


def view_metrics() -> None:
    print("\n--- Metrics (Prometheus) ---")
    try:
        resp = urllib.request.urlopen(f"{API_BASE}/metrics", timeout=5)
        text = resp.read().decode()
        lines = text.strip().split("\n")
        for line in lines:
            if not line.startswith("#"):
                print(f"  {line}")
        ok(f"Total: {len([l for l in lines if not l.startswith('#')])} métricas")
    except Exception as e:
        die(f"Error: {e}")


def menu() -> None:
    while True:
        print(f"""
\033[1m=== Ixmati All-in-One Explorer ===\033[0m
  API:   {API_BASE}
  Store: {STORE}

  [1] Health check
  [2] Write command
  [3] Check write status
  [4] Subscribe to events (live)
  [5] Stress test (N concurrent writes)
  [6] View metrics
  [7] Run e2e scenarios (1-7)
  [q] Quit
""")
        choice = input("  > ").strip().lower()

        if choice == "1":
            health()
        elif choice == "2":
            do_write()
        elif choice == "3":
            check_status()
        elif choice == "4":
            subscribe_events()
        elif choice == "5":
            stress_test()
        elif choice == "6":
            view_metrics()
        elif choice == "7":
            run_scenarios()
        elif choice in ("q", "quit", "exit"):
            print("  ¡Hasta luego!")
            break
        elif not choice:
            continue
        else:
            print(f"  Opción no válida: {choice}")


def run_scenarios() -> None:
    import subprocess
    from pathlib import Path

    scenarios_dir = Path(__file__).resolve().parent.parent / "scenarios"
    scripts = sorted(scenarios_dir.glob("0[1-7]-*.sh"))
    if not scripts:
        warn("No se encontraron escenarios en scenarios/")
        return

    print(f"\n--- Escenarios ({len(scripts)}) ---")
    for s in scripts:
        print(f"  {s.name}")

    sel = input("\n  Ejecutar todos? [S/n]: ").strip().lower()
    if sel in ("n", "no"):
        return

    env = os.environ.copy()
    env["IXMATI_HOST"] = HOST
    env["IXMATI_API_PORT"] = str(API_PORT)
    env["IXMATI_MQTT_PORT"] = str(MQTT_PORT)
    env["IXMATI_API_KEY"] = API_KEY
    env["IXMATI_STORE"] = STORE

    passed = 0
    failed = 0
    for s in scripts:
        print(f"\n\033[1m--- {s.name} ---\033[0m")
        result = subprocess.run(["bash", str(s)], env=env, capture_output=False)
        if result.returncode == 0:
            passed += 1
        else:
            failed += 1

    print(f"\n  Resultado: {passed}/{len(scripts)} ok, {failed} fallos")


def main() -> None:
    print(f"\033[34mIxmati All-in-One Explorer\033[0m")
    print(f"  API:  {API_BASE}")
    print(f"  MQTT: {HOST}:{MQTT_PORT}")
    print(f"  Key:  {API_KEY}")
    print()

    try:
        h = api_get("/health")
        if h.get("overall") == "OK":
            ok("Conexión establecida")
        else:
            warn(f"Sistema reporta: {h.get('overall', '?')}")
    except Exception as e:
        die(f"No se puede conectar: {e}")
        sys.exit(1)

    menu()


if __name__ == "__main__":
    main()
