### Runbook Operativo

#### Restaurar un store desde Litestream

1. Detener el writer del store afectado.
2. `litestream restore -o /data/<store>_restored.db s3://ixmati-backups/<store>`
3. Verificar integridad: `sqlite3 /data/<store>_restored.db "PRAGMA integrity_check;"`
4. Reemplazar el archivo y reiniciar el writer.
5. Si es necesario, reproyectar: `cargo run -p ixmati-reconciler -- --projection <name>`

#### Failover manual del writer

1. Detectar caída (health check o alerta).
2. Mosquitto retiene los comandos en disco (persistence). Sin pérdida.
3. Reiniciar el writer. El proceso consume los comandos pendientes.
4. El publicador vacía `_outbox` (eventos pendientes).

#### Reconstruir read models

```bash
# reproyección total (todos los read models desde cero)
cargo run -p ixmati-reconciler

# reproyección selectiva
cargo run -p ixmati-reconciler -- --projection pedidos_con_usuario

# dry-run (validar sin escribir)
cargo run -p ixmati-reconciler -- --dry-run
```

#### Troubleshooting

- **Cola creciendo**: `mosquitto_sub -t '$SYS/broker/messages/stored'` → verificar que el writer está corriendo.
- **Outbox estancado**: `SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL` → verificar publicador.
- **Lag de proyección**: `uv run python helpers/python/lag_probe.py /data/<store>.db --projection` → posible cuello de botella en proyector.
- **SQLITE_BUSY**: no debería ocurrir. Si aparece, verificar que solo un proceso abre el store en write.
- **Cache stale**: purgar `just cache-purge --prefix "c:<store>:*"` y dejar que se repueble con el tráfico.
