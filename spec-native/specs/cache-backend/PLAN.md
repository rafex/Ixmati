# Plan: 3 Backends de Cache (FlashDB + Redb + SQLite) con Patrón Escritor Único

> **Iniciativa**: `cache-backend` | **Task**: `TASK-CACHE-0001`
> **Estado**: Código completo, bloqueado por saturación de disco en bastion
> **Fecha**: 2026-08-05

---

## Contexto

DEC-0009 abrió el riesgo de FlashDB (librería C para microcontroladores vía FFI).
El spike inicial compiló pero FlashDB no persistía datos en Linux. La investigación
reveló **3 bugs raíz**:

| # | Bug | Causa | Fix aplicado |
|---|---|---|---|
| B1 | FlashDB no persistía | `fdb_kvdb_init` recibía `"fdb_kvdb1"` como path en vez de `data_dir` | `CString::new(data_dir)` |
| B2 | SIGFPE en runtime | `sec_size=0` en struct zeroed causaba división por cero | `(*kvdb).parent.sec_size = 4096` antes de init |
| B3 | Concurrencia insegura | API y writer abrían el mismo cache en modo read-write | Patrón escritor único: API usa `ReadOnlyCache` |

**Decisión del usuario**: Mantener FlashDB (con fixes), agregar Redb como alternativa,
y SQLite como fallback. Los 3 seleccionables vía `CACHE_BACKEND` env var.

---

## Arquitectura: Patrón Escritor Único

```
ESCRITURAS:
  API → POST /write → Mosquitto → Writer (único proceso)
                                        ├→ SQLite (fuente de verdad)
                                        └→ Cache (write-through tras commit)
                                            ├→ FlashDB  (si CACHE_BACKEND=flashdb)
                                            ├→ Redb     (si CACHE_BACKEND=redb)
                                            └→ SQLite   (si CACHE_BACKEND=sqlite)

LECTURAS:
  API → GET /read → Cache (read-only) → hit → return
                                ↓ miss
                          SQLite (read-only) → return
```

**Regla clave**: Solo el writer escribe en el cache. La API solo lee.

---

## Cambios ya implementados (no commiteados)

### Archivos nuevos

| Archivo | Líneas | Descripción |
|---|---|---|
| `crates/ixmati-cache/src/redb_backend.rs` | ~130 | RedbCacheBackend con `new()` (rw) y `new_readonly()` |
| `crates/ixmati-cache/src/readonly.rs` | ~25 | ReadOnlyCache wrapper: get delega, set/del/flush son no-op |

### Archivos modificados

| Archivo | Cambio |
|---|---|
| `crates/ixmati-cache/src/flashdb_store.rs` | Fix path: `"fdb_kvdb1"` → `data_dir`. Seteo `sec_size=4096` antes de init. |
| `crates/ixmati-cache/src/sqlite_backend.rs` | Agregado `fn flush(&self, _store: &str) {}` |
| `crates/ixmati-cache/src/lib.rs` | Registra `redb_backend`, `readonly`, exports |
| `crates/ixmati-cache/Cargo.toml` | Agregado `redb.workspace = true`, `rusqlite.workspace = true` |
| `Cargo.toml` (workspace) | Agregado `redb = "2"` |
| `crates/ixmati-api/Cargo.toml` | Agregado `ixmati-cache = { path = "../ixmati-cache" }` |
| `crates/ixmati-api/src/lib.rs` | `serve()` selecciona backend por `CACHE_BACKEND` env var |
| `crates/ixmati-api/src/rest.rs` | `AppState` con `cache: Arc<dyn CacheBackend>`, `read_handler` cache-aside |
| `crates/ixmati-api/src/main.rs` | Lee `CACHE_DIR` del entorno |
| `crates/ixmati-writer/src/main.rs` | Selecciona backend por `CACHE_BACKEND`, instancia `CacheSync` |
| `crates/ixmati-writer/src/cache_sync.rs` | Cambiado a `Arc<dyn CacheBackend>` (no genérico) |
| `containers/allinone/supervisord.conf` | Agrega `CACHE_BACKEND` y `CACHE_DIR` al environment |
| `containers/base/Containerfile` | Sin `--features flashdb` (compila sin FFI por defecto) |

---

## Bloqueador actual

**Bastion saturado de disco**:
- 192.168.3.175: cayó (no responde SSH)
- 192.168.3.143: 99% de disco (4.4G libres de 423G)
- `podman build` falla con `"no space left on device"`
- `podman system reset --force` y `podman image prune -af` hacen timeout por saturación

### Solución para desbloquear

```bash
# Opción A: Limpiar disco del bastion (requiere timeout largo)
ssh rafex@192.168.3.143 "podman system reset --force"

# Opción B: Limpieza manual si podman no responde
ssh rafex@192.168.3.143 "rm -rf ~/.local/share/containers/storage/overlay/*"
ssh rafex@192.168.3.143 "rm -rf ~/.local/share/containers/storage/overlay-images/*"
ssh rafex@192.168.3.143 "rm -rf ~/.local/share/containers/storage/overlay-layers/*"

# Opción C: Usar otro servidor con espacio suficiente
```

---

## Pasos pendientes (después de desbloquear disco)

