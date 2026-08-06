#!/usr/bin/env uv run
# helpers/python/mqtt_harness.py — publish/subscribe + HTTP client para smoke tests

"""Fixture reutilizable para tests de smoke sobre MQTT y API REST."""

import json
import time
import uuid
import urllib.request
import urllib.error
from dataclasses import dataclass, field

from typing import Optional

try:
    import paho.mqtt.client as mqtt
except ImportError:
    mqtt = None  # type: ignore


@dataclass
class MqttConfig:
    host: str = "localhost"
    port: int = 1883
    client_id: str = "ixmati-harness"
    qos: int = 1


@dataclass
class ApiConfig:
    host: str = "localhost"
    port: int = 30000
    key: str = ""


@dataclass
class WriteResult:
    status: str
    store: str
    idempotency_key: str
    message: Optional[str] = None


def create_client(config: Optional[MqttConfig] = None) -> "mqtt.Client":
    if mqtt is None:
        raise RuntimeError("paho-mqtt no instalado. Ejecuta: uv sync")
    cfg = config or MqttConfig()
    client_id = f"{cfg.client_id}-{uuid.uuid4().hex[:8]}"
    client = mqtt.Client(client_id=client_id)
    client.connect(cfg.host, cfg.port)
    return client


def publish(
    client: "mqtt.Client",
    topic: str,
    payload: dict,
    qos: int = 1,
) -> None:
    client.publish(topic, json.dumps(payload), qos=qos)


def wait_for_message(
    client: "mqtt.Client",
    topic: str,
    timeout: float = 5.0,
) -> Optional[dict]:
    received = []

    def on_message(_client, _userdata, msg):
        received.append(json.loads(msg.payload.decode()))

    client.on_message = on_message
    client.subscribe(topic, qos=1)

    client.loop_start()
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline and not received:
        time.sleep(0.05)
    client.loop_stop()
    client.unsubscribe(topic)

    return received[0] if received else None


def wait_for_messages(
    client: "mqtt.Client",
    topic: str,
    expected_count: int,
    timeout: float = 10.0,
) -> list[dict]:
    """Espera N mensajes en un topic."""
    received: list[dict] = []

    def on_message(_client, _userdata, msg):
        received.append(json.loads(msg.payload.decode()))

    client.on_message = on_message
    client.subscribe(topic, qos=1)

    client.loop_start()
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline and len(received) < expected_count:
        time.sleep(0.05)
    client.loop_stop()
    client.unsubscribe(topic)

    return received


def make_write_payload(
    store: str = "test",
    entity: str = "test",
    key: Optional[str] = None,
    version: int = 1,
    ack_mode: str = "accepted",
) -> dict:
    return {
        "op": "upsert",
        "store": store,
        "entity": entity,
        "key": key or f"key-{uuid.uuid4().hex[:12]}",
        "version": version,
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "idempotency_key": str(uuid.uuid4()),
        "ack_mode": ack_mode,
        "payload": {"data": "test", "ts": time.time()},
    }


def http_write(api: ApiConfig, payload: dict) -> WriteResult:
    """POST /write"""
    url = f"http://{api.host}:{api.port}/write"
    data = json.dumps(payload).encode()
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api.key}",
    }
    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            body = json.loads(resp.read())
            return WriteResult(
                status=body.get("status", ""),
                store=body.get("store", ""),
                idempotency_key=body.get("idempotency_key", ""),
                message=body.get("message"),
            )
    except urllib.error.HTTPError as e:
        error_body = e.read().decode("utf-8", errors="replace").strip()
        try:
            error_json = json.loads(error_body)
        except (json.JSONDecodeError, ValueError):
            error_json = {"detail": error_body}
        raise RuntimeError(f"HTTP {e.code}: {error_json}")


def http_write_status(api: ApiConfig, store: str, idempotency_key: str) -> Optional[dict]:
    """GET /writes/{store}/{idempotency_key}"""
    url = f"http://{api.host}:{api.port}/writes/{store}/{idempotency_key}"
    headers = {"Authorization": f"Bearer {api.key}"}
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        return None


def http_read(api: ApiConfig, store: str, entity: Optional[str] = None, key: Optional[str] = None) -> Optional[dict]:
    """GET /read"""
    params = [("store", store)]
    if entity:
        params.append(("entity", entity))
    if key:
        params.append(("key", key))
    query = "&".join(f"{k}={v}" for k, v in params)
    url = f"http://{api.host}:{api.port}/read?{query}"
    headers = {"Authorization": f"Bearer {api.key}"}
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError:
        return None


def http_health(api: ApiConfig) -> Optional[dict]:
    """GET /health"""
    url = f"http://{api.host}:{api.port}/health"
    try:
        with urllib.request.urlopen(url, timeout=5) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError:
        return None


def http_read_projection(api: ApiConfig, projection: str, key: str) -> Optional[dict]:
    """GET /read?projection={projection}&key={key}"""
    url = f"http://{api.host}:{api.port}/read?projection={projection}&key={key}"
    headers = {"Authorization": f"Bearer {api.key}"}
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        error_body = e.read().decode("utf-8", errors="replace").strip()
        try:
            return json.loads(error_body)
        except (json.JSONDecodeError, ValueError):
            return {"found": False, "message": error_body}
