# helpers/just/benchmark.just — comparativa SQLite/Ixmati/PostgreSQL

[group('benchmark')]
benchmark-db:
    @echo "[benchmark] comparativa SQLite directo / Ixmati / PostgreSQL 18..."
    @{{repo_root}}/benchmarks/run_suite.sh

[group('benchmark')]
benchmark-soak rate="150" duration="3600" concurrency="200":
    @command -v jmeter >/dev/null || (echo "jmeter no está instalado" >&2; exit 1)
    @mkdir -p {{repo_root}}/spec-native/evidence/raw/jmeter
    @jmeter -n -t {{repo_root}}/benchmarks/ixmati-soak.jmx \
        -Jrate={{rate}} -Jduration={{duration}} -Jconcurrency={{concurrency}} \
        -Jhost=$${IXMATI_TEST_HOST:-127.0.0.1} -Jport=$${IXMATI_TEST_PORT:-30000} \
        -Jstore=$${IXMATI_TEST_STORE:-default} -Japi_key=$${IXMATI_API_KEY:-ix-default-key} \
        -l {{repo_root}}/spec-native/evidence/raw/jmeter/soak-{{rate}}.jtl \
        -j {{repo_root}}/spec-native/evidence/raw/jmeter/soak-{{rate}}.log
