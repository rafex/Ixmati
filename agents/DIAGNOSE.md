# Diagnóstico del Proyecto

_Fecha: 2026-07-31 | Repositorio: Ixmati_

---

## 1. Exploración

### Estructura general

Repositorio **SpecNative** con código en crates Rust organizados por capas:

```
Ixmati/
├── crates/              → workspace Rust (7 crates + 1 de integración)
│   ├── ixmati-core/         → tipos y contratos compartidos
│   ├── ixmati-api/          → API Gateway (REST + gRPC)
│   ├── ixmati-writer/       → escritor serializado (consumer MQTT + SQLite)
│   ├── ixmati-cache/        → abstracción de cache (FlashDB)
│   ├── ixmati-projector/    → read models (patterns R y M)
│   ├── ixmati-reconciler/   → reproyección offline
│   └── ixmati-supervisor/   → orquestación multi-store
├── proto/ixmati/v1/     → contratos Protobuf (common, write, read)
├── api/                 → OpenAPI spec (openapi.yaml, 281 líneas)
├── config/              → stores.toml, projections.toml, litestream.yml, mosquitto.conf
├── containers/          → Containerfiles (7 servicios), docker-compose, quadlet
├── systemd/             → units nativas: api, projector, writer@
├── helpers/             → make/, just/, shell/, python/ (tooling de build y CI)
├── tests/               → integration/ (Rust), smoke/ (pytest), fixtures/
├── docs/                → mdBook (api, guides, operations, configuration)
├── examples/quickstart/ → compose E2E con seed-data
├── spike/flashdb-ffi    → prueba de viabilidad FlashDB vía FFI
├── spec-native/         → PRODUCT, ARCHITECTURE, DECISIONS, specs/ (4 iniciativas), tasks/
└── dist/                → artefactos release (gitignored)
```

### Lenguajes y tecnologías

- **Rust** (edition 2024, workspace + resolver 2): tokio, axum 0.8, tonic 0.12, rusqlite (bundled), rumqttc, serde, prost, uuid, chrono, tracing
- **SQLite** — fuente de verdad (WAL, `synchronous=NORMAL`)
- **Mosquitto** — broker MQTT (QoS 1, persistente)
- **FlashDB** — cache (spike FFI; hoy `NoOpBackend`)
- **Litestream** — backup continuo del WAL
- **Protobuf / gRPC** — contratos en `proto/ixmati/v1/`
- **Python 3.12 (uv)** — tooling de CI: instalador, validadores, coverage gate, MQTT harness
- **Shell (bash)** — helpers de preflight, restore, integridad SQLite, túneles Podman
- **Podman** (build remoto vía SSH, target `linux/amd64`), **GitHub Actions**, **mdBook** (docs)

### Sistema de build / dependencias

Doble capa:
- **Cargo** (workspace): `Cargo.toml` raíz define 8 members y `[workspace.dependencies]`
- **make** → artefactos (`Makefile` incluye `helpers/make/*.mk`: common, rust, proto, containers, artifacts, installer, ci, dist-validate)
- **just** → task manager (importa `helpers/just/*.just`)
- **uv** → Python tooling
- **cargo-llvm-cov** → cobertura; **cargo-deny/audit** → auditoría

### Puntos de entrada

6 binarios, cada uno en `src/main.rs` del crate:

| Binario | Puerto / Rol |
|---------|-------------|
| `ixmati-api` | REST 30000 / gRPC 30100 |
| `ixmati-writer` | Consumer MQTT → Batcher → WriteEngine → EventPublisher |
| `ixmati-projector` | ProjectorEngine (read models) |
| `ixmati-reconciler` | Reprojección offline síncrona |
| `ixmati-supervisor` | StoreRegistry + Supervisor |
| `scripts/install.sh` | Wrapper del instalador Python |

### Módulos y componentes clave

