# containers/ — Ixmati Container Infrastructure

Podman rootless · target `linux/amd64` · builds ejecutan en host remoto vía túnel SSH.

## Registro de puertos

| Servicio | Puerto | Rango | Función |
|---|---|---|---|
| API REST | `30000` | Web | `ixmati-api` HTTP |
| Health / Metrics | `30001` | Web | `/health` + `/metrics` |
| mdBook docs | `30002` | Web | `just docs-serve` containerizado |
| API gRPC | `30100` | API | `ixmati-api` gRPC |
| Mosquitto MQTT | `30200` | DB | Broker persistente. QoS 1, `persistence true`. |
| Mosquitto WebSockets | `30201` | DB | `listener 30201 protocol websockets` |
| Test Mosquitto | `30310` | Temp | Broker para smoke tests |
| Test API REST | `30311` | Temp | API para smoke tests |
| Test API gRPC | `30312` | Temp | gRPC para smoke tests |

**Reservados por terceros y evitados**: `30080` · `30081` · `30083` · `30300` (contenedores existentes, hoy `Exited` pero pueden reiniciar).

## Build strategy

- `containers/base/Containerfile` → cargo-chef → `localhost/ixmati-builder:local` (compila los 5 binarios una vez)
- Cada servicio: `FROM localhost/ixmati-builder:local AS build` + copia su binario
- Runtime: `debian:trixie-slim` (glibc, coincide con el host, seguro para FlashDB FFI)
- `.containerignore` excluye `target/`, `.git/`, `docs/book/` (el contexto viaja por SSH: mantenerlo pequeño)

## Conexión esperada

```
podman (macOS) → tcp://127.0.0.1:18081 → túnel SSH → /run/user/1000/podman/podman.sock
```

Verificar: `podman info | grep arch` debe devolver `amd64`. Si devuelve `arm64`, abortar build (se está ejecutando local, no contra el remoto).

Las instalaciones nuevas publican REST en `30000` y gRPC en `30100`. El
listener gRPC puede deshabilitarse con `GRPC_PORT=0` durante una migración
legacy. El contrato binario se valida con `make proto`; no se requiere
`grpcurl` en runtime porque reflection no está habilitada.
