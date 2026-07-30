### Guía: Kubernetes

#### Componentes

- **StatefulSet** por store: identidad estable + PVC.
- **PVC** por store: almacenamiento persistente del archivo SQLite.
- **Litestream sidecar**: replica WAL a S3.
- **ConfigMap**: `stores.toml`, `projections.toml`.

#### Ejemplo de manifiesto

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: ixmati-pedidos
spec:
  serviceName: ixmati-pedidos
  replicas: 1
  template:
    spec:
      containers:
      - name: ixmati
        image: ixmati:latest
        args: ["--store", "pedidos"]
        volumeMounts:
        - name: data
          mountPath: /data
      - name: litestream
        image: litestream/litestream:latest
        args: ["replicate", "/data/pedidos.db", "s3://ixmati-backups/pedidos"]
        volumeMounts:
        - name: data
          mountPath: /data
  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 10Gi
```

Un StatefulSet por store. Litestream como sidecar compartiendo el mismo PVC.
