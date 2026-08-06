"""Smoke test: e-commerce e2e — proyecciones pedidos_con_usuario y usuarios_materializados."""

import time
import pytest
from helpers.python.mqtt_harness import (
    ApiConfig,
    MqttConfig,
    create_client,
    http_write,
    http_read,
    http_read_projection,
    make_write_payload,
    wait_for_message,
)


@pytest.mark.smoke
@pytest.mark.e2e
class TestEcommerceProjections:
    """Flujo e2e: escribir usuario + pedido, verificar proyeccion y cache multi-proceso."""

    def test_materialized_view_copies_fields(
        self, compose_up_multi, api_config_multi, mqtt_config_multi
    ):
        """Pattern M: al escribir usuario, la proyeccion copia nombre y email."""
        api = ApiConfig(**api_config_multi)
        mqtt_cfg = MqttConfig(**mqtt_config_multi, client_id="e2e-mat")
        client = create_client(mqtt_cfg)

        resp = http_write(
            api,
            {
                "op": "upsert",
                "store": "usuarios",
                "entity": "usuario",
                "key": "usr_100",
                "version": 1,
                "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "idempotency_key": "e2e-mat-usr-100",
                "ack_mode": "accepted",
                "payload": {
                    "usuario_id": "usr_100",
                    "nombre": "Ana",
                    "email": "ana@example.com",
                },
            },
        )
        assert resp.status == "ACCEPTED"

        time.sleep(2)

        proj = http_read_projection(api, "usuarios_materializados", "usr_100")
        assert proj is not None, "proyeccion no encontrada"
        assert proj.get("found") is True, f"esperaba found=true, obtuve {proj}"
        payload = proj.get("payload", {})
        assert payload.get("nombre") == "Ana", f"nombre={payload.get('nombre')}"
        assert payload.get("email") == "ana@example.com", f"email={payload.get('email')}"

        client.disconnect()

    def test_write_order_triggers_projection(
        self, compose_up_multi, api_config_multi, mqtt_config_multi
    ):
        """Pattern R: al escribir pedido con usuario existente, la proyeccion une ambos."""
        api = ApiConfig(**api_config_multi)
        mqtt_cfg = MqttConfig(**mqtt_config_multi, client_id="e2e-r")
        client = create_client(mqtt_cfg)

        http_write(
            api,
            {
                "op": "upsert",
                "store": "usuarios",
                "entity": "usuario",
                "key": "usr_200",
                "version": 1,
                "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "idempotency_key": "e2e-r-usr-200",
                "ack_mode": "accepted",
                "payload": {
                    "usuario_id": "usr_200",
                    "nombre": "Carlos",
                    "email": "carlos@example.com",
                },
            },
        )

        time.sleep(1)

        resp = http_write(
            api,
            {
                "op": "upsert",
                "store": "pedidos",
                "entity": "pedido",
                "key": "ped_200",
                "version": 1,
                "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "idempotency_key": "e2e-r-ped-200",
                "ack_mode": "accepted",
                "payload": {
                    "pedido_id": "ped_200",
                    "usuario_id": "usr_200",
                    "total": 150.0,
                    "estado": "pendiente",
                },
            },
        )
        assert resp.status == "ACCEPTED"

        time.sleep(3)

        proj = http_read_projection(api, "pedidos_con_usuario", "ped_200")
        assert proj is not None, f"proyeccion no encontrada: {proj}"
        assert proj.get("found") is True, f"esperaba found=true, obtuve {proj}"
        payload = proj.get("payload", {})
        assert "pedidos" in payload, f"falta store pedidos en {payload}"
        assert payload["pedidos"].get("total") == 150.0
        assert "usuarios" in payload, f"falta store usuarios en {payload}"
        assert payload["usuarios"].get("nombre") == "Carlos"

        client.disconnect()

    def test_idempotent_projection(
        self, compose_up_multi, api_config_multi, mqtt_config_multi
    ):
        """Mismo pedido escrito 2 veces no duplica ni corrompe la proyeccion."""
        api = ApiConfig(**api_config_multi)
        mqtt_cfg = MqttConfig(**mqtt_config_multi, client_id="e2e-idem")
        client = create_client(mqtt_cfg)

        http_write(
            api,
            {
                "op": "upsert",
                "store": "usuarios",
                "entity": "usuario",
                "key": "usr_300",
                "version": 1,
                "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "idempotency_key": "e2e-idem-usr-300",
                "ack_mode": "accepted",
                "payload": {
                    "usuario_id": "usr_300",
                    "nombre": "Diana",
                    "email": "diana@example.com",
                },
            },
        )

        time.sleep(1)

        idem_key = "e2e-idem-ped-300"
        payload_template = {
            "op": "upsert",
            "store": "pedidos",
            "entity": "pedido",
            "key": "ped_300",
            "version": 1,
            "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "idempotency_key": idem_key,
            "ack_mode": "accepted",
            "payload": {
                "pedido_id": "ped_300",
                "usuario_id": "usr_300",
                "total": 200.0,
                "estado": "pendiente",
            },
        }

        r1 = http_write(api, dict(payload_template))
        assert r1.status == "ACCEPTED"
        r2 = http_write(api, dict(payload_template))
        assert r2.status == "ACCEPTED"

        time.sleep(3)

        proj = http_read_projection(api, "pedidos_con_usuario", "ped_300")
        assert proj is not None
        assert proj.get("found") is True
        payload = proj.get("payload", {})
        assert payload.get("pedidos", {}).get("total") == 200.0
        assert payload.get("usuarios", {}).get("nombre") == "Diana"

        client.disconnect()

    def test_concurrent_writes_consistent_cache(
        self, compose_up_multi, api_config_multi, mqtt_config_multi
    ):
        """Escrituras concurrentes a multiples stores no corrompen el cache-server."""
        import concurrent.futures

        api = ApiConfig(**api_config_multi)
        mqtt_cfg = MqttConfig(**mqtt_config_multi, client_id="e2e-conc")
        client = create_client(mqtt_cfg)

        def write_user(i):
            return http_write(
                api,
                {
                    "op": "upsert",
                    "store": "usuarios",
                    "entity": "usuario",
                    "key": f"usr_conc_{i}",
                    "version": 1,
                    "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                    "idempotency_key": f"e2e-conc-usr-{i}",
                    "ack_mode": "accepted",
                    "payload": {
                        "usuario_id": f"usr_conc_{i}",
                        "nombre": f"User_{i}",
                        "email": f"user{i}@example.com",
                    },
                },
            )

        def write_order(i):
            return http_write(
                api,
                {
                    "op": "upsert",
                    "store": "pedidos",
                    "entity": "pedido",
                    "key": f"ped_conc_{i}",
                    "version": 1,
                    "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                    "idempotency_key": f"e2e-conc-ped-{i}",
                    "ack_mode": "accepted",
                    "payload": {
                        "pedido_id": f"ped_conc_{i}",
                        "usuario_id": f"usr_conc_{i}",
                        "total": float(100 + i),
                        "estado": "pendiente",
                    },
                },
            )

        with concurrent.futures.ThreadPoolExecutor(max_workers=10) as executor:
            user_futures = [executor.submit(write_user, i) for i in range(5)]
            order_futures = [executor.submit(write_order, i) for i in range(5)]

            for f in user_futures + order_futures:
                result = f.result(timeout=15)
                assert result.status == "ACCEPTED", f"write failed: {result}"

        time.sleep(5)

        for i in range(5):
            proj = http_read_projection(api, "pedidos_con_usuario", f"ped_conc_{i}")
            assert proj is not None, f"proyeccion ped_conc_{i} no encontrada"
            assert proj.get("found") is True, f"ped_conc_{i}: {proj}"
            payload = proj.get("payload", {})
            assert payload.get("pedidos", {}).get("total") == float(100 + i)
            assert payload.get("usuarios", {}).get("nombre") == f"User_{i}"

        client.disconnect()
