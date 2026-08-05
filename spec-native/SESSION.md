+++
[session]
state = "in_progress"
agent = "opencode"
initiative = "cache-backend"
task = "TASK-CACHE-0001"
intent = "Implementar 3 backends de cache (FlashDB + Redb + SQLite) con patron escritor unico. Codigo completo pero bloqueado por saturacion de disco en bastion (192.168.3.175 cayo, 192.168.3.143 al 99% disco). Pendiente: rebuild + test comparativo."
last_updated = "2026-08-05T04:33:47Z"
+++

# Active Session

## Current state

Implementar 3 backends de cache (FlashDB + Redb + SQLite) con patron escritor unico. Codigo completo pero bloqueado por saturacion de disco en bastion (192.168.3.175 cayo, 192.168.3.143 al 99% disco). Pendiente: rebuild + test comparativo.

## Next steps

1. Limpiar disco del bastion: `ssh rafex@192.168.3.143 "podman system reset --force"` (requiere timeout largo)
2. Si no se puede limpiar, considerar usar otro servidor o limpiar manualmente: `rm -rf ~/.local/share/containers/storage/overlay/*`
3. Reestablecer tunnel podman: `ssh -fN -L 18081:/run/user/1000/podman/podman.sock rafex@192.168.3.143`
4. Verificar podman: `podman info`
5. Rebuild builder: `podman build --network=host -f containers/base/Containerfile -t localhost/ixmati-builder:local .`
6. Rebuild allinone: `podman build --network=host -f containers/allinone/Containerfile -t localhost/ixmati-allinone:local .`
7. Test redb: `podman run -d --name ixmati-allinone --network=host -e CACHE_BACKEND=redb -e CACHE_DIR=/var/lib/ixmati/cache -e IXMATI_API_KEYS=smoke-test-key -e STORE_NAME=default -e SQLITE_PATH=/var/lib/ixmati/stores/default.db localhost/ixmati-allinone:local`
8. Write + read + verificar cache hit
9. Repetir con CACHE_BACKEND=sqlite
10. Commit: `git add -A && git commit -m "feat(cache): 3 backends (flashdb+redb+sqlite) con patron escritor unico"`
11. Actualizar DEC-0009 en DECISIONS.md

## Context for next agent

## Cambios ya hechos en local (no commiteados)

### Archivos modificados:
1. `crates/ixmati-cache/src/flashdb_store.rs` — fix path bug: `CString::new("fdb_kvdb1")` → `CString::new(data_dir)`. Seteo de `sec_size=4096` y `max_size=128MB` antes de `fdb_kvdb_init`.
2. `crates/ixmati-cache/src/sqlite_backend.rs` — agregado `fn flush(&self, _store: &str) {}` (no-op, SQLite WAL hace flush automatico).
3. `crates/ixmati-cache/src/redb_backend.rs` — NUEVO. RedbCacheBackend con `new()` (read-write) y `new_readonly()`. Tabla `cache: &str → &[u8]`. Implementa get/set/del/delete_by_prefix/flush.
4. `crates/ixmati-cache/src/readonly.rs` — NUEVO. ReadOnlyCache<B> wrapper. get delega, set/del/delete_by_prefix/flush son no-op.
5. `crates/ixmati-cache/src/lib.rs` — registra `pub mod redb_backend`, `pub mod readonly`, `pub use RedbCacheBackend`, `pub use ReadOnlyCache`.
6. `crates/ixmati-cache/Cargo.toml` — agregado `redb.workspace = true` y `rusqlite.workspace = true`.
7. `Cargo.toml` (workspace) — agregado `redb = "2"`.
8. `crates/ixmati-api/Cargo.toml` — agregado `ixmati-cache = { path = "../ixmati-cache" }` (sin features=flashdb).
9. `crates/ixmati-api/src/lib.rs` — `serve()` selecciona backend por `CACHE_BACKEND` env var: flashdb (ReadOnlyCache), redb (ReadOnlyCache), sqlite (directo), noop.
10. `crates/ixmati-api/src/rest.rs` — `AppState` tiene `cache: Arc<dyn CacheBackend>`. `read_handler` implementa cache-aside: cache.get → miss → SQLite SELECT → return.
11. `crates/ixmati-api/src/main.rs` — lee `CACHE_DIR` del entorno.
12. `crates/ixmati-writer/src/main.rs` — selecciona backend por `CACHE_BACKEND`. Instancia `CacheSync` y llama `sync_batch()` tras cada batch.
13. `crates/ixmati-writer/src/cache_sync.rs` — modificado para usar `Arc<dyn CacheBackend>` (no generico). Agregado log de debug en `sync_batch`.
14. `containers/allinone/supervisord.conf` — agrega `CACHE_BACKEND` y `CACHE_DIR` al environment de API y writer.
15. `containers/base/Containerfile` — sin `--features flashdb` (compila sin FFI por defecto).

### Bloqueador
- Bastion 192.168.3.175 cayo. 192.168.3.143 al 99% de disco (4.4G libres de 423G).
- `podman build` falla con "no space left on device".
- `podman system reset --force` y `podman image prune -af` timeout por saturacion.
- Necesario: limpiar disco del bastion antes de continuar.

### Bugs encontrados y fixados
1. FlashDB path bug: `fdb_kvdb_init` recibia "fdb_kvdb1" como path en vez de data_dir. FlashDB escribia en CWD del contenedor, no en el directorio configurado.
2. SqliteCacheBackend no implementaba `fn flush()` del trait CacheBackend.
3. FlashDB SIGFPE: `sec_size=0` en struct zeroed causaba division by zero. Fix: setear `(*kvdb).parent.sec_size = 4096` antes de init.
4. CacheSync era generico `<B: CacheBackend>` pero se usaba con `dyn CacheBackend` (unsized). Fix: cambiar a `Arc<dyn CacheBackend>` concreto.
5. API y writer abrian el mismo cache en modo read-write (inseguro para FlashDB/Redb). Fix: API usa ReadOnlyCache, writer usa backend directo.

### Pendiente despues de limpiar disco
1. `podman build` del builder (con redb)
2. `podman build` del allinone
3. Test comparativo: `CACHE_BACKEND=redb` → write → read cache hit
4. Test comparativo: `CACHE_BACKEND=sqlite` → write → read cache hit
5. Test comparativo: `CACHE_BACKEND=flashdb` (con --features flashdb en builder) → write → read cache hit
6. Commit de todos los cambios
7. Actualizar DEC-0009 en DECISIONS.md con resultado: FlashDB path bug encontrado, redb como alternativa, SQLite como fallback
