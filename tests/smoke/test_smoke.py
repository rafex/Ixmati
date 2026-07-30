#!/usr/bin/env python3
"""Ixmati smoke tests — outbox durability, projection idempotency, crash recovery."""

import json
import sqlite3
import tempfile
from pathlib import Path
import uuid

TEMP_DIR = Path(tempfile.gettempdir()) / "ixmati-smoke-tests"
TEMP_DIR.mkdir(parents=True, exist_ok=True)

def test_db_creation():
    db = TEMP_DIR / "smoke.db"
    conn = sqlite3.connect(str(db))
    conn.execute("PRAGMA journal_mode=WAL;")
    conn.execute("""
        CREATE TABLE IF NOT EXISTS _idempotency (
            idempotency_key TEXT NOT NULL,
            store           TEXT NOT NULL,
            entity          TEXT NOT NULL,
            key             TEXT NOT NULL,
            version         INTEGER NOT NULL,
            applied_at      TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (store, idempotency_key)
        )
    """)
    conn.execute("INSERT INTO _idempotency(idempotency_key, store, entity, key, version) VALUES(?,?,?,?,?)",
                 ("ik-1", "pedidos", "pedido", "ped_1", 1))
    conn.commit()
    cursor = conn.execute("SELECT COUNT(*) FROM _idempotency")
    count = cursor.fetchone()[0]
    conn.close()
    assert count == 1, f"Expected 1 row, got {count}"

def test_crash_durability():
    db = TEMP_DIR / "crash.db"
    if db.exists():
        db.unlink()

    conn = sqlite3.connect(str(db))
    conn.execute("PRAGMA journal_mode=WAL;")
    conn.execute("CREATE TABLE IF NOT EXISTS test(id INTEGER PRIMARY KEY, data TEXT)")
    conn.execute("INSERT INTO test(id, data) VALUES(1, 'hello')")
    conn.commit()
    conn.close()

    conn2 = sqlite3.connect(str(db))
    row = conn2.execute("SELECT data FROM test WHERE id = 1").fetchone()
    conn2.close()
    assert row[0] == 'hello', f"WAL recovery failed: got {row}"

def test_outbox_eventual_publication():
    db = TEMP_DIR / "outbox.db"
    if db.exists():
        db.unlink()

    conn = sqlite3.connect(str(db))
    conn.execute("PRAGMA journal_mode=WAL;")
    conn.execute("""
        CREATE TABLE IF NOT EXISTS _outbox (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            store TEXT NOT NULL,
            entity TEXT NOT NULL,
            key TEXT NOT NULL,
            version INTEGER NOT NULL,
            occurred_at TEXT NOT NULL,
            payload BLOB NOT NULL,
            published_at TEXT
        )
    """)
    evt_id = str(uuid.uuid4())
    conn.execute(
        "INSERT INTO _outbox(event_id, event_type, store, entity, key, version, occurred_at, payload) VALUES(?,?,?,?,?,?,?,?)",
        (evt_id, "pedido.creado", "pedidos", "pedido", "ped_1", 1, "2026-07-30T00:00:00Z", json.dumps({"total": 100}))
    )
    conn.commit()

    row = conn.execute("SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL").fetchone()
    assert row[0] == 1, f"Expected 1 unpublished, got {row[0]}"

    conn.execute("UPDATE _outbox SET published_at = datetime('now') WHERE event_id = ?", (evt_id,))
    conn.commit()

    row = conn.execute("SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL").fetchone()
    assert row[0] == 0, f"Expected 0 unpublished, got {row[0]}"
    conn.close()

def test_idempotency_dedup():
    db = TEMP_DIR / "idempotency.db"
    if db.exists():
        db.unlink()

    conn = sqlite3.connect(str(db))
    conn.execute("""
        CREATE TABLE IF NOT EXISTS _idempotency (
            idempotency_key TEXT NOT NULL,
            store TEXT NOT NULL,
            entity TEXT NOT NULL,
            key TEXT NOT NULL,
            version INTEGER NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (store, idempotency_key)
        )
    """)
    conn.execute("INSERT INTO _idempotency(idempotency_key, store, entity, key, version) VALUES(?,?,?,?,?)",
                 ("ik-1", "pedidos", "pedido", "ped_1", 1))
    conn.commit()

    try:
        conn.execute("INSERT INTO _idempotency(idempotency_key, store, entity, key, version) VALUES(?,?,?,?,?)",
                     ("ik-1", "pedidos", "pedido", "ped_1", 2))
        conn.commit()
        conn.close()
        raise AssertionError("Duplicate insert should have failed")
    except sqlite3.IntegrityError:
        conn.close()
        pass

if __name__ == "__main__":
    print("=== Ixmati Smoke Tests ===")

    tests = [
        ("DB creation", test_db_creation),
        ("Crash durability", test_crash_durability),
        ("Outbox publication", test_outbox_eventual_publication),
        ("Idempotency dedup", test_idempotency_dedup),
    ]

    passed = 0
    for name, fn in tests:
        try:
            fn()
            passed += 1
            print(f"  [PASS] {name}")
        except Exception as e:
            print(f"  [FAIL] {name}: {e}")

    print(f"\n{passed}/{len(tests)} tests passed")
    assert passed == len(tests), f"Only {passed}/{len(tests)} passed"
