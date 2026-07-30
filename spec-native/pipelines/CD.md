# CD.md — Continuous Delivery Pipeline

## Visión general

El pipeline de release produce imágenes de contenedor versionadas para cada binario y las publica en un registry. Los artefacts se construyen una sola vez y se promueven entre entornos.

## Artefactos

Para cada release se producen:

| Artefacto | Descripción |
|---|---|
| `ixmati-api:<tag>` | API REST + gRPC |
| `ixmati-writer:<tag>` | Writer por store |
| `ixmati-projector:<tag>` | Runtime de proyecciones |
| `ixmati-supervisor:<tag>` | Orquestador de stores |
| `ixmati-reconciler:<tag>` | Reproductor offline |
| `ixmati-mosquitto:<tag>` | Broker MQTT con config de Ixmati |
| `ixmati-litestream:<tag>` | Sidecar Litestream por store |

## Estrategia de versionado

- Tags de imagen: `<major>.<minor>.<patch>-<short-sha>` para releases, `latest` para main
- Versión semántica desde `Cargo.toml` workspace
- Imágenes base: `debian:trixie-slim` (glibc, seguro para FlashDB FFI)

## Pipeline de release

1. **Build**: `make containers-build` → construye el builder compartido y todas las imágenes
2. **Tag**: las imágenes se tagean con `git describe --tags` + short SHA
3. **Test**: `just ci-main` contra la imagen de test
4. **Push**: las imágenes se empujan al registry (`ghcr.io/rafex/ixmati`)
5. **Release**: `gh release create` con changelog

## Entornos

| Entorno | Estrategia | Registry |
|---|---|---|
| Dev | Manual, `latest` | Registry local |
| Staging | Automático desde main | ghcr.io |
| Producción | Manual desde release tag | ghcr.io |

## Despliegue de stores

- K8s: 1 Deployment o StatefulSet por store, con PVC y sidecar Litestream
- Quadlet: `ixmati-writer@.container` unit template (1 instancia por store)
- El supervisor resuelve topología al iniciar

## Rollback

- Las imágenes son inmutables — rollback = deploy de tag anterior
- Litestream garantiza que los datos de SQLite por store son recuperables (RPO < 5s)
- Los read models en FlashDB son reconstruibles vía reconciler
