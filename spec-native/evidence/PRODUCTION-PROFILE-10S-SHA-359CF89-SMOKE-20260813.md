# Smoke operativo del perfil base — SHA `359cf89`

- Entorno: contenedor Debian trixie sobre Podman remoto `linux/amd64`
- Host Podman: `192.168.3.143` (generador interno con `PODMAN_HOST_IP=127.0.0.1`)
- Duración de carga: 60 s
- Drenado: 30 s
- Tasa objetivo: 10 escrituras durables/s
- Concurrencia del generador: 200
- Evidencia raw: `spec-native/evidence/raw/soak-10-20260813T035524Z/`

## Resultado

| Métrica | Resultado |
|---|---:|
| Solicitudes enviadas | 600 |
| Throughput real | 10.0/s |
| `200` | 600 |
| `202` | 0 |
| `429` | 0 |
| Errores | 0 |
| Saturación del generador | 0 ticks |
| p50 | 34.21 ms |
| p90 | 130.02 ms |
| p99 | 225.08 ms |
| Máxima | 226.07 ms |
| SQLite `integrity_check` | `ok` |
| Outbox pendiente después del drenado | 0 |

El smoke confirma que el hardening de límites de REST/gRPC y del stream no
regresó el perfil base en una instalación limpia del artefacto amd64. No
reemplaza la evidencia de una hora en `6c38eb8`; tampoco amplía la capacidad
publicable por encima de 10/s.
