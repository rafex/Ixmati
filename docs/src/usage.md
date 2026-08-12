# Guía de uso

Ixmati está en **beta técnica**. Esta guía cubre el recorrido mínimo para
desarrollar contra el servicio y operar una instalación pequeña en Linux.

## Elegir un despliegue

- **Docker/Podman**: recomendado para desarrollo y pruebas. Usa el ejemplo en
  [`examples/quickstart`](https://github.com/rafex/Ixmati/tree/main/examples/quickstart).
- **Debian nativo**: recomendado para una instalación persistente. Descarga el
  tarball de una release, ejecuta `install.sh` como root y deja que systemd
  gestione API, writer, cache-server y projector.
- **Kubernetes**: disponible para despliegues administrados; requiere adaptar
  almacenamiento, MQTT, secretos y backup al entorno del operador.

## Instalación nativa

Desde el artefacto de una release:

```bash
tar -xzf ixmati-<VERSION>-linux-amd64.tar.gz
cd ixmati-<VERSION>
sudo ./install.sh
```

El instalador configura Mosquitto, instala las unidades systemd y verifica el
health check. Para revisar el estado:

```bash
systemctl status ixmati-cache-server ixmati-api ixmati-projector
systemctl status 'ixmati-writer@*'
curl http://127.0.0.1:8080/health
```

La reinstalación conserva `stores.toml` y `projections.toml`. Para retirar el
software, usar `sudo ./install.sh --uninstall`; agregar `--purge` sólo cuando
también se quieran borrar datos y configuración.

## Primera escritura

La API requiere la clave configurada en el despliegue:

```bash
export IXMATI_API_KEY='cambia-esta-clave'
curl -sS -X POST http://127.0.0.1:8080/write \
  -H "Authorization: ApiKey $IXMATI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"op":"upsert","store":"pedidos","entity":"pedido","key":"ped_1","version":1,"idempotency_key":"pedido-1-v1","ack_mode":"committed","payload":{"total":1500,"estado":"pendiente"}}'
```

Con `ack_mode=committed`, la respuesta `200` confirma el commit en
`_idempotency`. `ack_mode=accepted` es un alias durable, no un modo de éxito
asíncrono. Si el commit no entra en el timeout, la API responde `202` y el
cliente debe consultar:

```bash
curl -sS http://127.0.0.1:8080/writes/pedidos/pedido-1
```

Los estados son `PENDING`, `APPLIED` y `REJECTED`. La clave de idempotencia es
la referencia estable para reintentar sin duplicar la escritura.

## Lecturas y operación diaria

Una lectura cache-aside usa `store`, `entity` y `key`:

```bash
curl -sS 'http://127.0.0.1:8080/read?store=pedidos&entity=pedido&key=ped_1'
```

Antes de declarar saludable un despliegue:

```bash
curl -sS http://127.0.0.1:8080/health
journalctl -u ixmati-api -u ixmati-projector --since '10 minutes ago'
journalctl -u 'ixmati-writer@*' --since '10 minutes ago'
```

Si se habilita `METRICS_PORT`, vigilar `outbox_size`,
`consumer_queue_depth`, el último commit de batch, errores MQTT y lag de
proyecciones. Un outbox o una cola que crece requiere reducir la tasa de
entrada antes de reiniciar servicios.

La entrega de eventos es at-least-once: el outbox se marca publicado después
del PUBACK, por lo que un reinicio puede repetir un evento. Los consumidores
deben deduplicar por identificador de evento.

## Capacidad de referencia

El throttle predeterminado de 40/s es el perfil de producción validado. En una
prueba de capacidad con throttle temporal elevado sobre Debian amd64 se
observaron 40–120/s sin respuestas `202` ni crecimiento de cola. A 150/s
aparecieron `202`, latencia de segundos y cola; ese escalón representa
saturación, no capacidad sostenible. En la comparativa completa con el perfil
productivo, el writer confirmó aproximadamente 40 escrituras durables/s y el
camino de lecturas cacheadas/proyectadas alcanzó 1,000 operaciones/s.

La p99 de las escrituras `ack_mode=committed` fue cercana a 2 s en esa
comparativa. Por eso Ixmati debe considerarse beta viable para escritura
moderada y alta fan-out de lectura, no un reemplazo de PostgreSQL para cargas
intensivas de escritura. Los `429` por encima del límite son backpressure
esperado y deben monitorizarse junto con el último commit, outbox y cola.

El sistema sigue siendo beta: el atasco de sesión MQTT bajo sobrecarga extrema
continúa como riesgo operativo conocido y la prueba determinista de crash en
la ventana PUBACK→`published_at` permanece pendiente.

Para migrar stores no se deben mover archivos con el servicio activo. Usa
`ixmati-store-migrate` en una ventana offline, valida el manifiesto, ejecuta
reconciler y sólo después vuelve a aceptar escrituras. Rename, merge y split,
incluyendo el algoritmo de hash y los criterios LWW, están descritos en el
[runbook de migración](../../spec-native/RUNBOOK-STORE-MIGRATION.md).

## Referencias

- [API REST](api/rest.md)
- [Configuración de stores](configuration/stores.md)
- [Runbook operativo](operations/runbook.md)
- [Observabilidad](operations/observability.md)
- [Disaster recovery](guides/disaster-recovery.md)
