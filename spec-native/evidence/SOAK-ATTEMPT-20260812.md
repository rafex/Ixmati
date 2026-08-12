# Intento de soak prolongado

- Fecha UTC: 2026-08-12T03:37:24Z
- SHA local/publicado: `7e27d1b0c3c323b7192c68db3dc2daef8fa4260a`
- Escalón solicitado: `150/s`, duración `3600s`, drenado `300s`
- Arnés: `helpers/shell/run_soak_debian.sh`
- Destino: Podman `debian-server` → `192.168.3.175:22`

## Resultado

La ejecución no llegó a crear el contenedor ni a enviar tráfico. El
provisioning terminó con:

```text
unable to connect to Podman socket: failed to connect: dial tcp 192.168.3.175:22: connect: operation timed out
```

Las comprobaciones independientes confirmaron el bloqueo de infraestructura:

```text
ssh 192.168.3.175: Host is down
nc 192.168.3.175 22: Host is down
```

La conexión alternativa `debian-via-bastion` también expiró. No existe
resultado de throughput, latencia, métricas ni clasificación de capacidad para
este intento; no se debe contar como una ejecución parcial.

## Siguiente acción

Cuando `debian-server` vuelva a estar accesible, ejecutar primero:

```bash
SOAK_RATES=150 DURATION=3600 DRAIN_SECONDS=300 \
  TEST_HOST=192.168.3.175 helpers/shell/run_soak_debian.sh
```

Después repetir con `SOAK_RATES=200`. El perfil productivo permanece en 40/s
hasta obtener ambas evidencias válidas.