Dependencias entre crates: **core** ← {api, cache, writer, projector, reconciler, supervisor}; **cache** ← {writer, projector, reconciler}; **writer** ← supervisor.

Flujo arquitectónico: backend → API (REST/gRPC) → Mosquitto → Writer por store → SQLite (BEGIN IMMEDIATE + outbox transaccional) → Projector → FlashDB (`p:`); lecturas vía cache-aside (`c:`) con fallback a SQLite read-only; Litestream replica el WAL por store.

Invariantes: 1 writer por store, ATTACH solo lectura, outbox atómico (0 eventos perdidos), cache desechable.

### Archivos de configuración relevantes

- `.gitignore` — ignora `target/`, `dist/`, `.specnative/.venv/`
- `.github/workflows/ci.yml` — fmt, clippy -D warnings, tests, tool boundary check
- `.github/workflows/release.yml` — release en tags `v*`, push imágenes a ghcr.io
- `.githooks/` — pre-commit, pre-push, commit-msg
- `containers/` — 7 Containerfiles, compose (dev/single/multi-store/test), quadlet
- `config/` — `stores.toml`, `projections.toml`, `litestream.yml`, `mosquitto.conf`
- `systemd/` — 3 units
- `codex.toml`, `.coverage-floor`, `VERSION`, `CHANGELOG.md`

### Estado del repositorio

- **Rama**: `main`, limpio, sin worktrees, al día con `origin/main`
- **Último commit**: `9b5daa4` — `fix: 🐛 allinone — db_path en API + --network=host en build` (2026-07-31 08:44)
- **Historial**: 29 commits, mensajes convencionales con emoji
- **Versión**: 0.1.0
- **TODO.md**: tareas pendientes en writer, projector, contenedores, CI/CD
- **SESSION.md**: `in_progress`, Fase 2 completada, 38+ tests verdes

---

## 2. Revisión de calidad

### Problemas estructurales o de diseño

- **FlashDB no integrado (DEC-0009)**: el cache real es `NoOpBackend`. El spike FFI (`spike/flashdb-ffi`) está aislado. FlashDB es una librería C para microcontroladores sin binding Rust oficial. Si el spike falla, las proyecciones (pattern R/M) dependen de un backend que no existe.
- **`.unwrap()` en código productivo**: `crates/ixmati-core/src/attach.rs` usa `.unwrap()` para abrir conexiones y crear directorios. En producción un fallo de I/O producirá un panic irrecuperable en lugar de un error manejable.
- **24 tareas pendientes** en `TODO.md`, con fases 3 y 4 sin empezar. La arquitectura está definida pero la implementación está al ~40% del roadmap.

### Deuda técnica identificada

- **Dependencias con versión wildcard** en `Cargo.toml` raíz: `rusqlite = "0"`, `rumqttc = "0"`, `axum = "0"`, `tonic = "0"`, `prost = "0"`. Un `cargo update` puede romper el build silenciosamente.
- **`tower = "0"`** en `crates/ixmati-api/Cargo.toml` como dependencia directa fuera del workspace.
- **`tracing = "0.1"` y `tracing-subscriber = "0.3"`** — posible incompatibilidad de versiones.
- **Sin dev-dependencies declaradas**: todos los tests dependen de las mismas dependencias de runtime. No es un bug, pero es una práctica no idiomática en Rust.
- **Código limpio**: 0 TODOs, 0 FIXMEs, 0 unsafe en todo el workspace. Sin marcadores de deuda explícitos.

### Prácticas del lenguaje no seguidas

- **`.unwrap()` en código productivo** (`attach.rs`): Rust idiomático prefiere `?` para propagación de errores o `.expect("mensaje")` si el panic es intencional y documentado.
- **Falta de `thiserror` o `anyhow`** en manejo de errores de `attach.rs`: las operaciones de filesystem y SQLite deberían usar tipos de error del crate (`ixmati-core::error`).
- **Versiones wildcard (`"0"`)**: no es idiomático en Rust; la convención es usar versiones mínimas (`"0.31"`) o rangos compatibles (`"^0.31"`).

