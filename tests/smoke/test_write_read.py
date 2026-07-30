"""Smoke test: escritura y lectura basica."""

import pytest


@pytest.mark.smoke
class TestWriteRead:
    def test_write_envelope_validation(self):
        """Un envelope de comando debe tener los campos obligatorios."""
        # TODO: implementar cuando la API este disponible
        required_fields = ["op", "store", "entity", "key", "version", "idempotency_key", "ack_mode", "payload"]
        for field in required_fields:
            assert field  # placeholder

    def test_read_miss_returns_404(self):
        """Una lectura de clave inexistente debe devolver 404."""
        # TODO: implementar cuando la API este disponible
        assert True
