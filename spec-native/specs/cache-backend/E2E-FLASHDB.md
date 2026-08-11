# E2E FlashDB — Smoke Test

> Fecha: 2026-08-05 | Backend: FlashDB vía FFI | Builder: Containerfile.flashdb

## Configuracion

```bash
podman run -d --name ixmati-allinone --network=host \
  -e CACHE_BACKEND=flashdb -e CACHE_DIR=/var/lib/ixmati/cache \
  -e IXMATI_API_KEYS=smoke-test-key -e STORE_NAME=default \
  -e SQLITE_PATH=/var/lib/ixmati/stores/default.db \
  localhost/ixmati-allinone:local
```

## Resultado de inicializacion

```
FlashDB initialized for API (read-only)     # API: ReadOnlyCache<FlashDb>
FlashDB initialized for writer               # Writer: FlashDb directo
```

Ambos procesos inicializan FlashDB sin SIGFPE. El fix de `sec_size=4096` funciona.

## Write + Read

```
POST /write → {"status":"APPLIED"} (HTTP 200, después del commit SQLite)
GET  /read  → {"source":"sqlite"}
```

**Cache miss**: FlashDB no retorna los datos que CacheSync escribio.
La API lee via ReadOnlyCache → FlashDB.get() → None → fallback SQLite.

## Persistencia en disco

```
ls /var/lib/ixmati/cache/ → directorio vacio
```

No hay archivos creados tras writes. `fdb_kv_set_blob` reporta `FDB_NO_ERR` pero
no escribe a disco en file mode (`FDB_USING_FILE_LIBC_MODE`).

## Veredicto

**FAIL** — FlashDB no persiste datos en Linux x86_64 con file mode.
Tres bugs intentados sin exito:

| Bug | Fix | Resultado |
|---|---|---|
| Path incorrecto (`"fdb_kvdb1"`) | `CString::new(data_dir)` | Inicializa pero no escribe |
| `sec_size=0` → SIGFPE | `(*kvdb).parent.sec_size = 4096` | Ya no crashea |
| Multi-proceso | API usa ReadOnlyCache | No aplica (no persiste single-process) |

**Conclusion**: FlashDB no es viable para Ixmati. Diseñado para microcontroladores,
su file mode en Linux no funciona. Criterio de salida DEC-0009 activado.

## Evidencia

```
{"timestamp":"...","level":"INFO","fields":{"message":"FlashDB initialized for API (read-only)",...}}
{"timestamp":"...","level":"INFO","fields":{"message":"FlashDB initialized for writer",...}}
{"status":"APPLIED","store":"default","idempotency_key":"fdb2-1","message":"committed"}
{"entity":"test","found":true,"key":"fdb2","payload":{"data":"flashdb-works"},"source":"sqlite","store":"default"}
```
