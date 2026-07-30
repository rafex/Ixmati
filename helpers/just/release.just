# helpers/just/release.just — tareas de release

repo_root := `git rev-parse --show-toplevel 2>/dev/null || pwd`

# muestra la version actual (desde Cargo.toml)
[group('release')]
version:
    @cargo metadata --format-version 1 --no-deps 2>/dev/null | \
        python3 -c "import sys,json; print(json.load(sys.stdin).get('workspace_root','?'))" || \
        echo "version no disponible (ejecuta cargo metadata)"

# genera changelog desde commits convencionales
[group('release')]
changelog since="HEAD~10":
    @echo "# Changelog"
    @git log {{since}}..HEAD --pretty=format:"- %s (%h)" --reverse
