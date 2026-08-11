# ROADMAP.md

Prioridades de mediano plazo para Ixmati. Las fases históricas de construcción
se consideran completadas; este documento describe el trabajo posterior al
producto viable beta.

## Estado v0.1 beta

- Core de escritura serializada, idempotencia, outbox y API REST/gRPC: **hecho**.
- Instalador nativo Debian, unidades systemd, cache-server y health checks:
  **hecho**.
- Backpressure, métricas y validación en Debian amd64: **hecho**.
- Capacidad rate-controlled validada: 40–120 solicitudes/s con throttle
  temporal elevado; el perfil productivo queda limitado a aproximadamente
  40 escrituras durables/s.
- Primer escalón de saturación observado: 150 solicitudes/s.
- Lecturas cacheadas/proyectadas validadas hasta 1,000 operaciones/s; p99 de
  escritura `ack_mode=committed` cercano a 2 s en la comparativa completa.

## Estado del hardening de producción

El ciclo de hardening quedó cerrado en `cc7b912`:

1. El writer mantiene ACK durable y el lookup de idempotencia usa un índice
   covering migrable en bases existentes. El baseline posterior al cambio
   sostuvo 39.0 escrituras/s aceptadas, p99 143.9 ms y cero saturación del
   generador; ver `spec-native/evidence/LOAD-POST-INDEX-20260811.md`.
2. El crash entre PUBACK y `published_at` fue probado con recuperación durable y
   duplicados at-least-once cuantificados.
3. Las alertas operativas pasan `promtool` y cubren writer, colas, outbox,
   MQTT, cache y lag de proyecciones.
4. El supuesto atasco MQTT se atribuyó al acceso no indexado de
   `_idempotency`; el transporte no requiere cambios. El watchdog queda como
   defensa ante pérdida auténtica de progreso.
5. La instalación, upgrade y validación de artefactos están automatizados.

El perfil productivo recomendado permanece en aproximadamente 40 escrituras
durables/s hasta completar una prueba prolongada de estabilidad a 150–200/s.
La investigación de profiling adicional es optimización, no una falla de
durabilidad.

## Próximo ciclo de producto

- Sharding interno de un store.
- Dashboard web de operación.
- Migración de stores (renombrar, merge, split).

## Horizonte de alta disponibilidad

- Clustering de Mosquitto y topologías de alta disponibilidad.

Estas funcionalidades requieren decisiones de arquitectura y no forman parte
del alcance de v0.1 beta.
