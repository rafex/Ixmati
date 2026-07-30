# helpers/just/hooks.just — gestion de git hooks

# instala los hooks (core.hooksPath → .githooks)
[group('hooks')]
hooks-install:
    @echo "[hooks] instalando..."
    git config core.hooksPath .githooks
    @chmod +x {{repo_root}}/.githooks/*
    @echo "[hooks] instalados: $(git config core.hooksPath)"

# desinstala hooks (vuelve al default de git)
[group('hooks')]
hooks-uninstall:
    @echo "[hooks] desinstalando..."
    git config --unset core.hooksPath
    @echo "[hooks] desinstalados"

# ejecuta manualmente el hook pre-commit
[group('hooks')]
hooks-pre-commit:
    @{{repo_root}}/.githooks/pre-commit

# ejecuta manualmente el hook pre-push
[group('hooks')]
hooks-pre-push:
    @{{repo_root}}/.githooks/pre-push

# ejecuta manualmente el hook commit-msg (pasar msg via stdin)
hooks-commit-msg msg:
    @echo "{{msg}}" | {{repo_root}}/.githooks/commit-msg /dev/stdin
