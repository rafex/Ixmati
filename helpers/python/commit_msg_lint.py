#!/usr/bin/env uv run
# helpers/python/commit_msg_lint.py — valida Conventional Commits

import os
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent

# spec: https://www.conventionalcommits.org/en/v1.0.0/
PATTERN = re.compile(
    r"^(?P<type>build|chore|ci|docs|feat|fix|perf|refactor|style|test)"
    r"(?:\((?P<scope>[^)]+)\))?"
    r"(?P<breaking>!)?"
    r":\s"
    r"(?P<subject>.+)"
)


def get_commit_messages() -> list[str]:
    """Obtiene los mensajes de commit a validar."""
    # en CI: validar todos los commits del PR vs base
    base = os.environ.get("GITHUB_BASE_REF", "main")
    head = os.environ.get("GITHUB_HEAD_REF", "HEAD")

    try:
        result = subprocess.run(
            ["git", "log", f"origin/{base}..{head}", "--format=%s"],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip().split("\n")
    except Exception:
        pass

    # fallback: ultimo commit
    result = subprocess.run(
        ["git", "log", "-1", "--format=%s"],
        capture_output=True,
        text=True,
        cwd=REPO_ROOT,
    )
    return result.stdout.strip().split("\n") if result.stdout.strip() else []


def validate_message(msg: str) -> str | None:
    if not PATTERN.match(msg):
        return f"formato invalido: '{msg}' (debe seguir Conventional Commits)"
    return None


def main():
    is_ci = "--ci" in sys.argv

    messages = get_commit_messages()
    if not messages:
        print("[commit-msg] sin commits que validar")
        return 0

    errors = []
    for msg in messages:
        error = validate_message(msg)
        if error:
            errors.append(error)

    if errors:
        print(f"[commit-msg] {len(errors)} error(es):")
        for e in errors:
            print(f"  - {e}")
        if is_ci:
            return 1
        print("[commit-msg] corrige el mensaje y vuelve a intentar")
        return 1

    print(f"[commit-msg] OK — {len(messages)} commit(s) validos")
    return 0


if __name__ == "__main__":
    sys.exit(main())
