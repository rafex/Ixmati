#!/usr/bin/env uv run
# helpers/python/lint_tool_boundary.py — enforce: make nunca invoca a just (DEC-0021)

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent

# patron: linea que NO es comentario ni linea vacia y contiene 'just ' como comando
JUST_INVOKE = re.compile(r"^\s*(?!#)(?!.*echo.*just\b).*\bjust\s+")


def check_file(path: Path) -> list[str]:
    violations = []
    for lineno, line in enumerate(path.read_text().splitlines(), 1):
        if JUST_INVOKE.search(line):
            violations.append(f"  {path.relative_to(REPO_ROOT)}:{lineno}: {line.strip()}")
    return violations


def main():
    make_files = [
        REPO_ROOT / "Makefile",
        *list((REPO_ROOT / "helpers" / "make").glob("*.mk")),
    ]

    all_violations = []
    for f in make_files:
        if f.exists():
            all_violations.extend(check_file(f))

    if all_violations:
        print("[boundary] VIOLACION: make invoca a just. Esto esta prohibido (DEC-0021).")
        for v in all_violations:
            print(v)
        return 1

    print("[boundary] OK — make no invoca a just")
    return 0


if __name__ == "__main__":
    sys.exit(main())
