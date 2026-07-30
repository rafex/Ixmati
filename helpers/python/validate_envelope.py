#!/usr/bin/env uv run
# helpers/python/validate_envelope.py — valida payloads JSON de comando/evento contra schema

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent

WRITE_ENVELOPE_SCHEMA = {
    "type": "object",
    "required": ["op", "store", "entity", "key", "version", "idempotency_key", "ack_mode", "payload"],
    "properties": {
        "op": {"type": "string", "enum": ["upsert", "delete", "patch"]},
        "store": {"type": "string", "pattern": "^[a-z][a-z0-9_]*$"},
        "entity": {"type": "string"},
        "key": {"type": "string"},
        "version": {"type": "integer", "minimum": 1},
        "ts": {"type": "string"},
        "idempotency_key": {"type": "string"},
        "ack_mode": {"type": "string", "enum": ["accepted", "committed"]},
        "payload": {"type": "object"},
    },
}

EVENT_ENVELOPE_SCHEMA = {
    "type": "object",
    "required": ["event_id", "event_type", "store", "entity", "key", "version", "occurred_at", "payload"],
    "properties": {
        "event_id": {"type": "string"},
        "event_type": {"type": "string"},
        "store": {"type": "string", "pattern": "^[a-z][a-z0-9_]*$"},
        "entity": {"type": "string"},
        "key": {"type": "string"},
        "version": {"type": "integer", "minimum": 1},
        "occurred_at": {"type": "string"},
        "outbox_seq": {"type": "integer"},
        "payload": {"type": "object"},
    },
}


def validate_envelope(data: dict, schema: dict, label: str) -> list[str]:
    errors = []
    try:
        import jsonschema
    except ImportError:
        print(f"[validate] jsonschema no instalado — validacion basica")
        for field in schema["required"]:
            if field not in data:
                errors.append(f"{label}: falta campo requerido '{field}'")
        op = data.get("op", "")
        store = data.get("store", "")
        if op and op not in ("upsert", "delete", "patch"):
            errors.append(f"{label}: op='{op}' invalido")
        if store and not store.replace("_", "").isalnum():
            errors.append(f"{label}: store='{store}' invalido")
        return errors

    try:
        jsonschema.validate(data, schema)
    except jsonschema.ValidationError as e:
        errors.append(f"{label}: {e.message}")
    return errors


def main():
    if len(sys.argv) < 2:
        print("uso: validate_envelope.py <archivo.json>")
        print("  valida si es un envelope de comando o evento valido")
        return 1

    filepath = Path(sys.argv[1])
    if not filepath.exists():
        print(f"archivo no encontrado: {filepath}")
        return 1

    data = json.loads(filepath.read_text())

    # detectar tipo
    if "op" in data:
        errors = validate_envelope(data, WRITE_ENVELOPE_SCHEMA, "comando")
    elif "event_id" in data:
        errors = validate_envelope(data, EVENT_ENVELOPE_SCHEMA, "evento")
    else:
        print("[validate] no se pudo detectar el tipo de envelope (necesita 'op' o 'event_id')")
        return 1

    if errors:
        for e in errors:
            print(f"  - {e}")
        return 1

    print("[validate] envelope OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
