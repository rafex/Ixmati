# API REST

## Endpoints

| Método | Ruta | JSON | Protobuf |
|---|---|---|---|
| `POST` | `/write` | `application/json` | `application/protobuf` (`WriteRequest`) |
| `GET` | `/writes/{store}/{idempotency_key}` | `application/json` | `Accept: application/protobuf` |
| `GET` | `/read` | `application/json` | `Accept: application/protobuf` |
| `POST` | `/read` | — | `application/protobuf` (`ReadRequest`) |
| `GET` | `/health` | `application/json` | `Accept: application/protobuf` |
| `GET` | `/ready` | `application/json` | `Accept: application/protobuf` |

El esquema binario es el mismo árbol [`proto/ixmati/v1/`](../../../proto/ixmati/v1/)
que usa gRPC. Los errores Protobuf se serializan como `ErrorDetail` y
conservan el código HTTP del camino JSON.

## Escritura JSON

```bash
curl -X POST http://localhost:30000/write \
  -H 'Content-Type: application/json' \
  -d '{"op":"upsert","store":"pedidos","entity":"pedido","key":"p-1","version":1,"idempotency_key":"pedido-1-v1","ack_mode":"committed","payload":{"total":1500}}'
```

`accepted` es alias de `committed`; no existe un modo async separado. `200`
indica commit durable confirmado. `202` indica `PENDING` y debe consultarse
con `GET /writes/{store}/{idempotency_key}`.

## Escritura Protobuf

El body es la codificación binaria de `ixmati.v1.WriteRequest`. La forma
recomendada es usar `WriteEnvelope.payload` como `google.protobuf.Struct`:

```text
Content-Type: application/protobuf
Accept: application/protobuf
```

`payload` es el campo `Struct` canónico (wire 10). `payload_bytes` conserva el
campo wire 9 deprecated para clientes que aún envían el JSON binario
histórico. Los payloads no objeto se rechazan en el camino Protobuf; JSON
conserva su validación existente.

## Lectura y health Protobuf

Para una lectura GET:

```text
GET /read?store=pedidos&entity=pedido&key=p-1
Accept: application/protobuf
```

Para POST, el body es `ixmati.v1.ReadRequest` y la respuesta es
`ixmati.v1.ReadResponse`. `GET /health` devuelve `HealthCheckResponse` cuando
se solicita Protobuf. Cuando `IXMATI_API_KEYS` está configurado, `/write`,
`/read` y `/writes/...` requieren `Authorization: ApiKey <clave>`; `/health`,
`/ready` y `/metrics` permanecen públicos. Una clave scoped sólo puede usar
stores declarados en `IXMATI_API_KEY_SCOPES`; otro store devuelve HTTP 403 o
`ErrorDetail.PERMISSION_DENIED`.

`GET /health` es diagnóstico y conserva HTTP `200` aunque reporte un estado
degradado. `GET /ready` está destinado a load balancers y systemd: devuelve
`200` sólo cuando todos los SQLite configurados y MQTT están saludables; si
algún componente está degradado o no disponible devuelve `503`, también en
Protobuf.

Una escritura rechazada por backpressure (`429`) incluye `Retry-After`. El
cliente debe respetar ese valor y reintentar con la misma `idempotency_key`.

## Límites de entrada

Para evitar que un payload consuma memoria sin límite, REST rechaza con `413`
los cuerpos mayores que `MAX_REQUEST_BODY_BYTES`. El valor predeterminado es
`1048576` bytes y aplica tanto a JSON como a `application/protobuf`, incluido
`POST /read`. Se puede ajustar en `/etc/ixmati/ixmati.env` para un contrato de
payload conocido; el valor debe ser positivo.
