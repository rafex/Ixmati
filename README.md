# Ixmati

Motor de escritura serializada para SQLite con soporte multi-backend. **Estado:
beta técnica**, validada en Debian amd64 con carga controlada.

## Qué es

Ixmati permite que múltiples backends o pods escriban en una misma instancia de SQLite sin contención. Las escrituras pasan por un canal de ingesta desacoplado (API REST/gRPC o MQTT) y son procesadas secuencialmente por un único writer. Las lecturas se sirven desde una cache rápida con fallback a SQLite. El instalador nativo y Compose incluyen Litestream v0.5; la réplica local y su restore están validados, mientras que el restore remoto S3, RPO/RTO y el segundo destino siguen siendo gates de producción abiertos.

## Uso rápido

Para probar el stack completo localmente:

```bash
cd examples/quickstart
docker compose up -d
./e2e-test.sh
```

La [guía de uso](docs/src/usage.md) cubre Docker, instalación nativa con
systemd, configuración, API REST, métricas, readiness (`/ready`), backup y
recuperación.

Para una instalación nativa Debian/amd64 desde un artefacto publicado:

```bash
tar xzf ixmati-<version>-linux-amd64.tar.gz
cd ixmati-<version>-linux-amd64
sudo IXMATI_API_KEYS='cambia-esta-clave' ./install.sh
systemctl status ixmati-litestream-file
```

El instalador fija Litestream `0.5.16`, instala una réplica local por store y
conserva la configuración existente durante reinstalaciones. Para habilitar la
réplica S3 se deben definir `IXMATI_LITESTREAM_S3_BUCKET`, opcionalmente
`IXMATI_LITESTREAM_S3_PREFIX`, `IXMATI_LITESTREAM_S3_REGION` y
`IXMATI_LITESTREAM_S3_ENDPOINT`, además de las credenciales AWS disponibles
para `ixmati-litestream-s3.service`. La réplica local no sustituye un backup
remoto ni demuestra por sí sola RPO/RTO.

La ruta S3-compatible puede probarse con el arnés reproducible
[`helpers/shell/test_litestream_s3.sh`](helpers/shell/test_litestream_s3.sh),
que usa MinIO fijado por digest, replica SQLite y restaura desde el objeto
remoto. Esta prueba valida compatibilidad de transporte; no reemplaza un
simulacro contra el bucket remoto de producción.

Una escritura durable se envía así:

```bash
curl -X POST http://localhost:8080/write \
  -H "Authorization: ApiKey $IXMATI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"op":"upsert","store":"pedidos","entity":"pedido","key":"ped_1","version":1,"idempotency_key":"pedido-1-v1","ack_mode":"committed","payload":{"total":1500}}'
```

`accepted` es un alias compatible de `committed`: `200` significa que el
commit SQLite quedó confirmado, mientras que `202` significa que la solicitud
quedó pendiente y debe consultarse con `GET /writes/{store}/{idempotency_key}`.

## Interfaces de acceso

La API REST escucha por defecto en `30000` y conserva JSON. La misma API
expone gRPC en `30100` y REST binario mediante `application/protobuf`; el
contrato está en [`proto/ixmati/v1/`](proto/ixmati/v1/) y la guía en
[`docs/src/api/grpc.md`](docs/src/api/grpc.md) y
[`docs/src/api/rest.md`](docs/src/api/rest.md). gRPC usa metadata `x-api-key`
cuando se habilita `IXMATI_API_KEYS`, y `GRPC_PORT=0` lo deshabilita para
instalaciones legacy.

El payload binario usa `google.protobuf.Struct` y debe ser un objeto JSON.
`accepted` sigue siendo alias durable de `committed`; `COMMITTED`/`200`
confirma `_idempotency`, mientras `PENDING`/`202` se consulta con
`GetWriteStatus` o `GET /writes/...`. El stream gRPC de eventos usa el cursor
durable de `_outbox`, replay acotado y entrega at-least-once.

## Capacidad y límites conocidos

- El throttle predeterminado es de 10 escrituras por segundo por store. Una
  corrida de una hora contra el SHA `6c38eb8`, con 200 clientes y generador no
  saturado, demostró este perfil como sostenible. La
  admisión usa un token bucket por store con una ráfaga máxima pequeña para no
  convertir el jitter normal del cliente en rechazos espurios.
