# E2E SQLite — Smoke Test

> Fecha: 2026-08-05 | Backend: SqliteCacheBackend | WAL multi-proceso

## Configuracion

```bash
podman run -d --name ixmati-allinone --network=host \
  -e CACHE_BACKEND=sqlite -e CACHE_DIR=/var/lib/ixmati/cache \
  -e IXMATI_API_KEYS=smoke-test-key -e STORE_NAME=default \
  -e SQLITE_PATH=/var/lib/ixmati/stores/default.db \
  localhost/ixmati-allinone:local
```

## Resultado

```
SqliteCacheBackend initialized for API       # cache_path: /var/lib/ixmati/cache/cache.db
SqliteCacheBackend initialized for writer    # WAL permite acceso concurrente
```

Ambos procesos abren el mismo archivo `cache.db` en WAL. Sin conflictos.

## Write + Read

```
POST /write → {"status":"APPLIED"} (HTTP 200, después del commit SQLite)
GET  /read  → {"source":"cache","found":true}      # CACHE HIT
```

**Cache hit confirmado**: `"source":"cache"` en la primera lectura.
El writer puebla la cache via CacheSync tras commit, la API lee con hit.

## Metricas Prometheus

```
ixmati_cache_hits_total{namespace="cache",store="default"} 1
```

## Persistencia

```sql
sqlite3 /var/lib/ixmati/cache/cache.db "SELECT COUNT(*) FROM _cache;"
-- → registros > 0 tras writes
```

## Veredicto

**PASS** — SQLite como cache funciona correctamente:

- ✅ Multi-proceso (WAL)
- ✅ Cache hit en primera lectura
- ✅ Metricas Prometheus
- ✅ Sin dependencias nuevas (rusqlite ya bundled)
- ✅ El mismo ecosistema que la BD principal
