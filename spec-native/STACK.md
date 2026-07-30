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
| Serialización | `serde` + `serde_json` | 1.x | JSON para API REST y envelope de mensajes |
| Serialización binaria | `prost` | 0.13.x | Protobuf para gRPC |

## Base de datos

| Componente | Tecnología | Configuración clave |
|---|---|---|
| Fuente de verdad | **SQLite** | `PRAGMA journal_mode=WAL`, `PRAGMA synchronous=NORMAL`, `PRAGMA busy_timeout=5000`, `PRAGMA foreign_keys=ON` |
| Driver Rust | `rusqlite` con feature `bundled` | 0.32.x. `bundled` compila SQLite estáticamente para evitar dependencia del sistema. |
| Pool de lectura | `r2d2-sqlite` o `deadpool-sqlite` | Pool de conexiones read-only para el camino de fallback de lectura. |

## Cola de mensajes

| Componente | Tecnología | Configuración clave |
|---|---|---|
| Broker | **Mosquitto** | `persistence true`, `persistence_file mosquitto.db`, `autosave_interval 60`, `max_queued_messages 100000` |
| QoS | 1 (at least once) | Garantiza entrega sin duplicados no manejados (la deduplicación está en la capa de aplicación vía `idempotency_key`). |
| Cliente Rust | `rumqttc` | 0.24.x. Cliente asíncrono compatible con tokio. |

## Cache

| Componente | Tecnología | Riesgo |
|---|---|---|
| Cache de lectura | **FlashDB** | ⚠️ **Riesgo abierto**: FlashDB es una librería C embebida diseñada para microcontroladores (STM32, ESP32). No tiene binding oficial en Rust. Se requiere FFI (`bindgen` + `cc`). |
| Alternativas si FlashDB no cuaja | `sled`, `redb`, `lmdb-rs` | Evaluar en el spike DEC-0009. Criterio: debe soportar get/set/invalidate con TTL, arranque rápido, y tamaño acotado. |

**Nota sobre FlashDB**: FlashDB ofrece operación ultrarrápida, baja huella de memoria, y resistencia a pérdida de energía en embebidos. En un entorno de servidor Linux, estas propiedades pueden no traducirse directamente. El spike de viabilidad (`TASK-WRITE-0001`) debe determinar si el FFI es mantenible y si el rendimiento justifica el costo de integración.

## Backup y disaster recovery

| Componente | Tecnología | Configuración |
|---|---|---|
| Replicación WAL | **Litestream** | `litestream replicate` hacia ≥2 destinos (S3-compatible o filesystem en VPS remoto) |
| RPO objetivo | < 5 segundos | Con `sync-interval` ajustado |
| RTO objetivo | < 60 segundos | Tiempo desde detección de fallo hasta restauración y servicio funcionando |
| Comando de restore | `litestream restore` | Documentado en `spec-native/COMMANDS.md` |

## Infraestructura y despliegue

| Componente | Tecnología |
|---|---|
| Contenedores | Docker + docker-compose para desarrollo local |
| Orquestación | Kubernetes (producción) o docker-compose (single-VPS) |
| Health checks | HTTP `/health` en la API + MQTT `$SYS/broker/uptime` + SQLite `PRAGMA integrity_check` |
| Métricas | `tracing` + `opentelemetry` (export a Prometheus o stdout) |
| Logs | `tracing-subscriber` con formato JSON estructurado |

## Restricciones de versión

- **SQLite >= 3.37.0**: requerido por `PRAGMA strict` y `BEGIN IMMEDIATE` con semántica correcta. La feature `bundled` de rusqlite garantiza la versión.
- **Mosquitto >= 2.0**: requerido por `persistence` configurable y `max_queued_messages`.
- **Rust >= 1.80**: requerido por edition 2024 y features del ecosistema.
