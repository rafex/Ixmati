# helpers/just/docs.just — documentacion mdBook

repo_root := `git rev-parse --show-toplevel 2>/dev/null || pwd`

# compila el libro (mdBook build)
[group('docs')]
docs-build:
    @echo "[docs] compilando mdBook..."
    @mdbook build {{repo_root}}/docs
    @echo "[docs] listo: docs/book/"

# levanta servidor de desarrollo con live reload
[group('docs')]
docs-serve:
    @echo "[docs] sirviendo en http://localhost:3000..."
    @mdbook serve --open {{repo_root}}/docs

# verifica links rotos en el libro
[group('docs')]
docs-check-links:
    @echo "[docs] verificando links..."
    @mdbook test {{repo_root}}/docs
