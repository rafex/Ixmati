# API gRPC

Ixmati sirve gRPC en `GRPC_HOST:GRPC_PORT`; por defecto
`127.0.0.1:30100`. REST continúa en `API_PORT` (normalmente `30000`). Para
despliegues legacy se puede usar `GRPC_PORT=0`.

El contrato compilable está en [`proto/ixmati/v1/`](../../../proto/ixmati/v1/)
y se genera durante el build con `tonic-prost-build`. No se habilitan
reflection ni el health protocol estándar.

## Servicios

```text
ixmati.v1.WriteService.Write
ixmati.v1.WriteService.GetWriteStatus
ixmati.v1.ReadService.Read
ixmati.v1.HealthService.Check
ixmati.v1.EventService.SubscribeEvents
```

`Write` recibe un `WriteRequest` con `WriteEnvelope`. El campo `payload` es un
`google.protobuf.Struct` (campo 10) que debe representar un objeto JSON; el
campo `payload_bytes` (campo wire 9) deprecated acepta JSON histórico sin
romper clientes anteriores. `store`, `entity`, `key` y `op` son obligatorios.

`ReadResponse.payload` usa el campo wire 9; el campo wire 6 (`payload_bytes`)
queda deprecated para conservar compatibilidad con respuestas antiguas.

La autenticación, cuando `IXMATI_API_KEYS` está habilitado, usa metadata:

```text
x-api-key: <clave-configurada>
```

Las respuestas mantienen la semántica durable de Ixmati:

- `COMMITTED`: `_idempotency` ya fue confirmado en SQLite.
- `PENDING`: el comando fue publicado, pero la confirmación no llegó dentro
  de la ventana; consultar `GetWriteStatus`.

## Eventos: replay y live

`SubscribeEvents` requiere `store` y acepta `entity`, `key` y
`after_outbox_seq`. El cursor es el `id` de `_outbox`; `0` comienza desde el
historial retenido. La entrega está ordenada por cursor para cada store,
reproduce el historial disponible y después espera eventos nuevos. El stream
es at-least-once y la deduplicación del servidor está acotada; el cliente debe
persistir su cursor y tolerar duplicados.

Si el cursor ya quedó fuera de la retención, el servidor devuelve
`OUT_OF_RANGE`. Un cliente que no puede consumir el buffer recibe
`RESOURCE_EXHAUSTED`. El historial no es ilimitado: depende de la limpieza de
`_outbox`.

## Cliente generado

Los tests deben usar el cliente generado por tonic, por ejemplo
`pb::write_service_client::WriteServiceClient` y
`pb::event_service_client::EventServiceClient`. `grpcurl` no es un requisito
de runtime porque no hay reflection.

Para regenerar/validar el contrato:

```bash
make proto
cargo test -p ixmati-api
```
