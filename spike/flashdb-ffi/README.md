# Spike: viabilidad de FlashDB via FFI en Rust

**Tarea**: TASK-WRITE-0001
**Decisión relacionada**: DEC-0009
**Severidad**: Alta (FlashDB alojará read models, no solo cache)
**Criterio de salida**: Resultado documentado — FlashDB compila y funciona en Linux x86_64, o se descarta con justificación.

## Objetivo

Integrar FlashDB (librería C para microcontroladores) en Rust vía `bindgen` + `cc`. Determinar si:
1. Compila en Linux x86_64 (no solo ARM/embebidos)
2. Soporta operaciones get/set/delete con TTL
3. La superficie `unsafe` es manejable
4. La invalidación por prefijo es viable
5. El rendimiento comparado con `sled` justifica la complejidad

## Resultado (2026-07-30)

### Compilación

- **macOS aarch64**: OK
- **Linux x86_64 (Debian 13 trixie)**: OK. Verificado vía `podman build` con `rust:1.85-slim-bookworm`, `bindgen`, `cc`, `libclang-dev`. Imagen `flashdb-spike:linux` build exitoso, binario ejecuta sin crash.
- **Dependencia clave**: requiere `fdb_cfg.h` personalizado por plataforma. La configuración está incluida.

### Bindings generados

- `fdb_kvdb_init`, `fdb_kv_set`, `fdb_kv_get`, `fdb_kv_del`, `fdb_kv_get_blob` — disponibles y tipados.
- `fdb_tsdb_init`, `fdb_tsl_append` — disponibles.
- `fdb_blob_make`, `fdb_blob_read` — disponibles.
- `fdb_kv_iterator_init`, `fdb_kv_iterate` — disponibles (necesario para purga por prefijo).
- Tipos: `fdb_kvdb_t`, `fdb_tsdb_t`, `fdb_blob_t`, `fdb_err_t`.

### Superficie unsafe

Todas las funciones FFI son `unsafe extern "C"`. Esperado para cualquier binding C → Rust. La superficie es manejable: se puede encapsular en un wrapper Rust seguro con:
- Conversión de `fdb_err_t` → `Result<T, Error>`.
- Getters que copian bytes desde buffers C a `Vec<u8>`.
- RAII wrapper para los handles `fdb_kvdb_t`/`fdb_tsdb_t`.

### Invalidación por prefijo

FlashDB expone `fdb_kv_iterate()` para iterar sobre todas las keys. La invalidación por prefijo se implementaría iterando, filtrando keys por prefijo (`c:pedidos:*`) y llamando `fdb_kv_del()` para cada una. No hay `delete_by_prefix()` nativo — es iteración + delete. Viabilidad: sí, pero con overhead lineal.

### TTL

FlashDB KVDB no tiene TTL nativo. La implementación requeriría:
1. Guardar timestamp + TTL en el payload de cada KV.
2. En cada `get()`, verificar si el TTL expiró y retornar "no encontrado" (borrado lazy).
3. O un sweeper periódico que itere y borre entradas expiradas.

### Benchmark sintético vs sled

*Diferido:* El benchmark requiere implementar primero el wrapper Rust seguro y un harness que mida latencia y throughput en Linux x86_64. Se ejecutará como parte de `TASK-WRITE-0011` (ixmati-cache) cuando el trait `CacheBackend` tenga implementaciones concretas para ambos backends.

## Evaluación

| Criterio | Resultado |
|---|---|
| Compilación en macOS aarch64 | OK |
| Compilación en Linux x86_64 | OK |
| Bindings completos | OK (todas las funciones expuestas) |
| Get/Set/Delete | OK (API C expuesta) |
| TTL | Requiere implementación a nivel de aplicación |
| Invalidación por prefijo | Viable vía iteración + delete (no nativa) |
| Superficie unsafe | Manejable (wrapper seguro alrededor de FFI) |
| Rendimiento vs sled | Diferido a TASK-WRITE-0011 |

## Veredicto

FlashDB es **viable** como backend de storage vía FFI. Compila tanto en macOS aarch64 como en Linux x86_64 (Debian trixie, `rust:1.85-slim-bookworm`). Los bindings son completos para las APIs requeridas (`get`, `set`, `del`, iteración). La superficie `unsafe` es encapsulable.

**DEC-0009 resuelta:** se procede con FlashDB como backend de storage para `ixmati-cache`, `ixmati-projector` y `ixmati-reconciler`. El benchmark vs sled se ejecutará durante `TASK-WRITE-0011` (implementación de `CacheBackend`), donde ambos backends tendrán wrappers concretos comparables.
