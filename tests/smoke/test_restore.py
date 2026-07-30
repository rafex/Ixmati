"""Smoke test: restore desde Litestream."""

import pytest


@pytest.mark.smoke
class TestRestore:
    def test_litestream_restore_integrity(self):
        """Restaurar un store desde Litestream y verificar integridad."""
        # TODO: requiere litestream instalado y S3 configurado
        pass

    def test_restore_rpo_under_5s(self):
        """RPO < 5 segundos tras restore."""
        # TODO: medir diferencia entre ultimo dato escrito y dato restaurado
        pass
