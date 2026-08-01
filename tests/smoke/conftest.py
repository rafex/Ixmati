"""Fixtures compartidos para smoke tests contra podman compose."""

import subprocess
import time
from pathlib import Path
import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
COMPOSE_FILE = REPO_ROOT / "containers" / "compose" / "smoke.yaml"


@pytest.fixture(scope="session")
def repo_root() -> Path:
    return REPO_ROOT


@pytest.fixture(scope="session")
def compose_up(repo_root: Path):
    """Levanta el stack smoke.yaml con podman compose y lo derriba al final."""
    tunnel = repo_root / "helpers" / "shell" / "podman_tunnel.sh"
    result = subprocess.run(
        [str(tunnel), "status"],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        pytest.skip(f"Podman tunnel inactivo: {result.stdout.strip()}. Ejecuta: just podman-tunnel-up")

    subprocess.run(
        ["podman", "compose", "-f", str(COMPOSE_FILE), "up", "-d", "--build"],
        check=True, cwd=repo_root,
    )

    _wait_for_port("localhost", 30310, timeout=30)
    _wait_for_port("localhost", 30311, timeout=30)

    yield

    subprocess.run(
        ["podman", "compose", "-f", str(COMPOSE_FILE), "down", "-v"],
        check=True, cwd=repo_root,
    )


@pytest.fixture
def mqtt_config():
    return {"host": "localhost", "port": 30310, "qos": 1}


@pytest.fixture
def api_config():
    return {"host": "localhost", "port": 30311, "key": "smoke-test-key"}


def _wait_for_port(host, port, timeout=30):
    import socket
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1):
                return
        except (ConnectionRefusedError, OSError):
            time.sleep(0.5)
    pytest.fail(f"Timeout esperando {host}:{port}")


def _wait_for_http(url, timeout=30):
    import urllib.request
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            urllib.request.urlopen(url, timeout=2)
            return
        except Exception:
            time.sleep(0.5)
    pytest.fail(f"Timeout esperando {url}")
