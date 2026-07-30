#!/usr/bin/env uv run
# helpers/python/mqtt_harness.py — publish/subscribe para smoke tests

"""Fixture reutilizable para tests de smoke sobre MQTT."""

import json
import time
import uuid
from dataclasses import dataclass

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


def create_client(config: MqttConfig | None = None) -> "mqtt.Client":
    if mqtt is None:
        raise RuntimeError("paho-mqtt no instalado. Ejecuta: uv sync")
    cfg = config or MqttConfig()
    client = mqtt.Client(client_id=f"{cfg.client_id}-{uuid.uuid4().hex[:8]}")
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
) -> dict | None:
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


def make_write_payload(
    store: str = "test",
    entity: str = "test",
    key: str | None = None,
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
        "payload": {"data": "test"},
    }
