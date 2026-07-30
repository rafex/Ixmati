# Ixmati — Runbook de Producción

## Restore de un store desde Litestream

1. Detener el writer del store afectado:
   ```bash
   kubectl scale deployment ixmati-writer-${store} --replicas=0
   ```
2. Restaurar desde S3:
   ```bash
   litestream restore -config config/litestream.yml -o /data/ixmati/${store}.db /data/ixmati/${store}.db
   ```
3. Verificar integridad:
   ```bash
   sqlite3 /data/ixmati/${store}.db "SELECT COUNT(*) FROM _idempotency"
   ```
4. Rearrancar writer:
   ```bash
   kubectl scale deployment ixmati-writer-${store} --replicas=1
   ```
5. Validar health:
   ```bash
   curl http://ixmati-api:8080/health
   ```

## Failover manual del writer de un store

1. Verificar estado actual:
   ```bash
   curl http://ixmati-api:8080/health | jq .components
   ```
2. Si el writer está en `UNAVAILABLE`, mover tráfico al pod de respaldo:
   ```bash
   kubectl scale deployment ixmati-writer-${store} --replicas=2
   ```
3. Esperar que el health reporte `OK` para ese store
4. Bajar el pod defectuoso:
   ```bash
   kubectl scale deployment ixmati-writer-${store} --replicas=1
   ```

## Reprojección completa con reconciler

1. Ejecutar reconciler en modo dry-run para estimar:
   ```bash
   ixmati-reconciler --dry-run
   ```
2. Reprojectar todo:
   ```bash
   ixmati-reconciler --cache-endpoint http://ixmati-cache:8081
   ```
3. Reprojectar una proyección específica:
   ```bash
   ixmati-reconciler --projection pedidos_con_usuario
   ```

## Diagnóstico de lag

- **Lag de cola**: consultar outbox no publicado
  ```bash
  sqlite3 /data/ixmati/${store}.db "SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL"
  ```
  Si crece monótonamente → el publicador de eventos está caído.

- **Lag de proyección**: comparar `MAX(version)` en _idempotency vs cache
  ```bash
  sqlite3 /data/ixmati/${store}.db "SELECT MAX(version) FROM _idempotency"
  ```
  Si el número en SQLite es mayor que en FlashDB → el projector está atrasado.

- **Store caliente**: medir escrituras por minuto
  ```bash
  sqlite3 /data/ixmati/${store}.db "SELECT COUNT(*) FROM _idempotency WHERE applied_at > datetime('now', '-1 minute')"
  ```

## Troubleshooting común

| Síntoma | Causa probable | Acción |
|---|---|---|
| `POST /write` devuelve 503 | Mosquitto caído | `systemctl restart mosquitto` |
| `GET /writes/{store}/{key}` 404 siempre | SQLite inaccesible | Verificar PVC y permisos |
| `GET /health` reporta `sqlite: UNAVAILABLE` | Litestream en restore | Esperar a que termine |
| Comandos aceptados pero nunca APPLIED | Writer caído | Verificar logs del writer |
| Proyecciones desactualizadas | Projector caído | Reiniciar projector |
| Outbox creciendo sin publicar | EventPublisher error | Verificar conectividad MQTT |
