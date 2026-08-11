CREATE TABLE IF NOT EXISTS payload_usuarios (
    entity TEXT NOT NULL,
    key TEXT NOT NULL,
    version BIGINT NOT NULL,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (entity, key)
);

CREATE TABLE IF NOT EXISTS payload_pedidos (
    entity TEXT NOT NULL,
    key TEXT NOT NULL,
    version BIGINT NOT NULL,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (entity, key)
);

CREATE TABLE IF NOT EXISTS _idempotency (
    idempotency_key TEXT PRIMARY KEY,
    store TEXT NOT NULL,
    entity TEXT NOT NULL,
    key TEXT NOT NULL,
    version BIGINT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS _outbox (
    id BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    store TEXT NOT NULL,
    entity TEXT NOT NULL,
    key TEXT NOT NULL,
    version BIGINT NOT NULL,
    payload JSONB NOT NULL,
    published_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_idempotency_entity_key
    ON _idempotency(store, entity, key, version);
CREATE INDEX IF NOT EXISTS idx_outbox_published
    ON _outbox(published_at);
CREATE INDEX IF NOT EXISTS idx_orders_user
    ON payload_pedidos((payload->>'usuario_id'));
