"""Smoke test: outbox transaccional — 0 eventos perdidos."""

import time
import pytest
from helpers.python.mqtt_harness import (
    ApiConfig,
    MqttConfig,
    create_client,
    http_write,
    make_write_payload,
    wait_for_messages,
)


@pytest.mark.smoke
class TestOutbox:
    def test_events_published_after_write(
        self, compose_up, api_config, mqtt_config, smoke_store
    ):
        """N comandos escritos producen N eventos en ixmati/evt/{smoke_store}/#."""
        api = ApiConfig(**api_config)
        mqtt_cfg = MqttConfig(**mqtt_config, client_id="smoke-outbox")
        client = create_client(mqtt_cfg)

        num_writes = 5
        written_keys = []
        for i in range(num_writes):
            payload = make_write_payload(
                store=smoke_store, entity=f"outbox_{i}", key=f"k_{i}"
            )
            result = http_write(api, payload)
            written_keys.append(result.idempotency_key)

        events = wait_for_messages(
            client,
            f"ixmati/evt/{smoke_store}/#",
            expected_count=num_writes,
            timeout=30,
        )

        assert (
            len(events) >= num_writes
        ), f"Expected {num_writes} events, got {len(events)}"
        client.disconnect()

    def test_outbox_eventual_consistency(
        self, compose_up, api_config, mqtt_config, smoke_store
    ):
        """Tras escribir con ack_mode=committed, el evento llega."""
        api = ApiConfig(**api_config)
        mqtt_cfg = MqttConfig(**mqtt_config, client_id="smoke-consistency")
        client = create_client(mqtt_cfg)

        payload = make_write_payload(
            store=smoke_store, entity="consistency", key="c1", ack_mode="committed"
        )
        http_write(api, payload)

        events = wait_for_messages(
            client, f"ixmati/evt/{smoke_store}/#", expected_count=1, timeout=10
        )

        assert len(events) >= 1, f"No events received: {events}"
        client.disconnect()
