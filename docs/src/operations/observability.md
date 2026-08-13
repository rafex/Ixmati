# Observabilidad

Los procesos de writer y projector exponen `/metrics` sólo cuando se asigna
un `METRICS_PORT` único a cada instancia. Esto evita colisiones en despliegues
con varios stores; el instalador no abre puertos por defecto.

## Métricas principales

| Métrica Prometheus | Proceso | Uso |
|---|---|---|
| `ixmati_write_requests_total` | API | Solicitudes de escritura por estado |
| `ixmati_write_latency_seconds` | API | Latencia HTTP de escritura |
| `ixmati_queue_depth` | API | Cola de comandos hacia MQTT |
| `ixmati_outbox_size` | API | Eventos sin `published_at` |
| `ixmati_consumer_queue_depth` | writer | Cola entre MQTT y el batcher |
| `ixmati_last_batch_commit_unix_seconds` | writer | Último commit SQLite exitoso |
| `ixmati_write_batch_duration_seconds` | writer | Escritura y espera del hilo SQLite |
| `ixmati_batch_fill_duration_seconds` | writer | Tiempo de acumulación del batch |
| `ixmati_batch_ack_duration_seconds` | writer | Tiempo de ACK después del commit |
| `ixmati_cache_sync_duration_seconds` | writer | Sincronización post-commit con cache |
| `ixmati_cache_sync_errors_total` | writer | Fallos de cache después de un commit |
| `ixmati_mqtt_consumer_connected` | writer/projector | Estado de conexión MQTT |
| `ixmati_mqtt_consumer_last_event_unix_seconds` | writer | Último comando recibido |
| `ixmati_mqtt_consumer_last_ack_unix_seconds` | writer | Último comando confirmado |
| `ixmati_mqtt_eventloop_errors_total` | writer/projector | Errores de los event loops |
| `ixmati_mqtt_ack_failures_total` | writer | ACKs de comandos que fallaron |
| `ixmati_outbox_publish_attempts_total` | writer | Filas entregadas al cliente MQTT |
| `ixmati_outbox_puback_timeouts_total` | writer | Filas sin PUBACK dentro del timeout |
| `ixmati_outbox_published_total` | writer | Filas marcadas después de PUBACK |
| `ixmati_outbox_mark_failures_total` | writer | Fallos al marcar filas confirmadas |
| `ixmati_outbox_mark_duration_seconds` | writer | Duración del marcado durable posterior a PUBACK |
| `ixmati_outbox_puback_queue_drops_total` | writer | PUBACKs descartados por cola acotada; el outbox se reintenta |
| `ixmati_outbox_last_puback_unix_seconds` | writer | Último PUBACK del outbox |
| `ixmati_projector_events_processed_total` | projector | Eventos procesados |
| `ixmati_projector_last_event_unix_seconds` | projector | Último evento procesado |
| `ixmati_projection_lag_seconds` | projector | Histograma de `now - occurred_at` |

Los counters aparecen con sufijo `_total` en Prometheus aunque en el código se
declaren sin ese sufijo, porque lo añade el exporter OpenTelemetry.

## Activar scraping

Para una unidad systemd template, crear un drop-in con un puerto libre por
store, por ejemplo:

```ini
# systemctl edit ixmati-writer@pedidos
[Service]
Environment=METRICS_PORT=9101
```

El projector necesita otro puerto:

```ini
# systemctl edit ixmati-projector
[Service]
Environment=METRICS_PORT=9102
```

Después:

```bash
sudo systemctl daemon-reload
sudo systemctl restart ixmati-writer@pedidos ixmati-projector
curl -fsS http://127.0.0.1:9101/metrics
curl -fsS http://127.0.0.1:9102/metrics
```

## Diagnóstico de alertas

- `IxmatiWriterCommitStalled`: confirmar que `mqtt_commands_enqueued_total`
  aumenta y `last_batch_commit_unix_seconds` no. Revisar SQLite, el hilo de
  escritura y el journal del writer.
- `IxmatiQueueGrowing`: comparar `consumer_queue_depth` con
  `write_batch_duration_seconds` y `batch_fill_duration_seconds`. Una cola
  estable durante tráfico no es una alerta.
- `IxmatiOutboxStalled`: comprobar `outbox_size`, los intentos de publicación,
  `outbox_puback_timeouts_total` y la conexión de Mosquitto.
- `IxmatiMqttErrors` o `IxmatiMqttAckFailures`: revisar errores del event loop,
  `last_ack`, la sesión persistente y los logs del broker antes de modificar
  QoS o semántica de ACK.
- `IxmatiProjectionLag`: consultar el p99 de
  `projection_lag_seconds`, `projector_last_event_unix_seconds` y el estado del
  projector. La métrica es lag temporal, no cantidad de eventos.
- `IxmatiCacheSyncErrors`: el commit SQLite ya ocurrió; ejecutar reconciler o
  revisar cache-server. No borrar la evidencia de `_outbox`.

Las consultas Prometheus completas están en `k8s/alerts.yaml`.