- El perfil de despliegue usa `BATCH_INTERVAL_MS=100` y sincronización de cache
  post-commit con cola acotada (`CACHE_SYNC_QUEUE_CAPACITY=1000`); la cache no
  forma parte de la frontera de durabilidad.
- La primera ejecución exacta sobre `b023819` acumuló 283 `202/PENDING` en
  unos 12 minutos. Se corrigió la segunda ruta de escritura SQLite del
  publicador y la ejecución exacta posterior sobre `6c38eb8` obtuvo
  `36,001/36,001` respuestas `200`, p99 de `212.86 ms`, cero saturación,
  `integrity_check=ok` y outbox drenado durante una hora más cinco minutos.
- El candidato anterior de 15/s no fue estable: en una corrida prolongada
  acumuló respuestas `202/PENDING` por vencer el límite de confirmación de 2 s,
  aunque las claves terminaron comprometidas. Por eso 15/s queda como
  diagnóstico y no como perfil productivo.
- Una validación de capacidad con throttle temporal elevado en Debian amd64
  sostuvo 40–120 solicitudes/s sin `202` ni crecimiento de cola.
- En la comparativa completa, con el perfil de 40/s usado para diagnóstico, el
  writer confirmó aproximadamente 40 escrituras durables/s; no debe confundirse con la tasa
  de capacidad temporal anterior.
- 150 solicitudes/s produjo la primera señal de saturación; no se presenta
  como capacidad sostenible.
- La entrega de eventos es at-least-once: un crash en la ventana de PUBACK
  puede producir duplicados, pero no debe perder eventos confirmados.
- Cada store tiene un writer y un archivo SQLite; sharding interno, clustering
  de Mosquitto y failover transparente siguen fuera del alcance de esta beta.
- El publicador de eventos usa el mismo hilo dueño de SQLite para marcar
  `published_at`; cualquier build anterior a `6c38eb8` no debe usarse para
  validar capacidad.

## Comparativa de capacidad

La suite reproducible está en [`benchmarks/`](benchmarks/). Ejecuta SQLite
directo, PostgreSQL 18 e Ixmati en Debian con dataset, tasas y métricas
equivalentes:

```bash
just benchmark-db
```

La evidencia queda en
[`spec-native/evidence/DB-COMPARISON-20260811.md`](spec-native/evidence/DB-COMPARISON-20260811.md).
Los resultados medidos se mantienen separados de las referencias oficiales
de PostgreSQL; el ejemplo de `pgbench` publicado por PostgreSQL no es una
promesa de capacidad para este hardware.

Para una prueba prolongada reutilizable, el repositorio incluye
[`benchmarks/ixmati-soak.jmx`](benchmarks/ixmati-soak.jmx) y
[`benchmarks/soak_capacity.sh`](benchmarks/soak_capacity.sh). Ambos requieren
un contenedor Debian ya instalado y una tasa externa controlada; los escalones
de 150/s y 200/s sólo se consideran válidos tras una hora completa más cinco
minutos de drenado.
Para provisionar un contenedor nuevo por escalón está disponible
[`run_soak_debian.sh`](helpers/shell/run_soak_debian.sh).

Las operaciones de ciclo de vida de stores son offline. El binario
`ixmati-store-migrate` implementa rename, merge y split con tombstones,
conflictos LWW deterministas, backups y publicación atómica. El procedimiento
está en [`spec-native/RUNBOOK-STORE-MIGRATION.md`](spec-native/RUNBOOK-STORE-MIGRATION.md).

Resumen de la corrida Debian amd64 (1,000 usuarios, 10,000 pedidos):

| Camino | Resultado sostenible observado | Latencia representativa | Lectura |
|---|---:|---:|---|
| SQLite directo | 200 escrituras/s; 1,000 lecturas/s | p99 1.01 ms / 0.26 ms | baseline de motor |
| PostgreSQL 18 directo | 200 escrituras/s; 500 lecturas/s válidas | p99 7.19 ms / 4.17 ms | 1000/s saturó el cliente |
| Ixmati completo | ~40 escrituras/s; 1000 lecturas/s | p99 ~2.0 s / 1.64 ms | incluye API, MQTT, commit, cache y proyección |

