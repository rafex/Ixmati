### Guía: Multi Store

N stores independientes, cada uno con su writer, WAL y Litestream.

1. Declarar stores:
```toml
[[stores]]
name = "pedidos"
path = "/data/pedidos.db"
label = "Pedidos"

[[stores]]
name = "usuarios"
path = "/data/usuarios.db"
label = "Usuarios"
```

2. El supervisor lanza 1 writer task por store (single-process) o 1 pod por store (K8s).

3. Los comandos incluyen `store` obligatorio:
```json
{"store": "pedidos", "entity": "pedido", ...}
{"store": "usuarios", "entity": "usuario", ...}
```

4. Cada store publica sus eventos en `ixmati/evt/<store>/...`. Los proyectores se suscriben al prefijo correspondiente.

5. Litestream sidecar por store. Backups independientes, frecuencias independientes.

6. La caída de un store no afecta a los demás. Los read models que referencian datos de ese store siguen sirviendo desde FlashDB.
