"""Smoke test: lag de proyeccion."""

import pytest


@pytest.mark.smoke
class TestProjectionLag:
    def test_projection_lag_under_500ms(self):
        """El lag de proyeccion p99 debe ser < 500ms en condiciones normales."""
        # TODO: medir tiempo desde evento publicado hasta read model actualizado
        pass

    def test_projection_idempotent(self):
        """Re-entrega del mismo event_id no produce duplicados en el read model."""
        # TODO: publicar mismo event_id 3 veces, verificar 1 sola actualizacion
        pass
