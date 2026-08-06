# Ixmati Load Test — Comparativa de Modos de Cache

> Fecha: 2026-08-06 | Writes: 100 | Reads por nivel: 1000 | Concurrencia: 1-1000

## Resultados

| Mode | Backend | Concurrente | p50 ms | p99 ms | p999 ms | reads/s | Errors |
|------|---------|-------------|--------|--------|---------|---------|--------|
| direct | sqlite | 1 | 1.07 | 6.03 | 40.51 | 750 | 0 |
| direct | sqlite | 10 | 7.79 | 76.93 | 101.64 | 1065 | 0 |
| direct | sqlite | 50 | 38.12 | 104.05 | 142.67 | 966 | 0 |
| direct | sqlite | 100 | 40.82 | 112.14 | 129.26 | 907 | 0 |
| direct | sqlite | 200 | 40.03 | 121.07 | 153.18 | 885 | 0 |
| direct | sqlite | 500 | 41.36 | 114.35 | 156.44 | 880 | 0 |
| direct | sqlite | 1000 | 39.00 | 110.72 | 146.60 | 885 | 0 |
| **socket** | **redb** | **1** | **0.84** | **3.13** | **40.37** | **1023** | **0** |
| **socket** | **redb** | **10** | **5.46** | **50.45** | **90.82** | **1546** | **0** |
| **socket** | **redb** | **50** | **19.32** | **51.24** | **60.50** | **1887** | **0** |
| **socket** | **redb** | **100** | **21.32** | **55.14** | **70.57** | **1648** | **0** |
| **socket** | **redb** | **200** | **20.71** | **53.50** | **90.75** | **1758** | **0** |
| **socket** | **redb** | **500** | **17.52** | **53.19** | **68.22** | **1975** | **0** |
| **socket** | **redb** | **1000** | **17.59** | **61.52** | **78.16** | **1957** | **0** |
| mqtt | redb | 1 | 1.59 | 7.11 | 45.62 | 504 | 0 |
| mqtt | redb | 10 | 12.59 | 28.60 | 80.73 | 740 | 0 |
| mqtt | redb | 50 | 59.44 | 90.02 | 113.71 | 786 | 0 |
| mqtt | redb | 100 | 51.87 | 121.31 | 137.19 | 834 | 0 |
| mqtt | redb | 200 | 50.72 | 119.99 | 176.07 | 853 | 0 |
| mqtt | redb | 500 | 82.53 | 178.77 | 210.78 | 816 | 0 |
| mqtt | redb | 1000 | 68.81 | 172.12 | 247.43 | 848 | 0 |

## Análisis

### Hallazgo #1: socket es más rápido que direct a TODOS los niveles

| Concurrencia | Direct p50 | Socket p50 | Socket vs Direct |
|---|---|---|---|
| 1 | 1.07ms | **0.84ms** | **1.3x más rápido** |
| 10 | 7.79ms | **5.46ms** | **1.4x más rápido** |
| 50 | 38.12ms | **19.32ms** | **2.0x más rápido** |
| 100 | 40.82ms | **21.32ms** | **1.9x más rápido** |
| 1000 | 39.00ms | **17.59ms** | **2.2x más rápido** |

**Causa**: el modo `direct` abre una conexión SQLite (`rusqlite::Connection::open`) + prepara statement + ejecuta query POR CADA READ. Esto añade ~1ms de overhead. El modo `socket` reusa una conexión persistente Unix socket — solo envía `GET key\n` y recibe `HIT len\n` sin overhead de conexión.

### Hallazgo #2: socket escala mejor bajo carga

| Concurrencia | Direct throughput | Socket throughput | Ganancia socket |
|---|---|---|---|
| 1 | 750 reads/s | **1023 reads/s** | 1.4x |
| 10 | 1065 reads/s | **1546 reads/s** | 1.5x |
| 1000 | 885 reads/s | **1957 reads/s** | **2.2x** |

A mayor concurrencia, mayor ventaja de socket. Direct se degrada antes por contención en SQLite WAL (aunque estable en p50~40ms). Socket mantiene mayor throughput hasta 1957 reads/s.

### Hallazgo #3: mqtt es el más lento pero el más desacoplado

| Concurrencia | MQTT p50 | vs Direct | vs Socket |
|---|---|---|---|
| 1 | 1.59ms | 1.5x más lento | 1.9x más lento |
| 1000 | 68.81ms | 1.8x más lento | 3.9x más lento |

MQTT añade ~0.5ms de overhead fijo (QoS 0 round-trip al broker Mosquitto local). A 1000 concurrentes, crece a 68ms porque Mosquitto es el cuello de botella.

### Hallazgo #4: todos los modos son estables

- Cero errores en los 21 tests (1000 reads × 3 modos × 7 niveles)
- Health check OK después de cada modo
- Sin degradación progresiva: p50 se mantiene dentro del mismo orden de magnitud

### Hallazgo #5: overhead de conexión domina la latencia

El benchmark unitario mostró SQLite GET en 28µs (microsegundos). La prueba de carga muestra 1.07ms (milisegundos) — **38x más lento**. La diferencia es el overhead de:
1. HTTP round-trip (axum + TCP)
2. `rusqlite::Connection::open` por cada request
3. `conn.prepare()` + `stmt.query_row()` + `serde_json::from_slice`

El modo `socket` elimina (2) y (3) porque el writer ya tiene la conexión abierta. Solo queda el overhead del Unix socket (syscall).

## Conclusión

| Criterio | Direct | Socket | MQTT |
|---|---|---|---|
| Latencia p50 (1 conc) | 1.07ms | **0.84ms** | 1.59ms |
| Latencia p50 (1000 conc) | 39.00ms | **17.59ms** | 68.81ms |
| Throughput max | 1065/s | **1975/s** | 853/s |
| Backend compatible | SQLite | Redb, FlashDB | Redb, FlashDB |
| Complejidad | Mínima | Unix socket server | MQTT broker |
| Desacoplamiento | Ninguno | Medio (IPC) | Alto (broker) |

**Recomendación**:

1. **SQLite**: usar `direct` para máxima simplicidad (menos componentes). El overhead de conexión SQLite por request es aceptable (~1ms) si el número de reads concurrentes es bajo (<50).

2. **Redb**: usar `socket` como modo default. **2.2x más rápido que direct a 1000 concurrentes** y mantiene el writer como único proceso que abre el archivo de cache.

3. **MQTT**: usar solo si se necesita desacoplamiento total (API y writer en contenedores separados sin filesystem compartido). 3.9x más lento que socket a alta carga.

### Respuesta a la pregunta original

> ¿A qué concurrencia los modos se emparejan?

**Nunca se emparejan**. Socket es SIEMPRE más rápido que direct (1.3x a 2.2x). MQTT es SIEMPRE más lento (1.5x a 3.9x). Las diferencias se acentúan con mayor carga, no disminuyen.

Esto contradice la hipótesis inicial (que SQLite WAL sería más rápido). La razón es que el modo `direct` actual no reusa la conexión SQLite entre requests — abre una nueva cada vez. Una optimización futura sería mantener un pool de conexiones SQLite en la API.
