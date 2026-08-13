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
  temporal elevado; el perfil productivo recomendado queda limitado a 10
  escrituras durables/s para conservar margen bajo el borde de latencia durable.
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

El perfil productivo recomendado queda en 10 escrituras durables/s por store,
con `Retry-After` y `/ready` para operación supervisada. El techo de 40/s es
un escalón de diagnóstico y no un SLO. Una prueba prolongada de estabilidad a
150–200/s sigue siendo necesaria para evaluar una futura ampliación.
La investigación de profiling adicional es optimización, no una falla de
durabilidad.

## Próximo ciclo de producto

- **En curso**: iniciativa `protobuf-api`: gRPC unary y streaming en `30100`,
  REST/Protobuf y compatibilidad REST/JSON. Falta cerrar integración real con
  clientes tonic/reqwest y medir impacto sobre ACK durable.
- **Completado para el perfil base**: `6c38eb8` sostuvo 10/s durante una hora
  con confirmación durable completa y drenado correcto. La prueba prolongada
  de 150/s y 200/s sigue pendiente y no debe extrapolarse desde el perfil
  base; esas tasas continúan siendo diagnóstico.
- **En curso**: ciclo de vida offline de stores (renombrar, merge, split),
  con tombstones, LWW determinista y reconstrucción mediante reconciler. El
  E2E Debian ya cubre merge/split y backup local; falta cutover de routing,
  topics antiguos, rollback y restore remoto.
- **Bloqueante para producción**: validar Litestream en el despliegue real
  (dos destinos, restore destructivo y RPO/RTO). La imagen sidecar no equivale
  a una recuperación probada y la instalación nativa todavía no lo arranca.
- Siguiente iniciativa: sharding interno de un store.
- Siguiente iniciativa: dashboard web de operación.

## Horizonte de alta disponibilidad

- Clustering de Mosquitto y topologías de alta disponibilidad.

Estas funcionalidades requieren decisiones de arquitectura y no forman parte
del alcance de v0.1 beta.
