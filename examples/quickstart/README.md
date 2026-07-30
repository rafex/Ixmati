# Ixmati Quickstart — prueba end-to-end local

Stack completo en un solo `docker compose up`:
Mosquitto → API → Writer → SQLite

## Requisitos

- Docker / Podman con `docker compose` o `podman compose`
- 2 GB de RAM disponible

## Arranque

```bash
cd examples/quickstart
docker compose up -d
```

Esperar ~10 segundos a que los healthchecks estén verdes:

```bash
docker compose ps
# mosquitto: healthy
# writer: running
# api: running
```

## Probar

### Automático (script)

```bash
chmod +x e2e-test.sh
./e2e-test.sh
```

```
=== Ixmati Quickstart E2E Test ===
  [1/5] Health check... PASS
  [2/5] POST /write sin auth... 401 PASS
  [3/5] POST /write ped_1... ACCEPTED PASS
  [4/5] POST /write ped_2... ACCEPTED PASS
  [5/5] GET /writes/pedidos/{key}... APPLIED PASS
```

### Manual

```bash
# Health
curl http://localhost:8080/health | jq

# Escribir un pedido (API key: ix-quickstart-key)
curl -X POST http://localhost:8080/write \
  -H "Authorization: ApiKey ix-quickstart-key" \
  -H "Content-Type: application/json" \
  -d '{
    "op": "upsert",
    "store": "pedidos",
    "entity": "pedido",
    "key": "ped_abc",
    "version": 1,
    "ts": "2026-07-30T00:00:00Z",
    "idempotency_key": "550e8400-e29b-41d4-a716-446655440000",
    "ack_mode": "accepted",
    "payload": {"total": 1500, "estado": "pendiente"}
  }' | jq

# El writer procesa el comando en ~1-3 segundos
sleep 3

# Consultar estado
curl "http://localhost:8080/writes/pedidos/550e8400-e29b-41d4-a716-446655440000" | jq
# {"status":"APPLIED","store":"pedidos","entity":"pedido",...}
```

## Detener

```bash
docker compose down
docker compose down -v  # también borra el volumen de datos
```

## Estructura

```
quickstart/
├── README.md            # este archivo
├── docker-compose.yaml  # stack completo (Mosquitto + API + Writer)
├── mosquitto.conf       # config broker (puerto 1883, ephemeral)
├── e2e-test.sh          # smoke test automatizado
├── seed-data.sql        # inserts de ejemplo para explorar la BD
└── db/                  # volumen montado (BD SQLite creada por el writer)
```
