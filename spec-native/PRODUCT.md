# PRODUCT.md

Fuente de verdad del producto.

## Estado actual

Ixmati es un producto viable en **beta técnica**. La validación rate-controlled
en Debian amd64 sostuvo 40–120 solicitudes de escritura por segundo con el
throttle productivo de 40/s como perfil predeterminado, sin `202` ni crecimiento
de la cola de consumo. A 150/s apareció saturación observable; no es una cifra
de capacidad sostenible.

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
  - p99 de latencia de ack de escritura < 50ms en modo async.
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
- **Semántica de escritura flexible**: el backend elige entre async (baja latencia, consistencia eventual) y sync (read-your-writes garantizado) por comando.
- **Funciona con 1 store sin overhead**: el caso base no activa bus de eventos, outbox ni proyectores. La complejidad escala con la necesidad.