### Riesgos de seguridad

- **Sin secretos expuestos**: `config/stores.toml` y demás archivos de configuración están limpios de passwords, tokens o API keys.
- **Sin archivos `.env`**: no hay leaks de configuración sensible.
- **Sin `unsafe` en el workspace**: indicador positivo de seguridad de memoria.
- **CI con gates de seguridad**: `clippy -D warnings` fuerza zero-warning lint. Sin embargo, **falta `cargo audit` o `cargo deny`** en el pipeline de CI para detectar vulnerabilidades en dependencias.

### Cobertura de tests y documentación

| Crate | Tests unitarios | Cobertura estimada |
|-------|----------------|-------------------|
| ixmati-core | ~10 tests (attach, mqtt, envelope) | Media |
| ixmati-api | ~20 tests (health, auth, status) | Alta |
| ixmati-writer | ~24 tests (outbox, batcher, dedup, write_engine, cache_sync, event_publisher) | Alta |
| ixmati-projector | ~8 tests (pattern_r, pattern_m) | Media |
| ixmati-reconciler | 3 tests | Baja |
| ixmati-cache | 4 tests (solo NoOpBackend) | Muy baja |
| ixmati-supervisor | ~5 tests | Media |
| **Integración** | 5 tests (bootstrap, crash) | — |
| **Smoke (Python)** | 7 tests (write_read, outbox, projection_lag, crash, restore) | — |

- **Total**: ~90+ unitarios, 5 integración, 7 smoke
- **Brechas**: `ixmati-cache` sin tests reales (solo NoOpBackend). `ixmati-reconciler` con cobertura mínima.
- **Documentación**: SpecNative completo (33 decisiones, 4 specs). mdBook para docs de usuario. Sin README por crate (no es crítico dado el workspace).

---

## 3. Síntesis ejecutiva

### Resumen del proyecto

**Ixmati** es un motor de serialización de escrituras para SQLite que permite que N backends concurrentes escriban en la misma base de datos sin contención (`SQLITE_BUSY`), con aislamiento de fallo entre dominios (stores) y sin introducir infraestructura pesada (Postgres, Kafka).

**Stack**: Rust (edition 2024) · SQLite (WAL) · Mosquitto (MQTT QoS 1) · FlashDB (cache, pendiente integración) · Litestream (backup) · Protobuf/gRPC + REST (axum) · Podman rootless

**Arquitectura**: 7 crates en workspace Rust. Flujo: backends → API/MQTT → writer (serializa, batchea, outbox transaccional) → SQLite. Lecturas vía cache-aside + read models proyectados. Un writer por store, con concurrencia interna.

**Madurez**: 29 commits, v0.1.0. Fases 0-1-2 del roadmap completadas. En fase 3 (cache + proyecciones). Sesión activa en `TASK-WRITE-0019`.

### Estado de salud

**🟡 AMARILLO** — Proyecto bien diseñado con ejecución disciplinada, pero con riesgos de reproducibilidad y una dependencia clave (FlashDB) sin resolver.

| Dimensión | Estado |
|---|---|
| Arquitectura | 🟢 Decisiones sólidas (33 DECs). Sin `unsafe`. Diseño coherente |
| Calidad de código | 🟢 CI robusto. Sin secretos. Código limpio |
| Tests | 🟡 ~90 unitarios + 5 integración + 7 smoke. Cache sin tests reales |
| Dependencias | 🟡 Wildcards en 6 dependencias críticas |
| Deuda técnica | 🟡 FlashDB no integrado. 24 tareas pendientes |
| Operaciones | 🟢 Containerfiles, compose, quadlet, systemd, docs |
| Documentación | 🟢 SpecNative completo, 33 decisiones, 4 specs |

### Top 3 fortalezas

