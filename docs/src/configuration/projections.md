## Configuración de Proyecciones

Las proyecciones se declaran en `config/projections.toml`:

### Patrón R (referencia + lookup, por defecto)

```toml
[[projections]]
name = "pedidos_con_usuario"
pattern = "R"
source_stores = ["pedidos", "usuarios"]
target_key = "pedido_id"
ttl_seconds = 300
```

El proyector guarda `{usr_id: 9}` en el read model. La consulta completa hace 2 lecturas a FlashDB (`ped:123` + `usr:9`) y las combina en Rust. Sin fan-out en escritura.

En la implementación actual, Pattern R materializa el snapshot disponible al
procesar el evento. Si cambia posteriormente una entidad referenciada (por
ejemplo, el usuario de un pedido), la vista existente no se actualiza de forma
automática para todos los pedidos que la referencian. Para datos relacionados
que cambian y deben reflejarse inmediatamente, usar Pattern M o ejecutar una
reproyección/fan-out controlado con el reconciler. Esta limitación queda fuera
del perfil de lectura productiva mutable hasta implementar el fan-out.

### Patrón M (materializado)

```toml
[[projections]]
name = "pedidos_desnormalizados"
pattern = "M"
source_stores = ["pedidos", "usuarios"]
target_key = "pedido_id"
ttl_seconds = 300

[[projections.copy_fields]]
source_store = "usuarios"
source_entity = "usuario"
fields = ["nombre", "email"]
```

El proyector copia `nombre` y `email` del usuario al documento del pedido. Una sola lectura, pero renombrar un usuario obliga a reescribir todos sus pedidos. **Restricción**: solo se permite patrón M si `fan_out ≤ 100` y `ratio_lectura_escritura ≥ 100:1` (ver DEC-0016).

### Campos comunes

| Campo | Tipo | Obligatorio | Descripción |
|---|---|---|---|
| `name` | string | Sí | Nombre único de la proyección |
| `pattern` | string | Sí | `"R"` o `"M"` |
| `source_stores` | []string | Sí | Stores cuyos eventos alimentan esta proyección |
| `target_key` | string | Sí | Campo del evento fuente usado como clave en FlashDB |
| `ttl_seconds` | int | No | TTL en segundos (default: 300) |
| `copy_fields` | tabla | Solo M | Campos a copiar desde otro store |
