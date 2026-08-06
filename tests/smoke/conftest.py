"""Fixtures compartidos para smoke tests contra podman compose."""

import os
import re
import subprocess
import time
from pathlib import Path
from typing import Optional

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
COMPOSE_FILE = REPO_ROOT / "containers" / "compose" / "smoke.yaml"
COMPOSE_MULTI = REPO_ROOT / "containers" / "compose" / "multi-store.yaml"
TUNNEL_SCRIPT = REPO_ROOT / "helpers" / "shell" / "podman_tunnel.sh"


def _resolve_smoke_host() -> str:
    host = os.environ.get("SMOKE_HOST", "")
    if host:
        return host
    try:
        text = TUNNEL_SCRIPT.read_text()
        m = re.search(r'SSH_HOST="([^"]*@)?([^"]+)"', text)
        if m:
            return m.group(2)
    except Exception:
        pass
    return "localhost"


def _resolve_ssh_host() -> Optional[str]:
    host = os.environ.get("SMOKE_SSH_HOST", "")
    if host:
        return host
    return None


SMOKE_HOST = _resolve_smoke_host()
SSH_HOST = _resolve_ssh_host()


def run_podman(args: list[str], timeout: int = 30) -> subprocess.CompletedProcess:
    if SSH_HOST:
        import shlex
        remote_cmd = (
            "cd ~/Ixmati && podman compose -f containers/compose/smoke.yaml "
            + " ".join(shlex.quote(a) for a in args)
        )
        return subprocess.run(
            ["ssh", "-o", "ConnectTimeout=10", SSH_HOST, remote_cmd],
            capture_output=True, text=True, timeout=timeout,
        )
    return subprocess.run(
        ["podman", "compose", "-f", str(COMPOSE_FILE)] + args,
        capture_output=True, text=True, timeout=timeout,
    )


@pytest.fixture(scope="session")
def repo_root() -> Path:
    return REPO_ROOT


@pytest.fixture(scope="session")
def compose_up(repo_root: Path):
    """Levanta el stack smoke.yaml con podman compose y lo derriba al final.

    Si SMOKE_HOST esta definido, asume stack externo y solo verifica conectividad.
    Si no, intenta gestionar el compose via podman_tunnel.sh.
    """
    if os.environ.get("SMOKE_HOST"):
        mqtt_port = int(os.environ.get("SMOKE_MQTT_PORT", "30310"))
        api_port = int(os.environ.get("SMOKE_API_PORT", "30311"))
        _wait_for_port(SMOKE_HOST, mqtt_port, timeout=30)
        _wait_for_port(SMOKE_HOST, api_port, timeout=30)
        yield
        return

    tunnel = repo_root / "helpers" / "shell" / "podman_tunnel.sh"
    result = subprocess.run(
        [str(tunnel), "status"],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        pytest.skip(f"Podman tunnel inactivo: {result.stdout.strip()}. Ejecuta: just podman-tunnel-up")

    subprocess.run(
        ["podman", "compose", "-f", str(COMPOSE_FILE), "up", "-d"],
        check=True, cwd=repo_root,
    )

    _wait_for_port(SMOKE_HOST, 30310, timeout=30)
    _wait_for_port(SMOKE_HOST, 30311, timeout=30)

    yield

    subprocess.run(
        ["podman", "compose", "-f", str(COMPOSE_FILE), "down", "-v"],
        check=True, cwd=repo_root,
    )


@pytest.fixture(scope="session")
def compose_up_multi(repo_root: Path):
    """Levanta el stack multi-store.yaml con podman compose y lo derriba al final.

    Requiere podman compose disponible. Usa puertos directos: MQTT 30200, API 30000.
    """
    mqtt_port = int(os.environ.get("E2E_MQTT_PORT", "30200"))
    api_port = int(os.environ.get("E2E_API_PORT", "30000"))
    host = os.environ.get("E2E_HOST", "localhost")

    if os.environ.get("E2E_EXTERNAL"):
        _wait_for_port(host, mqtt_port, timeout=30)
        _wait_for_port(host, api_port, timeout=30)
        yield
        return

    subprocess.run(
        ["podman", "compose", "-f", str(COMPOSE_MULTI), "down", "-v"],
        capture_output=True, cwd=repo_root,
    )

    subprocess.run(
        ["podman", "compose", "-f", str(COMPOSE_MULTI), "up", "-d"],
        check=True, cwd=repo_root,
    )

    _wait_for_port(host, mqtt_port, timeout=60)
    _wait_for_port(host, api_port, timeout=60)

    yield

    subprocess.run(
        ["podman", "compose", "-f", str(COMPOSE_MULTI), "down", "-v"],
        check=False, cwd=repo_root,
    )


@pytest.fixture
def mqtt_config_multi():
    return {
        "host": os.environ.get("E2E_HOST", "localhost"),
        "port": int(os.environ.get("E2E_MQTT_PORT", "30200")),
        "qos": 1,
    }


@pytest.fixture
def api_config_multi():
    return {
        "host": os.environ.get("E2E_HOST", "localhost"),
        "port": int(os.environ.get("E2E_API_PORT", "30000")),
        "key": os.environ.get("SMOKE_API_KEY", "ix-default-key"),
    }


@pytest.fixture
def mqtt_config():
    return {
        "host": SMOKE_HOST,
        "port": int(os.environ.get("SMOKE_MQTT_PORT", "30310")),
        "qos": 1,
    }


@pytest.fixture
def api_config():
    return {
        "host": SMOKE_HOST,
        "port": int(os.environ.get("SMOKE_API_PORT", "30311")),
        "key": os.environ.get("SMOKE_API_KEY", "smoke-test-key"),
    }


@pytest.fixture
def smoke_store():
    return os.environ.get("SMOKE_STORE", "smoke")


def _wait_for_port(host, port, timeout=30):
    import socket
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            sock = socket.create_connection((host, port), timeout=1)
            sock.close()
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
