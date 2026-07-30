"""Smoke test: durabilidad ante crash del writer."""

import pytest


@pytest.mark.smoke
class TestCrashDurability:
    def test_zero_commands_lost_after_kill9(self):
        """Tras kill -9 del writer, 0 comandos se pierden (Mosquitto persistence)."""
        # TODO: implementar con docker compose: publicar N comandos, kill writer, verificar SQLite
        pass

    def test_zero_events_lost_after_crash(self):
        """Tras crash entre commit y publish, 0 eventos se pierden (outbox transaccional)."""
        # TODO: implementar con docker compose: kill entre commit y publish, verificar _outbox se vacia
        pass
