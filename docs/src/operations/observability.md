### Observabilidad

#### Métricas clave

| Métrica | Tipo | Descripción |
|---|---|---|
| `ixmati_cmds_total{store}` | Counter | Comandos recibidos |
| `ixmati_cmds_latency_ms{store}` | Histogram | Latencia de commit |
| `ixmati_events_published{store}` | Counter | Eventos publicados |
| `ixmati_outbox_pending{store}` | Gauge | Eventos en _outbox sin publicar |
| `ixmati_cache_hits` / `_misses` | Counter | Hits/misses en FlashDB |
| `ixmati_projection_lag_ms{projection}` | Histogram | Lag de proyección |
| `ixmati_writer_alive{store}` | Gauge | 1 si el writer responde |

#### Logs

Formato JSON estructurado (`tracing-subscriber`). Campos obligatorios: `timestamp`, `level`, `store`, `idempotency_key`/`event_id`, `entity`, `key`, `latency_ms`.

#### Health checks

- `GET /health`: estado agregado de todos los componentes.
- `GET /health/{store}`: estado de un store específico.
- Heartbeat MQTT: cada writer publica en `ixmati/health/{store}` cada 5s.

#### Alertas recomendadas

- Writer caído: `ixmati_writer_alive == 0` por > 30s.
- Outbox estancado: `ixmati_outbox_pending > 1000` por > 5 min.
- Lag de proyección: `ixmati_projection_lag_ms p99 > 1000` por > 2 min.
- Litestream detenido: sin generaciones nuevas por > 5 min.
- Cola creciendo: `$SYS/broker/messages/stored > 10000`.
