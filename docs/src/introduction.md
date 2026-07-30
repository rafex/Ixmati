## Qué es Ixmati

Ixmati es un motor de escritura serializada para SQLite. Permite que múltiples backends o pods envíen comandos de escritura concurrentes sin bloquearse entre sí ni corromper la base de datos.

La unidad atómica del sistema es el **store**: un archivo SQLite con su propio escritor, su propio prefijo de topic MQTT y su propio destino de backup. Con 1 store se obtiene un despliegue mínimo. Con N stores se obtiene aislamiento de fallo y evolución desacoplada por bounded context.

## Stack

Rust (tokio, axum, tonic, rusqlite, rumqttc) · Mosquitto (persistence + QoS 1) · SQLite (WAL + synchronous=NORMAL) · FlashDB (cache-aside + read models) · Litestream (backup continuo por store).

## Cómo funciona

1. El backend envía un **comando** a la API REST o gRPC.
2. La API publica el comando en `ixmati/cmd/<store>/<entity>/<id>` (Mosquitto).
3. El **writer** del store consume el comando, lo aplica en SQLite con `BEGIN IMMEDIATE`, e inserta un **evento** en `_outbox` dentro de la misma transacción.
4. El publicador lee `_outbox` y emite el evento en `ixmati/evt/<store>/<entity>/<id>`.
5. Los **proyectores** consumen eventos y actualizan read models en FlashDB.
6. Las **lecturas** se sirven desde cache-aside (`c:*`) o desde read models proyectados (`p:*`), con fallback a SQLite.
7. Litestream **replica** el WAL de cada store a S3 o a otro VPS.

## Para quién es

- Equipos con despliegues multi-pod que no quieren gestionar un RDBMS distribuido.
- Aplicaciones con muchas lecturas, escrituras moderadas y presupuesto de infraestructura bajo.
- Arquitecturas con bounded contexts que necesitan aislamiento de fallo sin microservicios completos.
