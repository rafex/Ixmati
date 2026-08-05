# Benchmark Unit-Test — 3 Backends de Cache

> Fecha: 2026-08-05 | 1000 operaciones | Valores de 1KB

## Resultados

| Metrica | NoOp | SQLite | Redb 4.1.0 | FlashDB 2.2.0 |
|---|---|---|---|---|
| **Init** | 0ms | 60ms | 2ms | 8ms |
| **SET 1000** | 0ms | 1545ms (647/s) | 1351ms (740/s) | 1331ms (751/s) |
| **GET 1000** | 0ms | 36ms (27,777/s) | 69ms (14,493/s) | 1106ms (904/s) |
| **GET p50** | 0µs | 14µs | 26µs | 1,107µs |
| **GET p99** | 0µs | 34µs | 149µs | 1,179µs |
| **DEL 500** | 0ms | 34ms | 638ms | 544ms |
| **Size on disk** | 0B | 1.4MB (1392KB) | 2.9MB (2916KB) | 4KB |

## Analisis

### SQLite — Ganador en lecturas

- GET p50: **14µs** (el mas rapido)
- GET p99: **34µs** (latencia predecible)
- SET: 647 writes/s (el mas lento en escritura)
- Tamaño: 1.4MB (intermedio)
- Multi-proceso: ✅ WAL

**Recomendado para**: cache-aside donde las lecturas dominan (>90% reads).

### Redb — Balanceado

- GET p50: **26µs** (cercano a SQLite)
- SET: **740 writes/s** (el mas rapido)
- Init: **2ms** (arranque instantaneo)
- Tamaño: 2.9MB (el mayor, ~2x SQLite)
- Multi-proceso: ❌ (4.1.0 sin ReadOnlyDatabase, esperar a 4.5+)

**Recomendado para**: single-process donde se requiere alto throughput de escritura.

### FlashDB — No viable

- GET p50: **1,107µs** (80x mas lento que SQLite)
- GET throughput: **904 reads/s** (30x menos que SQLite)
- Tamaño: solo 4KB (compacto pero... sin datos persistentes)
- Mensaje de init: "All sector header is incorrect. Set it to default."
- Multi-proceso: ❌ (diseñado para microcontroladores)
- Persistencia: ❌ (no escribe a disco en Linux)

**No recomendado**. Diseñado para microcontroladores, no para servidores Linux.

## Datos crudos

```
test bench_noop ... ok
test bench_sqlite ... ok
test bench_redb ... ok
test bench_flashdb ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; finished in 2.99s
```

## Conclusion

Para el patron de cache-aside con multi-proceso en el all-in-one:

| Criterio | SQLite | Redb | FlashDB |
|---|---|---|---|
| Multi-proceso | ✅ WAL | ❌ | ❌ |
| GET p50 < 50µs | ✅ 14µs | ✅ 26µs | ❌ 1107µs |
| Persistencia | ✅ | ✅ | ❌ |
| Sin unsafe | ✅ | ✅ | ❌ FFI |
| Dependencias extra | 0 | redb crate | libclang-dev |

**Ganador**: **SQLite** (WAL multi-proceso, 14µs p50, sin dependencias nuevas).
