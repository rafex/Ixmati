## Configuración de Stores

Los stores se declaran en `config/stores.toml`:

```toml
[[stores]]
name = "pedidos"
path = "/data/pedidos.db"
label = "Pedidos (dominio)"

[[stores]]
name = "usuarios"
path = "/data/usuarios.db"
label = "Usuarios (dominio)"
```

### Campos

| Campo | Tipo | Obligatorio | Descripción |
|---|---|---|---|
| `name` | string | Sí | Identificador único. `snake_case`, sin `/`. Inmutable tras creación. |
| `path` | string | Sí | Ruta del archivo SQLite. |
| `label` | string | No | Etiqueta descriptiva (ej. "dominio Pedidos"). El motor no la usa. |

### Topología

- `stores=1`: el supervisor lanza 1 proceso writer. Sin bus de eventos ni outbox activos.
- `stores=N`: el supervisor lanza 1 writer task por store (single-process) o 1 pod por store (K8s). Se activa el bus de eventos, outbox y proyecciones.
