#!/usr/bin/env uv run
# helpers/python/lag_probe.py — medicion de lag de outbox y proyeccion

import sqlite3
import sys
import time
from pathlib import Path


def probe_outbox_lag(db_path: str) -> int | None:
    """Numero de eventos en _outbox sin publicar."""
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    row = conn.execute(
        "SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL"
    ).fetchone()
    conn.close()
    return row[0] if row else None


def probe_projection_lag(
    db_path: str,
    event_table: str = "_projection_state",
    max_event_age_sec: int = 60,
) -> int | None:
    """Eventos sin procesar por el proyector (basado en edad max)."""
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    row = conn.execute(
        f"SELECT COUNT(*) FROM {event_table} WHERE processed_at IS NULL"
    ).fetchone()
    conn.close()
    return row[0] if row else None


def main():
    if len(sys.argv) < 2:
        print("uso: lag_probe.py <store> [--outbox | --projection]")
        print("  store: ruta al archivo .db del store")
        return 1

    db_path = sys.argv[1]
    mode = sys.argv[2] if len(sys.argv) > 2 else "--outbox"

    lag = None
    if mode == "--outbox":
        lag = probe_outbox_lag(db_path)
        label = "outbox lag"
    elif mode == "--projection":
        lag = probe_projection_lag(db_path)
        label = "projection lag"
    else:
        print(f"modo desconocido: {mode}")
        return 1

    if lag is None:
        print(f"[lag] no se pudo medir ({db_path})")
        return 1

    print(f"[lag] {label}: {lag} pendientes (store={db_path})")
    return 0 if lag < 100 else 1


if __name__ == "__main__":
    sys.exit(main())
