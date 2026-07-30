"""Smoke test: outbox transaccional."""

import pytest


@pytest.mark.smoke
class TestOutbox:
    def test_event_in_outbox_after_commit(self):
        """Tras un commit exitoso, el evento esta en la tabla _outbox."""
        # TODO: verificar que SELECT * FROM _outbox contiene el evento tras un comando aplicado
        pass

    def test_outbox_emptied_after_publish(self):
        """El publicador vacia _outbox tras publicar los eventos."""
        # TODO: verificar que _outbox WHERE published_at IS NULL = 0 tras esperar al publicador
        pass

    def test_outbox_survives_writer_restart(self):
        """Tras reiniciar el writer, los eventos pendientes en _outbox se publican."""
        # TODO: kill writer, verificar que _outbox tiene pendientes, reiniciar, verificar que se vacia
        pass
