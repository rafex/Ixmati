# Evidencia E2E — Protobuf, REST binario y gRPC

Fecha: 2026-08-12
SHA base del árbol: `48283cb0035110a951e71204eaf40a591880efb3`
Estado: **pasada funcional local; no es benchmark de capacidad**

## Alcance

Se validó el camino completo con procesos locales efímeros:

- Mosquitto en `127.0.0.1:18884`.
- REST en `127.0.0.1:30311`.
- gRPC en `127.0.0.1:30312`.
- writer, API y cache-server construidos desde el árbol actual.
- SQLite temporal y cache SQLite temporal.
- `google.protobuf.Struct` como payload canónico.

La prueba no mide throughput ni durabilidad bajo carga. Su objetivo es comprobar
que los dos protocolos llegan al mismo núcleo de escritura/lectura y que el
stream de eventos entrega el evento asociado a una escritura nueva.

## Comando reproducible

```bash
cargo build -p ixmati-api -p ixmati-writer -p ixmati-cache-server
helpers/shell/test_protobuf_e2e.sh
```

También queda disponible como:

```bash
just protobuf-e2e
```

El arnés genera las solicitudes binarias con `protoc`, ejecuta REST/Protobuf,
decodifica las respuestas y después ejecuta el cliente tonic:

```bash
IXMATI_E2E_GRPC=http://127.0.0.1:30312 \
IXMATI_E2E_API_KEY=protobuf-e2e-key \
cargo test -p ixmati-api --test protobuf_e2e -- --nocapture
```

## Resultado observado

| Camino | Operación | Resultado |
|---|---|---|
| REST/Protobuf | `GET /health` con `Accept: application/protobuf` | `STATUS_OK`; API, SQLite y Mosquitto OK |
| REST/Protobuf | `POST /write` con `WriteRequest` | `COMMITTED` |
| REST/Protobuf | `GET /writes/{store}/{key}` binario | `WRITE_STATUS_COMMITTED` / `APPLIED` |
| REST/Protobuf | `GET /read` binario | `found=true`, `source=cache` |
| REST/Protobuf | `POST /read` binario | `found=true`, `source=cache` |
| REST/JSON | `GET /health` con `Accept: application/json` | contrato JSON conservado |
| REST/Protobuf | cuerpo inválido | HTTP `400`, `ErrorDetail.error=INVALID_ARGUMENT` |
| REST/Protobuf | autenticación ausente | HTTP `401`, `ErrorDetail.error=UNAUTHORIZED` |
| gRPC | `HealthService.Check` | OK |
| gRPC | `WriteService.Write` | `COMMITTED` |
| gRPC | `WriteService.GetWriteStatus` | `WRITE_STATUS_COMMITTED` |
| gRPC | `ReadService.Read` | `found=true` |
| gRPC | `EventService.SubscribeEvents` | stream pasó; evento con `outbox_seq > 0` y payload correcto |

Resultado del test tonic: `1 passed, 0 failed`. También pasaron los casos gRPC
de metadata ausente/inválida (`UNAUTHENTICATED`), store requerido, cursor
negativo y status con argumentos ausentes (`INVALID_ARGUMENT`).

Durante la primera ejecución se encontró y corrigió un defecto real: el
middleware REST devolvía JSON para errores de autenticación incluso cuando el
cliente negociaba Protobuf. Ahora devuelve `ErrorDetail` binario cuando
`Accept` o `Content-Type` solicita `application/protobuf`, y mantiene JSON para
clientes JSON.

## Gates ejecutados antes de esta evidencia

- `cargo fmt --all -- --check` — pasa.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — pasa.
- `cargo test --workspace` — pasa.
- `just test-integration` — pasa.
- `make proto` — pasa.
- `just validate-config` — pasa.
- `bash -n helpers/shell/test_protobuf_e2e.sh` — pasa.
- `git diff --check` — pasa.
- `make dist dist-checksums dist-validate` — pasa.

`just docs-check-links` no se pudo ejecutar porque `mdbook` no está instalado
en el entorno local.

## Limitaciones y siguiente validación

El primer intento de smoke Compose quedó bloqueado por un defecto existente del
arnés: `containers/litestream/Containerfile` usa rutas relativas al repositorio,
mientras el contexto de build configurado por Compose es
`containers/litestream`. No se atribuye ese fallo al protocolo.

Esta evidencia tampoco cubre todavía:

- ejecución en Debian amd64 desde el SHA publicado;
- matriz de errores gRPC y autenticación ausente/incorrecta;
- `OUT_OF_RANGE`, cliente lento y backpressure del stream;
- benchmark REST JSON vs REST/Protobuf vs gRPC.

Esas pruebas permanecen pendientes antes de cerrar la iniciativa
`protobuf-api` o anunciar una mejora de capacidad.
