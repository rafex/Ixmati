"""Tests para helpers/python/ — validaciones y linting."""

import sys
import subprocess
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent


class TestLintToolBoundary:
    def test_no_just_in_makefile(self):
        """El Makefile y helpers/make/*.mk no deben contener invocaciones a just."""
        result = subprocess.run(
            ["uv", "run", "python", str(REPO_ROOT / "helpers" / "python" / "lint_tool_boundary.py")],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
        )
        assert result.returncode == 0, f"boundary lint fallo:\n{result.stdout}\n{result.stderr}"


class TestCoverageGate:
    def test_floor_file_exists(self):
        """El archivo .coverage-floor debe existir en la raiz."""
        floor_file = REPO_ROOT / ".coverage-floor"
        assert floor_file.exists(), ".coverage-floor no existe en la raiz"

    def test_floor_file_has_number(self):
        """El archivo .coverage-floor debe contener un numero."""
        floor_file = REPO_ROOT / ".coverage-floor"
        content = floor_file.read_text().strip()
        assert float(content) >= 0, f".coverage-floor debe ser un numero >= 0, encontrado: {content}"


class TestValidateConfig:
    def test_stores_example_valid(self):
        """El stores.test.toml de fixtures debe ser valido."""
        # TODO: ejecutar validate_config.py apuntando al archivo de fixtures
        pass

    def test_rejects_invalid_store_name(self):
        """Nombres de store con / deben ser rechazados."""
        pass
