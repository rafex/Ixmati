# Changelog

Todos los cambios notables de Ixmati documentados en este archivo.

El formato se basa en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/),
y este proyecto adhiere al [Versionamiento Semántico](https://semver.org/lang/es/).

## [0.1.0] — 2026-07-30

### Added
- Motor de escritura con SQLite (WAL, synchronous=NORMAL) por store
- API REST + gRPC (axum + tonic) con health checks
- Writer con consumer MQTT, batching, dedup, outbox transaccional
- `ixmati-cache` con trait `CacheBackend` + `NoOpBackend`
- `ixmati-projector` con patterns R (referencia) y M (materializado)
- `ixmati-reconciler` para reproyección offline fan-in sobre N stores
- `ixmati-supervisor` para orquestación de N stores con topología configurable
- Authentication middleware (API keys + Bearer tokens)
- Contratos: `proto/ixmati/v1` (common, write, read) + `api/openapi.yaml`
- Container infrastructure con Podman rootless (builder cargo-chef + 5 servicios)
- Parseador MQTT (`tcp://host:port`) unificado en `ixmati-core`
- Instalador Python para servidores Linux
- Pipeline CI/CD con GitHub Actions + ghcr.io
- Quickstart con `docker compose` (`examples/quickstart/`)
- Tests: 90+ unitarios, 5 integración, 4 smoke
- Spike FlashDB FFI confirmado viable en Linux x86_64 (DEC-0009)
- Litestream config con >=2 destinos por store
- Runbook de producción con restore, failover y troubleshooting
