# API hardening — resource limits and slow event streams

- SHA probado: `1ae50a7`
- Fecha: 2026-08-13
- Alcance: REST JSON/Protobuf y gRPC de `ixmati-api`

## Cambios validados

1. REST aplica `MAX_REQUEST_BODY_BYTES`, con default de `1 MiB`, y devuelve
   `413 Payload Too Large` antes de decodificar un body excesivo.
2. gRPC aplica `GRPC_MAX_MESSAGE_BYTES` (`1 MiB` por defecto) a mensajes de
   entrada y salida y `GRPC_MAX_CONCURRENT_STREAMS=256` por conexión.
3. `EventService.SubscribeEvents` usa un buffer configurable por cliente
   (`EVENT_STREAM_BUFFER_CAPACITY=128` por defecto) y reserva un slot para
   `RESOURCE_EXHAUSTED`.
4. Un cliente que no consume eventos recibe el código `RESOURCE_EXHAUSTED`
   junto con `resume from cursor <outbox_seq>`; no termina con EOF silencioso.

## Evidencia reproducible

| Gate | Resultado |
|---|---|
| `cargo test -p ixmati-api` | 47 tests de unidad + E2E condicionado, 0 fallos |
| `cargo clippy -p ixmati-api --all-targets --all-features -- -D warnings` | OK |
| `cargo test --workspace` | OK |
| `just test-integration` | OK |
| `make proto` | OK |
| `just validate-config` | OK |
| `bash -n helpers/shell/*.sh benchmarks/*.sh` | OK |
| `python3 -m py_compile helpers/python/installer.py` | OK |
| Compose single-store/multi-store config | OK |
| `make dist dist-checksums dist-validate` | OK; artefact ELF linux/amd64 válido |
| `git diff --check` | OK |

Las pruebas específicas son
`rest::tests::oversized_write_body_is_rejected_before_handler` y
`grpc::tests::slow_event_client_receives_resource_exhausted_with_cursor`.

## Límites de esta evidencia

Este cambio protege memoria y hace observable el cierre de un stream lento; no
demuestra una capacidad durable nueva del writer. La capacidad de producción
sigue siendo el perfil validado de `10` escrituras durables/s por store. El
soak prolongado de `150/200` solicitudes/s, restore S3 real, RPO/RTO, TLS y
cutover/rollback de stores permanecen como gates operativos separados.
