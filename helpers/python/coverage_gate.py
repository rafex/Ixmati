#!/usr/bin/env uv run
# helpers/python/coverage_gate.py — ratchet de cobertura
# Si la cobertura actual es menor que el piso en .coverage-floor, falla.
# Si es mayor en >= 0.5pp, sugiere subir el piso.
# El piso arranca en 0 y solo sube.

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
FLOOR_FILE = REPO_ROOT / ".coverage-floor"


def read_floor() -> float:
    if not FLOOR_FILE.exists():
        return 0.0
    content = FLOOR_FILE.read_text().strip()
    match = re.search(r"(\d+\.?\d*)", content)
    if match:
        return float(match.group(1))
    return 0.0


def parse_lcov_coverage(lcov_path: Path) -> float | None:
    """Extrae el porcentaje de cobertura de lineas de un archivo lcov."""
    if not lcov_path.exists():
        print(f"[coverage_gate] lcov no encontrado: {lcov_path}")
        return None

    lines_total = 0
    lines_hit = 0
    in_file = False

    for line in lcov_path.read_text().splitlines():
        if line.startswith("SF:"):
            in_file = True
        elif line.startswith("end_of_record"):
            in_file = False
        elif in_file and line.startswith("DA:"):
            parts = line.split(",")
            lines_total += 1
            if int(parts[1]) > 0:
                lines_hit += 1

    if lines_total == 0:
        return 0.0
    return (lines_hit / lines_total) * 100.0


def main():
    floor = read_floor()

    # intentar leer lcov
    lcov_path = REPO_ROOT / "target" / "coverage.lcov"
    current = parse_lcov_coverage(lcov_path)

    if current is None:
        print(f"[coverage_gate] no se pudo determinar cobertura — omitiendo (floor={floor:.1f}%)")
        return 0

    print(f"[coverage_gate] piso={floor:.1f}%  actual={current:.1f}%")

    if current < floor:
        print(f"[coverage_gate] FAIL: cobertura bajo el piso ({current:.1f}% < {floor:.1f}%)")
        return 1

    margin = current - floor
    if margin >= 0.5:
        print(f"[coverage_gate] sugerencia: sube el piso a {current:.1f}% (margen +{margin:.1f}pp)")
        print(f"              edita .coverage-floor en la raiz del repo")

    print("[coverage_gate] OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