El `~40/s` de Ixmati es el throughput durable confirmado por el writer, no la
tasa ofrecida al API. Desde 40–60/s aparecen `429` y pendientes; por eso la
tabla no presenta 100 o 200/s como capacidad productiva. Los números de
SQLite/PostgreSQL no son equivalentes al pipeline completo: sirven para
separar el costo del motor del costo de durabilidad, mensajería y proyección.

La comparación de interfaces está documentada en
[`PROTOBUF-BENCH-20260812.md`](spec-native/evidence/PROTOBUF-BENCH-20260812.md):
JSON, REST/Protobuf y gRPC confirmaron el mismo throughput durable bajo el
throttle productivo. Protobuf y gRPC son interfaces de transporte; no cambian
la frontera de commit ni deben anunciarse como una mejora de capacidad.

### Qué dicen estos resultados del producto

Ixmati no compite por throughput SQL bruto. La comparativa demuestra que
convierte la limitación de escritor único de SQLite en un servicio explícito:
coordina productores, confirma durabilidad mediante `_idempotency`, aplica
backpressure, publica mediante outbox y ofrece lecturas aceleradas por cache y
proyecciones.

La lectura es el lado fuerte del producto: el camino completo sostuvo 1,000
lecturas/s con p99 aproximado de 1.64 ms en este workload. El lado de escritura
es el límite actual: aproximadamente 40 escrituras durables/s y p99 cercana a
2 s en `ack_mode=committed`. Los `429` por encima del límite son una señal de
backpressure correcta, no capacidad adicional oculta.

La conclusión de producto es **viable para un perfil operativo acotado de
single-host/edge, escritura moderada y alta fan-out de lectura**. Para llamarlo
productivo en un entorno concreto todavía deben cerrarse backup remoto,
restauración medible, rollback y pruebas prolongadas del perfil elegido. Estos
resultados no justifican afirmar
que Ixmati supera a SQLite o PostgreSQL en capacidad bruta, que soporta
100–200 escrituras durables/s, ni que ya es un reemplazo general de PostgreSQL.
El siguiente trabajo de rendimiento debe concentrarse en la latencia del
writer y en la observabilidad de la cola; la prueba determinista de crash
entre PUBACK y `published_at` sigue pendiente.

## Stack

Rust (tokio, axum, tonic, rusqlite, rumqttc) · Mosquitto (persistence + QoS 1) · SQLite (WAL + synchronous=NORMAL) · FlashDB (cache) · Litestream 0.5.16 (replicación local validada; remoto pendiente de evidencia).

## Navegación

| Documento | Propósito |
|---|---|
| [`spec-native/PRODUCT.md`](spec-native/PRODUCT.md) | Problema, usuarios, objetivos, métricas |
| [`spec-native/ARCHITECTURE.md`](spec-native/ARCHITECTURE.md) | Opciones de arquitectura, módulos, restricciones |
| [`spec-native/STACK.md`](spec-native/STACK.md) | Tecnologías, versiones, notas de riesgo |
| [`spec-native/CONVENTIONS.md`](spec-native/CONVENTIONS.md) | Código, naming, envelope, errores, logging |
| [`spec-native/COMMANDS.md`](spec-native/COMMANDS.md) | Build, test, docker, resync, restore |
| [`spec-native/DECISIONS.md`](spec-native/DECISIONS.md) | Decisiones de arquitectura (ADRs) |
| [`spec-native/ROADMAP.md`](spec-native/ROADMAP.md) | Fases y prioridades |
| [`spec-native/TRACEABILITY.md`](spec-native/TRACEABILITY.md) | Vínculos entre artefactos |
| [`spec-native/SESSION.md`](spec-native/SESSION.md) | Estado activo de trabajo |
| [`TODO.md`](TODO.md) | Tablero de tareas |
| [`docs/src/usage.md`](docs/src/usage.md) | Guía de uso para desarrolladores y operadores |

### Especificación y tareas

- **Spec**: [`spec-native/specs/write-engine/SPEC.md`](spec-native/specs/write-engine/SPEC.md) — SPEC-WRITE-0001
- **Tareas**: [`spec-native/tasks/write-engine/TASKS.md`](spec-native/tasks/write-engine/TASKS.md) — TASK-WRITE-0001 a TASK-WRITE-0016

## Licencia

MIT. Ver [`LICENSE`](LICENSE).
