+++
[session]
state = "idle"
agent = "opencode"
initiative = "hardening"
task = "TASK-HARD-0001"
intent = "Fases 0-4 code-complete. Implementado: ixmati-core, ixmati-api (REST/gRPC + auth), ixmati-writer (consumer MQTT, batcher, dedup, outbox transaccional, event_publisher), ixmati-cache (CacheBackend + NoOpBackend), ixmati-projector (pattern R + M), ixmati-reconciler, ixmati-supervisor, containers (7 Containerfiles, compose, quadlet), systemd units, tooling (make, just, uv, hooks, CI, docs). 90+ tests unitarios, 5 integración, 7 smoke. Siguiente: FlashDB backend real, K8s manifests, Fase 5 (observabilidad), fijar versiones de dependencias."
last_updated = "2026-07-31T15:00:00Z"
+++

# Active Session

## Current state

Fases 0-4 code-complete (v0.1.0). Todos los crates implementados con tests. TODO.md sincronizado con la realidad del repositorio.

## Pendiente post-v0.1.0

1. **FlashDB backend**: integrar `spike/flashdb-ffi/` en `ixmati-cache` como backend concreto (reemplazar NoOpBackend).
2. **K8s manifests**: generar Deployment, PVC, Litestream sidecar por store (TASK-WRITE-0024 prometía estos archivos pero no existen).
3. **Fase 5 — Observabilidad**: métricas Prometheus (lag de cola, lag de proyección, latencia de commit, tasa de misses, tamaño de outbox), backpressure, alertas, benchmarks.
4. **Fijar dependencias**: reemplazar wildcards en Cargo.toml por versiones mínimas (rusqlite 0.40, rumqttc 0.25, axum 0.8, tonic 0.14, prost 0.14, tower 0.5).
5. **CI hardening**: agregar `cargo audit` / `cargo deny` al pipeline.