1. **Arquitectura decisional completa**: 33 decisiones documentadas con contexto, trade-offs y consecuencias. Cualquier agente o contribuidor puede entender *por qué* el sistema es como es.
2. **Outbox transaccional + idempotencia por diseño**: garantía de 0 eventos perdidos y 0 duplicados construida desde la base, no añadida como parche.
3. **Disciplina operativa**: CI con gates automáticos, hooks versionados, separación docs/spec-native, tiers de test definidos, despliegue con Podman rootless + Quadlet.

### Top 3 riesgos o deudas

1. **FlashDB como backend de cache (DEC-0009)**: hoy es `NoOpBackend`. El spike FFI no está integrado. FlashDB es una librería C para microcontroladores sin binding Rust oficial. Si el spike falla, las proyecciones (pattern R/M) dependen de un backend que no existe. **Impacto**: bloquea fase 3 completa.
2. **Wildcards de versión en dependencias críticas**: `rusqlite = "0"`, `rumqttc = "0"`, `axum = "0"`, `tonic = "0"`, `prost = "0"`. Un `cargo update` puede romper el build silenciosamente.
3. **24 tareas pendientes con Fase 3-4 sin empezar**: cache real, proyecciones, reconciler, Litestream por store, health checks, runbook de producción. La implementación está al ~40% del roadmap.

### Próximos pasos recomendados

1. **P0 — Resolver DEC-0009**: cerrar el spike FlashDB FFI o decidir alternativa (sled/redb/lmdb-rs) de forma definitiva. Bloquea fase 3.
2. **P1 — Fijar versiones de dependencias**: reemplazar wildcards por rangos semánticos (ej. `rusqlite = "0.31"`, `axum = "0.8"`). Mejora reproducibilidad del build.
3. **P1 — Eliminar `.unwrap()` en código productivo**: reemplazar por propagación de errores (`?`) o `.expect("mensaje")` en `attach.rs`.
4. **P2 — Completar proyecciones pattern R/M**: `TASK-WRITE-0020` + `TASK-WRITE-0021`. Funcionalidad core del read path.
5. **P2 — Añadir tests reales para `ixmati-cache`** una vez decidido el backend.
6. **P3 — Completar tests de crash** (`TASK-WRITE-0007`): validar garantía de 0 comandos perdidos.
7. **P3 — Validación e2e** (`TASK-CONT-0012`): cierre del loop de despliegue contra host real.

---

## 4. Archivos relevantes

| Archivo | Tipo | Relevancia |
|---------|------|------------|
| `Cargo.toml` | config | Workspace raíz con wildcards de versión — riesgo de reproducibilidad |
| `crates/ixmati-core/src/attach.rs` | module | `.unwrap()` en código productivo — riesgo de panics en producción |
| `crates/ixmati-writer/src/write_engine.rs` | module | Write engine — componente más complejo del sistema |
| `crates/ixmati-writer/src/outbox.rs` | module | Outbox transaccional — garantía de 0 eventos perdidos |
| `crates/ixmati-cache/src/lib.rs` | module | Cache — NoOpBackend, sin integración real de FlashDB |
| `crates/ixmati-api/src/main.rs` | entry | Entry point de la API Gateway (REST + gRPC) |
| `config/stores.toml` | config | Configuración de stores por defecto |
| `.github/workflows/ci.yml` | CI/CD | CI pipeline — falta cargo-audit/deny |
| `spike/flashdb-ffi/` | spike | Prueba de viabilidad FlashDB — no integrado al workspace |
| `spec-native/ARCHITECTURE.md` | doc | Documento de arquitectura — fuente de verdad del diseño |
| `spec-native/DECISIONS.md` | doc | 33 decisiones de arquitectura con trade-offs |
| `spec-native/ROADMAP.md` | doc | Roadmap por fases (0-4) con hitos cuantificables |
| `TODO.md` | tracking | 24 tareas pendientes con estados |
