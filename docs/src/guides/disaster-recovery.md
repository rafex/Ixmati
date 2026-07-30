### Guía: Disaster Recovery

#### Configurar Litestream

```toml
# por store en stores.toml
litestream.enabled = true
litestream.destinations = ["s3://ixmati-backups/pedidos", "/mnt/backup-vps/pedidos"]
```

#### Restaurar un store

```bash
litestream restore -o /data/pedidos_restored.db s3://ixmati-backups/pedidos
sqlite3 /data/pedidos_restored.db "PRAGMA integrity_check;"
```

#### Restaurar a un punto en el tiempo

```bash
litestream restore -o /data/pedidos_restored.db --timestamp "2026-07-29T10:30:00Z" s3://ixmati-backups/pedidos
```

#### Post-restauración

```bash
just doctor
just env-up
cargo run -p ixmati-supervisor -- --config config/stores.toml
```

Los read models se reconstruyen con el reconciler si es necesario:
```bash
cargo run -p ixmati-reconciler -- --config config/stores.toml
```
