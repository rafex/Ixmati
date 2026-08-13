# Evidencia: aislamiento por API key y rutas de datos

- Fecha: 2026-08-13
- SHA de implementación: `5682fd07ecb77cdb56b6778a5ed69fabba8ebb7d`
- Rama: `main`
- Entorno local: macOS, pruebas de API con SQLite/Mosquitto temporales
- Entorno de distribución: contenedor `debian:trixie-slim` amd64 con systemd y
  Podman privilegiado

## Cambio validado

`IXMATI_API_KEY_SCOPES` usa el formato
`orders-key=orders|users;audit-key=audit`. El alcance se aplica a REST y gRPC
para escrituras, lecturas, estados y eventos. `/health`, `/ready` y `/metrics`
son las únicas rutas REST operativas públicas cuando la autenticación está
habilitada. El instalador persiste las claves y sus scopes en
`/etc/ixmati/ixmati.env`.

## Comandos y resultados

```text
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings  PASS
cargo test --workspace                                      PASS
just test-integration                                       PASS
make proto                                                   PASS
just validate-config                                         PASS
bash -n helpers/shell/*.sh                                  PASS
python3 -m py_compile helpers/python/installer.py           PASS
make dist dist-checksums dist-validate                       PASS
git diff --check                                             PASS
```

Pruebas específicas de API:

- `cargo test -p ixmati-api`: 53/53 pruebas exitosas.
- Clave scoped `orders-key`: acceso permitido a `orders` y HTTP 403 para
  lectura/estado de `users`.
- REST/Protobuf fuera de scope: HTTP 403 con `ErrorDetail.error =
  PERMISSION_DENIED`.
- REST sin credenciales con autenticación habilitada: HTTP 401 para `/read`.
- gRPC: identidad scoped rechaza stores no autorizados con
  `tonic::Code::PermissionDenied`.
- `helpers/shell/test_protobuf_e2e.sh`: REST JSON, REST/Protobuf, gRPC unary y
  replay/live pasaron; la compatibilidad JSON existente se mantuvo.

Validación del instalador Debian:

- `just installer-test`: instalación limpia, seis servicios activos,
  `IXMATI_API_KEY_SCOPES="ix-default-key=default"` persistido, write/read
  autenticados, restore local Litestream íntegro, reinstalación idempotente y
  desinstalación con purga; todo pasó.

## Límites

Esta evidencia demuestra aislamiento de API y persistencia de configuración.
No demuestra capacidad sostenible de 150–200/s, restore contra un bucket S3
real, RPO/RTO, TLS/reverse proxy ni cutover/rollback de stores. Esos gates
siguen pendientes y están separados en `SESSION.md` y `TRACEABILITY.md`.
