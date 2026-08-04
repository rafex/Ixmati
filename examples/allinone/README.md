# Ixmati All-in-One — Manual de evaluación

> **Objetivo**: levantar una instancia completa de Ixmati (Mosquitto + API + Writer + Projector)
> en un solo contenedor, explorarla con scripts interactivos, y encontrar bugs.

## Arquitectura

```
┌─────────────────────────────────────────────────┐
│              ixmati-allinone:local               │
│                                                   │
│  ┌──────────┐  ┌──────────┐  ┌───────────────┐  │
│  │ Mosquitto│  │   API    │  │    Writer      │  │
│  │  :1883   │  │  :30000  │  │  read/process  │  │
│  │  broker  │◄─┤ REST/gRPC├─►│  sqlite + WAL  │  │
│  └────┬─────┘  └──────────┘  └───────┬───────┘  │
│       │                               │          │
│       │  ixmati/cmd/...    (cmd)     │          │
│       │  ixmati/evt/...    (evt)     │          │
│       │                               │          │
│  ┌────┴─────┐                  ┌──────┴───────┐  │
│  │Events    │                  │   Projector  │  │
│  │out/in    │                  │   (events)   │  │
│  └──────────┘                  └──────────────┘  │
│                      supervisord                   │
└─────────────────────────────────────────────────┘
```

- **API** escucha en `0.0.0.0:30000` (expuesto al host como 30080)
- **Mosquitto** escucha en `:1883` interno, expuesto al host como 30200
- **Writer** consume comandos de MQTT, aplica batches con `BEGIN IMMEDIATE` en SQLite
- **Projector** consume eventos de `ixmati/evt/#` para construir read models (en desarrollo)

## Prerrequisitos

- `podman` con acceso al host remoto (ya configurado en el túnel SSH)
- `curl` + `jq` (para scripts bash)
- Opcional: `mosquitto_sub` (para monitor de eventos), `python3` + `paho-mqtt` (para explorador interactivo)

## Setup rápido

```bash
cd examples/allinone
./run.sh
```

Esto construye la imagen (si no existe), levanta el contenedor en `192.168.3.175`,
y espera a que el health check esté OK.

## Variables de entorno

| Variable | Default | Descripción |
|---|---|---|
| `IXMATI_HOST` | `localhost` | Host donde está la API (localhost si hay port-forward, IP si acceso directo) |
| `IXMATI_API_PORT` | `30080` | Puerto de la API |
| `IXMATI_MQTT_PORT` | `30200` | Puerto de Mosquitto |
| `IXMATI_API_KEY` | `smoke-test-key` | API key para autenticación |
| `STORE_NAME` | `default` | Nombre del store (pasado al contenedor) |

## Endpoints API

| Método | Ruta | Auth | Descripción |
|---|---|---|---|
| `GET` | `/health` | No | Health check: api, sqlite, mosquitto |
| `POST` | `/write` | `Bearer <key>` | Enviar un comando de escritura |
| `GET` | `/writes/{store}/{key}` | `Bearer <key>` | Consultar estado de un write |
| `GET` | `/read?store=X&entity=Y&key=Z` | `Bearer <key>` | Lectura (cache-aside en desarrollo) |
| `GET` | `/metrics` | No | Métricas en formato Prometheus |

## Topics MQTT

| Topic | Dirección | Descripción |
|---|---|---|
| `ixmati/cmd/{store}/{entity}/{key}` | API → Writer | Comandos de escritura |
| `ixmati/evt/{store}/{entity}/{key}` | Writer → Projector | Eventos publicados |

## Estructura de archivos

```
examples/allinone/
├── README.md              ← este manual
├── run.sh                 ← build + run + esperar health
├── stop.sh                ← detener y limpiar
├── e2e-test.sh            ← smoke test automático
├── stress-test.sh         ← escenarios de carga y edge cases
├── subscribe-events.sh    ← monitor de eventos MQTT en tiempo real
├── shell-helpers.sh       ← funciones reutilizables (sourcable)
├── python/
│   ├── explore.py         ← menú interactivo Python
│   └── requirements.txt   ← paho-mqtt, requests
└── scenarios/
    ├── 01-health.sh       ← GET /health → OK
    ├── 02-write-read.sh   ← Write → status APPLIED
    ├── 03-outbox.sh        ← Write → evento MQTT
    ├── 04-idempotency.sh  ← Mismo key → 1 evento
    ├── 05-version-conflict.sh ← v2 → v1 rechazado
    ├── 06-stress.sh       ← 100 concurrentes
    └── 07-crash.sh        ← Kill writer → recuperación
```

## Troubleshooting

**El container no arranca**
```bash
podman logs ixmati-allinone
# Verificar que no haya conflicto de puertos: ss -tlnp | grep -E '30080|30200'
```

**No puedo conectar a la API**
```bash
# Probar desde el host remoto directamente:
ssh 192.168.3.175 curl http://127.0.0.1:30000/health

# Si falla, el problema es el contenedor. Si funciona, es el port-forward.
# Para port-forward:
ssh -L 30080:192.168.3.175:30080 -L 30200:192.168.3.175:30200 rafex@192.168.3.175 -fN
```

**MQTT no recibe eventos**
```bash
podman logs ixmati-allinone 2>&1 | grep -E 'event_publisher|failed to publish'
```
