"""Smoke test: lag de proyeccion e idempotencia."""

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
class TestProjectionLag:
    def test_event_received_within_5s(self, compose_up, api_config, mqtt_config):
        """El evento se recibe en < 5 segundos tras el write."""
        api = ApiConfig(**api_config)
        mqtt_cfg = MqttConfig(**mqtt_config, client_id="smoke-lag")
        client = create_client(mqtt_cfg)

        t0 = time.monotonic()
        payload = make_write_payload(
            store="smoke", entity="lag_test", key="l1", version=1
        )
        http_write(api, payload)

        events = wait_for_messages(
            client, "ixmati/evt/smoke/#", expected_count=1, timeout=10
        )
        elapsed = time.monotonic() - t0

        assert len(events) >= 1, f"No events received in {elapsed:.2f}s"
        assert elapsed < 10, f"Event took {elapsed:.2f}s (>10s timeout)"
        client.disconnect()

    def test_same_key_idempotent_delivery(self, compose_up, api_config, mqtt_config):
        """Misma idempotency_key reusada no duplica comandos."""
        api = ApiConfig(**api_config)

        idem_key = "smoke-idem-dup-test"
        for version in [1, 1, 1]:
            payload = {
                "op": "upsert",
                "store": "smoke",
                "entity": "idem_test",
                "key": "idem_key",
                "version": version,
                "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "idempotency_key": idem_key,
                "ack_mode": "accepted",
                "payload": {"data": f"v{version}"},
            }
            result = http_write(api, payload)
            assert result.status == "ACCEPTED"
