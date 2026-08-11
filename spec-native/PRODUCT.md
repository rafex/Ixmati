# PRODUCT.md

Fuente de verdad del producto.

## Estado actual

Ixmati es un producto viable en **beta técnica**, orientado a cargas con alta
lectura y escritura durable moderada. En una prueba de capacidad con throttle
temporal elevado sostuvo 40–120 solicitudes/s y mostró el primer escalón de
saturación en 150/s. En la comparativa del pipeline completo con el perfil
productivo, el writer confirmó aproximadamente 40 escrituras durables/s y la
lectura cacheada/proyectada alcanzó 1,000 operaciones/s.

La comparativa también mostró una p99 cercana a 2 s para escrituras con
`ack_mode=committed`. Ese número es un cuello de botella actual del camino
durable, no una capacidad objetivo ni una razón para presentar Ixmati como
motor de alto throughput de escrituras. El producto coordina productores y
expone durabilidad, idempotencia, outbox y modelos de lectura; no pretende
superar a SQLite o PostgreSQL en throughput SQL bruto.

La operación sigue requiriendo administración de systemd, Mosquitto y
Litestream. La entrega de eventos es at-least-once y puede repetir eventos
alrededor de un crash; los consumidores deben deduplicar.

## Problema

Múltiples pods o backends necesitan escribir en una misma base de datos SQLite, pero SQLite solo soporta un escritor simultáneo por archivo. Abrir conexiones de escritura desde cada backend provoca contención, `SQLITE_BUSY`, y degradación de latencia. En arquitecturas con múltiples bounded contexts, además se necesita aislamiento de fallo (un dominio no puede tirar abajo a otro) y evolución desacoplada de esquemas (migrar `pedidos` sin afectar `usuarios`). Se necesita un motor que serialice las escrituras, aisle los dominios, y escale sin introducir infraestructura pesada (Postgres, MySQL, Kafka).

## Usuarios

- **Equipos con despliegues multi-pod**: necesitan escalar réplicas de su servicio sin que la base de datos sea el cuello de botella.
- **Aplicaciones con presupuesto de infraestructura bajo**: no pueden o no quieren gestionar un RDBMS distribuido; SQLite es suficiente para su volumen de datos.
- **Escenarios con alta lectura y escritura moderada**: el patrón más común en SaaS B2B, dashboards, y herramientas internas.
- **Arquitecturas con bounded contexts aislados**: equipos que necesitan independencia de esquema, backup y disponibilidad por dominio, pero sin la complejidad operativa de microservicios completos.

## Objetivos

- **Objetivo principal**: permitir que N backends (pods) envíen comandos de escritura concurrentes sin bloquearse entre sí ni corromper la base de datos, con aislamiento de fallo entre stores.
- **Métricas de éxito**:
  - 0 errores `SQLITE_BUSY` propagados al cliente en condiciones normales.
  - 0 comandos perdidos ante caída y recuperación del writer (kill -9, reinicio).
  - 0 eventos perdidos ante crash entre commit y publicación (outbox transaccional).
  - p99 de latencia de ACK durable dentro del guardrail operativo definido por
    la validación de producción; un modo async real no forma parte del
    contrato actual.
  - Orden de comandos preservado por `(store, entity, id)`.
  - Read-your-writes garantizado en modo sync (`ack=committed`), acotado al store del comando.
  - RPO < 5s, RTO < 60s en disaster recovery vía Litestream por store.
  - Lag de proyección p99 < 500ms en condiciones normales.
  - Caída de un store → los read models que lo referencian siguen sirviendo lecturas.
  - Escritura en store A no genera contención observable en store B.

## No objetivos

- Replicación multi-master o escritura concurrente directa a SQLite desde múltiples nodos.
- Sharding horizontal dentro de un store (particionado de carga, no de dominio).
- Autenticación de negocio, autorización por roles, o MFA (responsabilidad de capas superiores).
- Alta disponibilidad automática con failover transparente del writer (manual u orquestado externamente).
- Reemplazar SQLite por otro motor de base de datos.
- Transacciones cross-store ni orquestador de sagas (el motor provee primitivas; la aplicación coordina).
- JOIN SQL cross-store operacional (ATTACH read-only para analítica sí está permitido).

## Valor diferencial

- **Infraestructura mínima**: todo el stack (Rust binary + Mosquitto + SQLite) corre en un solo VPS o en pods pequeños. No requiere un operador de base de datos.
- **Comandos desacoplados**: los backends no conocen SQLite; solo publican comandos o llaman a una API. La serialización ocurre en un solo proceso por store, eliminando la contención.
- **Aislamiento de blast radius**: un store corrupto o caído no afecta a los demás. Cada store tiene su propio writer, su propio WAL, su propio backup y su propia frecuencia de replicación.
- **Cache dual**: FlashDB sirve como cache-aside (lazy, sin configuración) y como read model proyectado (eager, declarativo). El backend elige el camino óptimo por consulta.
- **0 eventos perdidos por diseño**: el outbox transaccional garantiza que cada comando aplicado genera exactamente un evento, sin dual-write, sin Debezium, sin triggers.
- **Backup continuo por store**: Litestream replica cada store a uno o más destinos remotos con RPO de segundos. Stores críticos pueden tener frecuencias más agresivas que stores no críticos.
- **Semántica de escritura durable**: `accepted` y `committed` son aliases compatibles y ambos garantizan read-your-writes cuando la API devuelve `200`; un modo async eventual requerirá una iniciativa y contrato separados.
- **Funciona con 1 store sin overhead**: el caso base no activa bus de eventos, outbox ni proyectores. La complejidad escala con la necesidad.

## Lectura de los resultados de capacidad

Los baselines de SQLite y PostgreSQL directos miden el motor con menos capas;
no son una comparación uno-a-uno contra el servicio completo. La diferencia
de latencia y throughput cuantifica el costo de añadir API, MQTT, serialización
del writer, confirmación en `_idempotency`, outbox, cache y proyecciones.

Por ello, la afirmación actual del producto es: **Ixmati convierte la
limitación de escritor único de SQLite en un servicio durable, observable y
con backpressure explícito; no elimina esa limitación**. Es una buena opción
para single-host/edge, escritura moderada y lecturas de alto fan-out. No debe
usarse todavía como reemplazo de PostgreSQL para cargas intensivas de escritura.
