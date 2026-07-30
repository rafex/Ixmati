"""Fixtures compartidos para smoke tests (pytest)."""

import pytest
import subprocess
import time
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent.parent


@pytest.fixture(scope="session")
def repo_root() -> Path:
    return REPO_ROOT


@pytest.fixture(scope="session")
def docker_compose_up(repo_root: Path):
    """Levanta servicios de desarrollo y los detiene al finalizar la sesion."""
    compose_file = repo_root / "docker" / "docker-compose.dev.yml"
    if not compose_file.exists():
        pytest.skip("docker-compose.dev.yml no existe — omitiendo smoke tests")

    subprocess.run(
        ["docker", "compose", "-f", str(compose_file), "up", "-d"],
        check=True,
        cwd=repo_root,
    )
    time.sleep(3)  # esperar a que los servicios esten listos

    yield

    subprocess.run(
        ["docker", "compose", "-f", str(compose_file), "down"],
        check=True,
        cwd=repo_root,
    )


@pytest.fixture
def mqtt_config():
    """Configuracion MQTT para tests."""
    return {"host": "localhost", "port": 1883, "qos": 1}
