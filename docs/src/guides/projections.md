### Guía: Proyecciones

1. Declarar en `config/projections.toml`:
```toml
[[projections]]
name = "pedidos_con_usuario"
pattern = "R"
source_stores = ["pedidos", "usuarios"]
target_key = "pedido_id"
```

2. El proyector se suscribe automáticamente a `ixmati/evt/pedidos/#` y `ixmati/evt/usuarios/#`.

3. Cada evento actualiza el read model en `p:pedidos_con_usuario:{pedido_id}`.

4. La API de lectura consulta por proyección:
```
GET /read?projection=pedidos_con_usuario&key=ped_123
```

Pattern R sirve el snapshot construido al recibir los eventos. Un cambio en
una entidad referenciada no busca automáticamente todos los read models que la
usan; si esa relación debe mantenerse fresca, usar Pattern M o reproyectar con
`ixmati-reconciler` antes de servir el nuevo estado.

5. Para migrar el esquema de una proyección, purgar y reproyectar:
```bash
just purge-projection pedidos_con_usuario
cargo run -p ixmati-reconciler -- --projection pedidos_con_usuario
```

Regla de decisión patrón M: solo si fan-out ≤ 100 y ratio lectura/escritura ≥ 100:1.
