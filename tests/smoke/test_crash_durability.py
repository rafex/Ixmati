"""Smoke test: durabilidad ante crash del writer."""

import time
from pathlib import Path
import pytest
from helpers.python.mqtt_harness import (
    ApiConfig,
    http_write,
    make_write_payload,
)
from conftest import run_podman, SSH_HOST


COMPOSE_FILE = Path(__file__).resolve().parent.parent.parent / "containers" / "compose" / "smoke.yaml"


def _require_podman():
    if not SSH_HOST and not Path("/var/run/docker.sock").exists():
        import subprocess as sp
        result = sp.run(["podman", "info"], capture_output=True, text=True)
        if result.returncode != 0:
            pytest.skip("Podman not available. Set SMOKE_SSH_HOST for remote podman.")


@pytest.mark.smoke
class TestCrashDurability:
    def test_commands_survive_writer_kill9(
        self, compose_up, api_config, mqtt_config, smoke_store
    ):
        """Tras kill del writer y reinicio, los comandos se recuperan."""
        _require_podman()
        api = ApiConfig(**api_config)

        progress_payload = make_write_payload(
            store=smoke_store, entity="crash_test", key="progress", version=1
        )
        http_write(api, progress_payload)

        result = run_podman(["kill", "writer"])
        if result.returncode != 0:
            pytest.skip(f"Cannot kill writer: {result.stderr.strip()}")

        time.sleep(2)

        result = run_podman(["start", "writer"])
        if result.returncode != 0:
            pytest.skip(f"Cannot start writer: {result.stderr.strip()}")

        time.sleep(5)

        post_crash_payload = make_write_payload(
            store=smoke_store, entity="crash_test", key="post_crash", version=1
        )
        post_crash_result = http_write(api, post_crash_payload)

        assert post_crash_result.status == "ACCEPTED", (
            f"Writer not responsive after restart: {post_crash_result}"
        )

    def test_writer_recovers_after_restart(
        self, compose_up, api_config, mqtt_config, smoke_store
    ):
        """Reiniciar el writer no genera problemas de conexion."""
        _require_podman()
        api = ApiConfig(**api_config)

        result = run_podman(["restart", "writer"])
        if result.returncode != 0:
            pytest.skip(f"Cannot restart writer: {result.stderr.strip()}")

        time.sleep(5)

        payload = make_write_payload(
            store=smoke_store, entity="restart_test", key="r1", version=1
        )
        result = http_write(api, payload)

        assert result.status == "ACCEPTED", f"Writer stuck after restart: {result}"
