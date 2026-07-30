### Guía: Single Store

El caso más simple. Un archivo SQLite, un writer, sin bus de eventos.

1. Crear `config/stores.toml` con un store:
```toml
[[stores]]
name = "pedidos"
path = "/data/pedidos.db"
```

2. Iniciar Mosquitto:
```bash
docker run -d --name mosquitto -p 1883:1883 eclipse-mosquitto:2
```

3. Iniciar el supervisor:
```bash
just env-up
cargo run -p ixmati-supervisor -- --config config/stores.toml
```

4. Enviar comandos vía API REST:
```bash
curl -X POST http://localhost:8080/write \
  -H "Content-Type: application/json" \
  -d '{"op":"upsert","store":"pedidos","entity":"pedido","key":"p1","version":1,"idempotency_key":"...","ack_mode":"accepted","payload":{}}'
```

Con `stores=1`, el sistema no activa bus de eventos ni outbox. La cache-aside funciona con fallback a SQLite. Sin proyecciones declaradas, no se ejecuta ningún proyector.