### 1. Reestablecer túnel podman
```bash
ssh -fN -L 18081:/run/user/1000/podman/podman.sock rafex@192.168.3.143
podman info  # verificar conexión
```

### 2. Rebuild builder
```bash
podman build --network=host -f containers/base/Containerfile -t localhost/ixmati-builder:local .
```

### 3. Rebuild allinone
```bash
podman build --network=host -f containers/allinone/Containerfile -t localhost/ixmati-allinone:local .
```

### 4. Test comparativo: Redb
```bash
podman rm -f ixmati-allinone 2>/dev/null
podman run -d --name ixmati-allinone --network=host \
  -e CACHE_BACKEND=redb \
  -e CACHE_DIR=/var/lib/ixmati/cache \
  -e IXMATI_API_KEYS=smoke-test-key \
  -e STORE_NAME=default \
  -e SQLITE_PATH=/var/lib/ixmati/stores/default.db \
  localhost/ixmati-allinone:local

# Verificar logs
podman logs --tail=10 ixmati-allinone

# Write
curl -s -X POST http://<BASTION_IP>:30000/write \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer smoke-test-key" \
  -d '{"op":"upsert","store":"default","entity":"test","key":"r1","version":1,"ts":"2026-08-01T00:00:00Z","idempotency_key":"redb-test-1","ack_mode":"accepted","payload":{"data":"redb-test"}}'

sleep 2

# Read #1 (debe ser cache hit — writer ya pobló via CacheSync)
curl -s "http://<BASTION_IP>:30000/read?store=default&entity=test&key=r1"

# Read #2 (debe ser cache hit)
curl -s "http://<BASTION_IP>:30000/read?store=default&entity=test&key=r1"

# Métricas
curl -s http://<BASTION_IP>:30000/metrics | grep ixmati_cache
```

### 5. Test comparativo: SQLite
```bash
podman rm -f ixmati-allinone
podman run -d --name ixmati-allinone --network=host \
  -e CACHE_BACKEND=sqlite \
  -e CACHE_DIR=/var/lib/ixmati/cache \
  ... (mismo que arriba)

# Mismos tests que Redb
```

### 6. Test comparativo: FlashDB (opcional, requiere build con feature)
```bash
# Rebuild builder con --features flashdb
podman build --network=host --no-cache \
  -f containers/base/Containerfile \
  -t localhost/ixmati-builder:local \
  --build-arg CARGO_FEATURES=ixmati-cache/flashdb \
  .

# Rebuild allinone
podman build --network=host -f containers/allinone/Containerfile -t localhost/ixmati-allinone:local .

podman run -d --name ixmati-allinone --network=host \
  -e CACHE_BACKEND=flashdb \
  -e CACHE_DIR=/var/lib/ixmati/cache \
  ... (mismo que arriba)
```

### 7. Commit
```bash
git add -A
git commit -m "feat(cache): 3 backends (flashdb+redb+sqlite) con patron escritor unico

- RedbCacheBackend: redb 2.x con new() y new_readonly()
- ReadOnlyCache: wrapper que hace set/del/flush no-op para API
- SqliteCacheBackend: tabla _cache en SQLite separado, WAL
- FlashDB: fix path bug (fdb_kvdb1 → data_dir) + sec_size=4096
- CACHE_BACKEND env var: flashdb|redb|sqlite|noop
- Writer: unico proceso que escribe en cache via CacheSync
- API: solo lee cache (ReadOnlyCache para flashdb/redb)"
git push
```

### 8. Actualizar DEC-0009
```markdown
### DEC-0009 — RESUELTO: FlashDB path bug encontrado; Redb + SQLite como alternativas

- **Estado**: `accepted` → `replaced` por DEC-00XX
- **Resultado del spike**:
  1. FlashDB FFI compila en Linux x86_64 ✅
  2. FlashDB no persistía datos — bug en path parameter ❌ (fix aplicado)
  3. FlashDB SIGFPE por sec_size=0 ❌ (fix aplicado)
  4. FlashDB no soporta múltiples procesos — patrón escritor único ✅
- **Decisión**: mantener FlashDB como opción, agregar Redb como alternativa
  principal, SQLite como fallback. Seleccionable via CACHE_BACKEND.
```

---

## Matriz de backends

| Feature | FlashDB | Redb | SQLite |
|---|---|---|---|
| Lenguaje | C (FFI) | Rust puro | C (bundled) |
| Sin unsafe | ❌ | ✅ | ❌ |
| Read-only mode | ❌ (via wrapper) | ✅ (ReadOnlyDatabase) | ✅ (WAL) |
 delete_by_prefix | ✅ (iter) | ✅ (iter) | ✅ (LIKE) |
| Transaccional | ❌ | ✅ | ✅ |
| Tamaño binario | +0 (static link) | +2MB | +0 (ya bundled) |
| Madurez Linux | ⚠️ (microcontrolador) | ✅ (diseñado Linux) | ✅ (20+ años) |

---

## Iniciativas posteriores (post-cache)

1. **Iniciativa 2**: Activar projector (consumer MQTT + process_event)
2. **Iniciativa 3**: Popular métricas Prometheus
3. **Iniciativa 4**: Outbox cleanup periódico
4. **Iniciativa 5**: Store validation en API
5. **Iniciativa 6**: Backpressure configurable
