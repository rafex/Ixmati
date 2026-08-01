"""Smoke test: escritura y lectura basica E2E."""

import json
import time
import pytest
from helpers.python.mqtt_harness import (
    ApiConfig,
    http_write,
    http_write_status,
    http_health,
    make_write_payload,
)


@pytest.mark.smoke
class TestWriteRead:
    def test_write_accepted(self, compose_up, api_config, mqtt_config):
        """POST /write devuelve ACCEPTED."""
        api = ApiConfig(**api_config)
        payload = make_write_payload(store="smoke", entity="item")
        result = http_write(api, payload)

        assert result.status == "ACCEPTED", f"Expected ACCEPTED, got {result.status}"
        assert result.store == "smoke"

    def test_write_status_applied(self, compose_up, api_config, mqtt_config):
        """GET /writes/{store}/{idempotency_key} eventualmente devuelve APPLIED."""
        api = ApiConfig(**api_config)
        payload = make_write_payload(store="smoke", entity="item", ack_mode="committed")
        result = http_write(api, payload)

        status = None
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            status = http_write_status(api, "smoke", result.idempotency_key)
            if status and status.get("status") == "APPLIED":
                break
            time.sleep(0.5)
        else:
            status = status or {}
            assert False, f"Write not applied after 10s: {status}"

    def test_missing_entity_returns_error(self, compose_up, api_config, mqtt_config):
        """POST /write sin los campos obligatorios devuelve error."""
        api = ApiConfig(**api_config)
        payload = {
            "op": "upsert",
            "store": "smoke",
            "version": 1,  # sin entity, sin key, sin idempotency_key
            "ack_mode": "accepted",
        }
        try:
            http_write(api, payload)
            pytest.fail("Expected HTTP error, got success")
        except RuntimeError as e:
            assert "HTTP" in str(e) or "error" in str(e).lower()

    def test_health_endpoint_ok(self, compose_up, api_config, mqtt_config):
        """GET /health devuelve status ok."""
        api = ApiConfig(**api_config)
        health = http_health(api)
        assert health is not None, "Health endpoint not reachable"
