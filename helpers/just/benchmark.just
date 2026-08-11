# helpers/just/benchmark.just — comparativa SQLite/Ixmati/PostgreSQL

[group('benchmark')]
benchmark-db:
    @echo "[benchmark] comparativa SQLite directo / Ixmati / PostgreSQL 18..."
    @{{repo_root}}/benchmarks/run_suite.sh
