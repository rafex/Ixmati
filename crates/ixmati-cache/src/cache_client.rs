use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

pub struct CacheClient {
    path: String,
    stream: Arc<Mutex<Option<UnixStream>>>,
}

impl CacheClient {
    pub async fn connect(path: &str) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path).await?;
        tracing::info!(path = %path, "CacheClient connected");
        Ok(Self {
            path: path.to_owned(),
            stream: Arc::new(Mutex::new(Some(stream))),
        })
    }

    pub fn cache_key(store: &str, entity: &str, key: &str) -> String {
        format!("c:{store}:{entity}:{key}")
    }

    pub fn projection_key(name: &str, key: &str) -> String {
        format!("p:{name}:{key}")
    }

    pub async fn get(&self, store: &str, entity: &str, key: &str) -> Option<Vec<u8>> {
        self.get_raw(&Self::cache_key(store, entity, key)).await
    }

    pub async fn get_raw(&self, key: &str) -> Option<Vec<u8>> {
        let req = format!("GET {key}\n");
        self.send_command_with_response(&req).await
    }

    pub async fn get_projection(&self, name: &str, key: &str) -> Option<Vec<u8>> {
        self.get_raw(&Self::projection_key(name, key)).await
    }

    pub async fn set(&self, store: &str, entity: &str, key: &str, value: &[u8]) {
        self.set_raw(&Self::cache_key(store, entity, key), value)
            .await;
    }

    pub async fn set_raw(&self, key: &str, value: &[u8]) {
        let req = format!("SET {key} {}\n", value.len());
        for _ in 0..2 {
            let mut guard = self.stream.lock().await;
            if guard.is_none() {
                *guard = UnixStream::connect(&self.path).await.ok();
                if guard.is_some() {
                    tracing::info!(path = %self.path, "CacheClient reconnected");
                }
            }
            let Some(stream) = guard.as_mut() else {
                continue;
            };
            if Self::write_set(stream, &req, value).await {
                return;
            }
            *guard = None;
        }
        tracing::warn!(path = %self.path, "CacheClient: SET failed after reconnect");
    }

    pub async fn set_projection(&self, name: &str, key: &str, value: &[u8]) {
        self.set_raw(&Self::projection_key(name, key), value).await;
    }

    pub async fn del(&self, store: &str, entity: &str, key: &str) {
        self.del_raw(&Self::cache_key(store, entity, key)).await;
    }

    pub async fn del_raw(&self, key: &str) {
        let req = format!("DEL {key}\n");
        self.send_simple(&req).await;
    }

    pub async fn delete_by_prefix(&self, prefix: &str) {
        let req = format!("DEL_PREFIX {prefix}\n");
        self.send_simple(&req).await;
    }

    pub async fn flush(&self) {
        self.send_simple("FLUSH\n").await;
    }

    async fn send_command_with_response(&self, req: &str) -> Option<Vec<u8>> {
        for _ in 0..2 {
            let mut guard = self.stream.lock().await;
            if guard.is_none() {
                *guard = UnixStream::connect(&self.path).await.ok();
                if guard.is_some() {
                    tracing::info!(path = %self.path, "CacheClient reconnected");
                }
            }
            let Some(stream) = guard.as_mut() else {
                continue;
            };
            if let Some(payload) = Self::read_get(stream, req).await {
                return Some(payload);
            }
            *guard = None;
        }
        None
    }

    async fn send_simple(&self, req: &str) {
        for _ in 0..2 {
            let mut guard = self.stream.lock().await;
            if guard.is_none() {
                *guard = UnixStream::connect(&self.path).await.ok();
                if guard.is_some() {
                    tracing::info!(path = %self.path, "CacheClient reconnected");
                }
            }
            let Some(stream) = guard.as_mut() else {
                continue;
            };
            if Self::read_simple_response(stream, req).await {
                return;
            }
            *guard = None;
        }
    }

    async fn write_set(stream: &mut UnixStream, req: &str, value: &[u8]) -> bool {
        if stream.write_all(req.as_bytes()).await.is_err()
            || stream.write_all(value).await.is_err()
            || stream.write_all(b"\n").await.is_err()
        {
            return false;
        }
        Self::read_simple_response(stream, "").await
    }

    async fn read_get(stream: &mut UnixStream, req: &str) -> Option<Vec<u8>> {
        if stream.write_all(req.as_bytes()).await.is_err() {
            return None;
        }
        let mut reader = BufReader::new(stream);
        let mut header = String::new();
        let read_result = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                header.clear();
                let n = reader.read_line(&mut header).await?;
                if n == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "CacheClient: connection closed",
                    ));
                }
                if !header.trim().is_empty() {
                    break;
                }
            }
            Ok::<_, std::io::Error>(())
        })
        .await;
        if !matches!(read_result, Ok(Ok(()))) {
            return None;
        }
        let len = header.trim().strip_prefix("HIT ")?.trim().parse().ok()?;
        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).await.ok()?;
        let mut trail = [0u8; 1];
        reader.read_exact(&mut trail).await.ok()?;
        Some(payload)
    }

    async fn read_simple_response(stream: &mut UnixStream, req: &str) -> bool {
        if !req.is_empty() && stream.write_all(req.as_bytes()).await.is_err() {
            return false;
        }
        let reader = BufReader::new(stream);
        let mut reader = reader;
        let mut line = String::new();
        match tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line)).await {
            Ok(Ok(_)) => {
                let resp = line.trim();
                if resp != "OK" {
                    tracing::warn!(response = %resp, "CacheClient: unexpected response");
                    false
                } else {
                    true
                }
            }
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "CacheClient: read response failed");
                false
            }
            Err(_) => {
                tracing::warn!("CacheClient: read response timed out");
                false
            }
        }
    }
}
