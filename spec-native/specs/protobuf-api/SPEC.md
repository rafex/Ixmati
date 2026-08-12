+++
id = "SPEC-PROTOBUF-0001"
title = "Integración de Protobuf, gRPC y REST binario"
state = "active"
owner = "team-core"
+++

# Objetivo

Agregar una interfaz binaria sobre el núcleo durable existente de Ixmati sin
romper el contrato REST/JSON. La primera versión expone gRPC unary para
escritura, estado, lectura y health; un stream server-side de eventos con
replay acotado y transición a live; y negociación REST mediante
`application/protobuf`.

## Contrato

- El package Protobuf es `ixmati.v1` y la fuente de verdad es `proto/`.
- El payload de negocio es `google.protobuf.Struct` en el campo wire 10 y
  representa un objeto JSON. `payload_bytes` conserva el campo wire 9 y queda
  deprecated para compatibilidad con clientes anteriores.
- `ReadResponse` conserva su antiguo `payload_bytes` en wire 6 y usa
  `payload` Struct en wire 9.
- `accepted` conserva la semántica durable de `committed`; no existe un modo
  async separado. `200`/`COMMITTED` confirma `_idempotency`; `202`/`PENDING`
  requiere consultar el estado.
- gRPC escucha en `GRPC_HOST:GRPC_PORT`, por defecto `127.0.0.1:30100`.
  `GRPC_PORT=0` deshabilita el listener para despliegues legacy.
- La autenticación gRPC usa metadata `x-api-key` y la misma configuración de
  claves que REST. Health permanece como servicio propio de Ixmati; no se
  habilitan reflection ni el health protocol estándar.

## Streaming

`EventService.SubscribeEvents` recibe store, filtros opcionales y un cursor
`after_outbox_seq`. El cursor es el `id` durable de `_outbox`; el servidor
reproduce el historial retenido y continúa consultando eventos nuevos. La
entrega es at-least-once, con deduplicación acotada y buffer por cliente. Un
cursor fuera de la retención devuelve `OUT_OF_RANGE`; un cliente que no pueda
mantenerse al día termina con `RESOURCE_EXHAUSTED`.

## Criterios de aceptación

1. Los `.proto` compilan reproduciblemente mediante `build.rs` y `make proto`.
2. REST/JSON conserva sus respuestas y códigos actuales.
3. REST/Protobuf y gRPC producen comandos equivalentes y comparten
   durabilidad, cache, SQLite, MQTT y autenticación.
4. Unary, conversión `Struct`, errores y replay/live tienen pruebas locales;
   integración real contra SQLite/MQTT se agrega antes de cerrar la iniciativa.
5. Las instalaciones nuevas publican REST en `30000` y gRPC en `30100`; los
   servicios legacy pueden usar `GRPC_PORT=0`.
