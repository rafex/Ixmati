# Evidencia — migración offline de stores

- Fecha UTC: 2026-08-12
- Base Git: `2b726ba62f7d870fd407a19b8e2aa9b0766d56a0`
- Estado: cambios de esta ejecución aún no publicados
- Entorno: contenedor `debian:trixie-slim` con systemd, Podman remoto
- Artefacto: amd64 generado por `localhost/ixmati-builder:local`

## Preflight negativo

Se publicaron tres escrituras contra `default` y se detuvieron los servicios
sin esperar el publicador. El outbox tenía dos filas pendientes. El comando:

```text
ixmati-store-migrate execute --manifest /tmp/rename.toml
```

abortó con:

```text
precondition failed: default tiene 2 eventos outbox pendientes
```

No se creó ni publicó el destino.

## Rename y split

Después de arrancar el publicador y esperar outbox pendiente igual a cero:

- `rename default → orders`: 3 payloads, 3 idempotencias, 3 eventos, 0
  conflictos; destino publicado y verificado con checksum
  `2e111340ed020b59e37b900e1174ecd2194ad546c67eae292cb05dd48f1a4e37`.
- `split default → orders-0, orders-1`: 3 payloads, 3 idempotencias, 3
  eventos; `orders-0` recibió las tres claves en este dataset y ambos destinos
  pasaron `verify` e `integrity_check`.
- Checksums del split:
  - `orders-0`: `b7fddd876db6646e49a5eb4d0026d43ae1ee13e4a8cf7a29b3f8edcefc0caec8`
  - `orders-1`: `5abd9f3be14d422a59a33712e0a559803e8242636897ef1722db94e414b70847`

La corrida confirma el guard de outbox, la publicación atómica y la
reproducibilidad del split. No valida todavía el cutover de systemd,
reconciler/cache ni un merge conflictivo en el contenedor Debian; esos son
escenarios pendientes de `TASK-STORE-0005`.
