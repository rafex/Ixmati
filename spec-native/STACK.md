# STACK.md

Tecnologías, versiones objetivo y restricciones del proyecto.

## Lenguaje

- **Rust** (stable, edition 2024). Toolchain gestionada con `rustup`.
- Razón: zero-cost abstractions, sin garbage collector, ecosistema maduro para networking (tokio), bases de datos (rusqlite, sqlx), y MQTT (rumqttc).

## Runtime y frameworks

| Componente | Tecnología | Versión | Notas |
|---|---|---|---|
| Runtime asíncrono | `tokio` | 1.x (latest) | Multi-threaded, work-stealing |
| API REST | `axum` | 0.8.x | Sobre hyper + tokio |
| API gRPC | `tonic` | 0.12.x | Codegen desde `.proto` |
| Serialización | `serde` + `serde_json` | 1.x | JSON para API REST y envelopes |
| Serialización binaria | `prost` | 0.13.x | Protobuf para gRPC |

## Base de datos

| Componente | Tecnología | Configuración clave |
|---|---|---|
| Fuente de verdad | **SQLite** (un archivo por store) | `PRAGMA journal_mode=WAL`, `PRAGMA synchronous=NORMAL`, `PRAGMA busy_timeout=5000`, `PRAGMA foreign_keys=ON` |
| Driver Rust | `rusqlite` con feature `bundled` | 0.32.x. `bundled` compila SQLite estáticamente. |
| Pool de lectura | `r2d2-sqlite` o `deadpool-sqlite` | Pool de conexiones read-only para fallback de lectura y para projector/reconciler. |

### Tablas por store

| Tabla | Propósito | Limpieza |
|---|---|---|
| `{entity}` | Datos de la entidad (una tabla por entidad declarada) | — |
| `_idempotency` | Registro de comandos procesados: `(key, applied_at, status)` | TTL 24h |
| `_outbox` | Eventos pendientes de publicación: `(id, event_type, event_id, store, entity, key, version, occurred_at, payload, published_at)` | Filas con `published_at` > 7 días |
| `_projection_state` | (opcional) Tracking de event_id procesados por proyección, para idempotencia | TTL configurable |

## Cola de mensajes

| Componente | Tecnología | Configuración clave |
|---|---|---|
| Broker | **Mosquitto** | `persistence true`, `persistence_file mosquitto.db`, `autosave_interval 60`, `max_queued_messages 100000` |
| QoS comandos | 1 (at least once) | Deduplicación en capa de aplicación (`_idempotency` por store) |
| QoS eventos | 1 (at least once) | Idempotencia en proyectores (`event_id` o upsert natural) |
| Cliente Rust | `rumqttc` | 0.24.x. Cliente asíncrono compatible con tokio. |
| `retained` | `false` para cmd y evt | Los proyectores se ponen al día desde outbox, no desde retained messages |

## Cache y read models

| Componente | Tecnología | Riesgo |
|---|---|---|
| Backend de storage | **FlashDB** | ⚠️ **Riesgo abierto**: librería C para microcontroladores. Sin binding oficial en Rust. Requiere FFI (`bindgen` + `cc`). **Severidad alta**: aloja tanto cache-aside como read models proyectados. |
| Alternativas | `sled`, `redb`, `lmdb-rs` | Evaluar en spike `TASK-WRITE-0001`. Criterio: get/set/invalidate con TTL + delete_by_prefix + arranque rápido. |
| Namespace `c:` | Cache-aside (lazy, se llena en read-miss) | TTL configurable. Invalidación/repoblación por writer. |
| Namespace `p:` | Read models (eager, se llenan por proyección) | TTL configurable por proyección. Reconstruibles vía reconciler. |

## Backup y disaster recovery

| Componente | Tecnología | Configuración |
|---|---|---|
| Replicación WAL | **Litestream** | `litestream replicate` por store hacia ≥2 destinos (S3-compatible o filesystem en VPS remoto) |
| RPO objetivo | < 5 segundos | Por store, configurable independientemente |
| RTO objetivo | < 60 segundos | Tiempo desde detección hasta servicio restaurado |
| Número de instancias | 1 por store | Sidecar en K8s o proceso separado |
| Comando de restore | `litestream restore` | Documentado en `COMMANDS.md` |

## Infraestructura y despliegue

| Componente | Tecnología |
|---|---|
| Contenedores | Docker + docker-compose para desarrollo local |
| Orquestación | Kubernetes (producción, 1 pod por store) o docker-compose (single-VPS, supervisor con N writers) |
| PersistentVolumeClaim | 1 PVC por store (para el archivo SQLite) |
| Litestream sidecar | 1 contenedor sidecar por store (lee WAL, replica a S3) |
| Health checks | HTTP `/health` + MQTT `$SYS/broker/uptime` + SQLite `SELECT 1` por store |
| Métricas | `tracing` + `opentelemetry` (export a Prometheus o stdout) |
| Logs | `tracing-subscriber` con formato JSON estructurado |

## Tooling

| Componente | Tecnología | Versión | Notas |
|---|---|---|---|
| Build system | **make** | — | Compila artefactos. `helpers/make/*.mk`. |
| Task runner | **just** | ≥ 1.0 | Task manager. `helpers/just/*.just`. |
| Python tooling | **uv** + Python | ≥ 3.12 | Scripts de CI, validación, coverage. |
| Cobertura | **cargo-llvm-cov** | latest | Ratchet de cobertura. |
| Auditoría deps | **cargo-deny** + **cargo-audit** | latest | Licencias, seguridad. |
| Documentación | **mdBook** | ≥ 0.4 | Libro en `docs/`. |
| Git hooks | `.githooks/` versionado | — | `just hooks-install` configura `core.hooksPath`. |

## Restricciones de versión

- **SQLite >= 3.37.0**: requerido por `PRAGMA strict` y `BEGIN IMMEDIATE`. Feature `bundled` de rusqlite garantiza la versión.
- **Mosquitto >= 2.0**: requerido por `persistence` configurable y `max_queued_messages`.
- **Rust >= 1.80**: requerido por edition 2024 y features del ecosistema.
- **Python >= 3.12**: gestionado por `uv`. Nunca usar el intérprete del sistema.
- **Rustup**: prerequisito duro para desarrollo. Detectado por `just doctor`.
