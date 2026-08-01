"""Smoke test: durabilidad ante crash del writer."""

import subprocess
import time
from pathlib import Path
import pytest
from helpers.python.mqtt_harness import (
    ApiConfig,
    http_write,
    http_write_status,
    make_write_payload,
)


COMPOSE_FILE = Path(__file__).resolve().parent.parent.parent / "containers" / "compose" / "smoke.yaml"


@pytest.mark.smoke
class TestCrashDurability:
    def test_commands_survive_writer_kill9(self, compose_up, api_config, mqtt_config):
        """Tras kill del writer y reinicio, los comandos se recuperan."""
        api = ApiConfig(**api_config)

        progress_payload = make_write_payload(
            store="smoke", entity="crash_test", key="progress", version=1
        )
        http_write(api, progress_payload)

        subprocess.run(
            ["podman", "compose", "-f", str(COMPOSE_FILE), "kill", "writer"],
            check=True,
        )

        time.sleep(2)

        subprocess.run(
            ["podman", "compose", "-f", str(COMPOSE_FILE), "start", "writer"],
            check=True,
        )

        time.sleep(3)

        post_crash_payload = make_write_payload(
            store="smoke", entity="crash_test", key="post_crash", version=1
        )
        post_crash_result = http_write(api, post_crash_payload)

        assert post_crash_result.status == "ACCEPTED", (
            f"Writer not responsive after restart: {post_crash_result}"
        )

    def test_writer_recovers_after_restart(self, compose_up, api_config, mqtt_config):
        """Reiniciar el writer no genera problemas de conexion."""
        api = ApiConfig(**api_config)

        subprocess.run(
            ["podman", "compose", "-f", str(COMPOSE_FILE), "restart", "writer"],
            check=True,
        )

        time.sleep(3)

        payload = make_write_payload(
            store="smoke", entity="restart_test", key="r1", version=1
        )
        result = http_write(api, payload)

        assert result.status == "ACCEPTED", f"Writer stuck after restart: {result}"
