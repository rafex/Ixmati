#!/usr/bin/env uv run
# helpers/python/validate_config.py — valida stores.toml y projections.toml

import sys
from pathlib import Path

try:
    import tomli
except ImportError:
    import tomllib as tomli  # type: ignore

REPO_ROOT = Path(__file__).resolve().parent.parent.parent


def validate_stores(config: dict) -> list[str]:
    errors = []
    stores = config.get("stores", [])
    names = set()

    if not isinstance(stores, list):
        errors.append("stores debe ser una lista")
        return errors

    for idx, store in enumerate(stores):
        if not isinstance(store, dict):
            errors.append(f"stores[{idx}]: debe ser una tabla")
            continue

        name = store.get("name", "")
        if not name:
            errors.append(f"stores[{idx}]: falta 'name'")
            continue

        if not re.match(r"^[a-z][a-z0-9_]*$", name):
            errors.append(f"stores[{idx}]: name '{name}' invalido (snake_case sin /)")

        if name in names:
            errors.append(f"stores[{idx}]: name '{name}' duplicado")
        names.add(name)

        if not store.get("db_path"):
            errors.append(f"stores[{idx}] ({name}): falta 'db_path'")

    return errors


def validate_projections(config: dict) -> list[str]:
    errors = []
    projections = config.get("projections", [])

    if not isinstance(projections, list):
        errors.append("projections debe ser una lista")
        return errors

    for idx, proj in enumerate(projections):
        if not isinstance(proj, dict):
            errors.append(f"projections[{idx}]: debe ser una tabla")
            continue

        name = proj.get("name", "")
        if not name:
            errors.append(f"projections[{idx}]: falta 'name'")
            continue

        pattern = proj.get("pattern", "")
        if pattern not in ("R", "M"):
            errors.append(f"projections[{idx}] ({name}): pattern debe ser 'R' o 'M'")

        if not proj.get("source_stores"):
            errors.append(f"projections[{idx}] ({name}): falta 'source_stores'")

        if not proj.get("target_key"):
            errors.append(f"projections[{idx}] ({name}): falta 'target_key'")

    return errors


def main():
    stores_path = REPO_ROOT / "config" / "stores.toml"
    projections_path = REPO_ROOT / "config" / "projections.toml"

    all_errors = []

    if stores_path.exists():
        print(f"[validate] {stores_path.name}")
        config = tomli.loads(stores_path.read_text())
        errors = validate_stores(config)
        all_errors.extend(errors)
        if not errors:
            print("  OK")
    else:
        print(f"[validate] {stores_path.name} no encontrado — omitiendo")

    if projections_path.exists():
        print(f"[validate] {projections_path.name}")
        config = tomli.loads(projections_path.read_text())
        errors = validate_projections(config)
        all_errors.extend(errors)
        if not errors:
            print("  OK")
    else:
        print(f"[validate] {projections_path.name} no encontrado — omitiendo")

    if all_errors:
        print(f"\n[validate] {len(all_errors)} error(es):")
        for e in all_errors:
            print(f"  - {e}")
        return 1

    print("[validate] OK")
    return 0


if __name__ == "__main__":
    import re
    sys.exit(main())
