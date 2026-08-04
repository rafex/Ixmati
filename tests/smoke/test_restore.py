"""Smoke test: restore desde Litestream."""

import json
import subprocess
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


def _run_sqlite(store: str, sql: str) -> str:
    """Ejecuta una consulta SQL en el writer container."""
    result = run_podman(["exec", "-T", "writer", "sqlite3", f"/data/{store}.db", sql])
    if result.returncode != 0:
        return ""
    return result.stdout.strip()


@pytest.mark.smoke
class TestRestore:
    def test_sqlite_integrity_after_writes(self, compose_up, api_config, mqtt_config):
        """PRAGMA integrity_check pasa tras writes."""
        api = ApiConfig(**api_config)

        for i in range(3):
            payload = make_write_payload(
                store="smoke", entity="integrity", key=f"i_{i}", version=1
            )
            http_write(api, payload)

        time.sleep(2)

        output = _run_sqlite("smoke", "PRAGMA integrity_check;")
        assert "ok" in output.lower(), f"Integrity check failed: {output}"

    def test_journal_mode_is_wal(self, compose_up, api_config, mqtt_config):
        """El journal_mode de SQLite debe ser WAL."""
        output = _run_sqlite("smoke", "PRAGMA journal_mode;")
        assert output == "wal", f"Expected WAL, got: {output}"
