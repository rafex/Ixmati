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

## Próximo ciclo — hardening de producción

1. Reducir la latencia de cola del writer y validar el SLO de escritura durable
   con `accepted`/`committed` como alias durable. Un modo async real queda como
   iniciativa independiente.
2. Prueba determinista del crash entre PUBACK y `published_at`.
3. Alertas de writer detenido, último batch, cola de consumo, outbox, errores
   MQTT y lag de proyecciones.
4. Investigar el atasco de sesión MQTT bajo sobrecarga extrema y definir una
   recuperación automática segura.
5. Automatizar la validación de releases, instalación idempotente y upgrade con
   los mismos artefactos publicados.

`TASK-VAL-0025` queda como investigación de profiling no bloqueante para la
beta; no debe confundirse con una falla de durabilidad.

## Horizonte posterior

- Sharding interno de un store.
- Dashboard web de operación.
- Migración de stores (renombrar, merge, split).
- Clustering de Mosquitto y topologías de alta disponibilidad.

Estas funcionalidades requieren decisiones de arquitectura y no forman parte
del alcance de v0.1 beta.
