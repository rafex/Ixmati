# PRODUCT.md

Fuente de verdad del producto.

## Problema

Múltiples pods o backends necesitan escribir en una misma base de datos SQLite, pero SQLite solo soporta un escritor simultáneo. Abrir conexiones de escritura desde cada backend provoca contención, `SQLITE_BUSY`, y degradación de latencia. Se necesita un mecanismo que serialice las escrituras sin sacrificar la escalabilidad horizontal de los backends y sin introducir infraestructura pesada (Postgres, MySQL, etc.).

## Usuarios

- **Equipos con despliegues multi-pod**: necesitan escalar réplicas de su servicio sin que la base de datos sea el cuello de botella.
- **Aplicaciones con presupuesto de infraestructura bajo**: no pueden o no quieren gestionar un RDBMS distribuido; SQLite es suficiente para su volumen de datos.
- **Escenarios con alta lectura y escritura moderada**: el patrón más común en SaaS B2B, dashboards, y herramientas internas.

## Objetivos

- **Objetivo principal**: permitir que N backends (pods) envíen escrituras concurrentes sin bloquearse entre sí ni corromper la base de datos.
- **Métricas de éxito**:
  - 0 errores `SQLITE_BUSY` propagados al cliente en condiciones normales.
  - 0 mensajes perdidos ante caída y recuperación del writer (kill -9, reinicio).
  - p99 de latencia de ack de escritura < 50ms en modo async.
  - Orden de escrituras preservado por entidad (mismo `id` → mismo orden de aplicación).
  - Read-your-writes garantizado en modo sync (`ack=committed`).
  - RPO < 5s, RTO < 60s en disaster recovery vía Litestream.

## No objetivos

- Replicación multi-master o escritura concurrente directa a SQLite desde múltiples nodos.
- Sharding horizontal de datos (particionado de la base en múltiples archivos SQLite).
- Autenticación de negocio, autorización por roles, o MFA (eso es responsabilidad de las capas superiores).
- Alta disponibilidad automática con failover transparente (el writer es single-node; el failover es manual o vía orquestador).
- Reemplazar SQLite por otro motor de base de datos.

## Valor diferencial

- **Infraestructura mínima**: todo el stack (Rust binary + Mosquitto + SQLite) corre en un solo VPS o en pods pequeños. No requiere un operador de base de datos.
- **Escrituras desacopladas**: los backends no conocen SQLite; solo publican mensajes o llaman a una API. La serialización ocurre en un solo proceso, eliminando la contención.
- **Cache integrada**: FlashDB evita consultar SQLite para lecturas repetitivas, reduciendo la carga y mejorando la latencia de lecturas.
- **Backup continuo**: Litestream replica el WAL a uno o más destinos remotos con RPO de segundos.
- **Semántica de escritura flexible**: el backend elige entre async (baja latencia, consistencia eventual) y sync (read-your-writes garantizado) por request.
