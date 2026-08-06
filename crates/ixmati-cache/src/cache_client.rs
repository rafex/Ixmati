use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

pub struct CacheClient {
    stream: Arc<Mutex<UnixStream>>,
}

impl CacheClient {
    pub async fn connect(path: &str) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path).await?;
        tracing::info!(path = %path, "CacheClient connected");
        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
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
        self.set_raw(&Self::cache_key(store, entity, key), value).await;
    }

    pub async fn set_raw(&self, key: &str, value: &[u8]) {
        let req = format!("SET {key} {}\n", value.len());
        let mut guard = self.stream.lock().await;
        if guard.write_all(req.as_bytes()).await.is_err() {
            return;
        }
        if guard.write_all(value).await.is_err() {
            return;
        }
        let _ = guard.write_all(b"\n").await;
        self.read_simple_response(&mut *guard).await;
    }

    pub async fn set_projection(&self, name: &str, key: &str, value: &[u8]) {
        self.set_raw(&Self::projection_key(name, key), value).await;
    }

    pub async fn del(&self, store: &str, entity: &str, key: &str) {
        self.del_raw(&Self::cache_key(store, entity, key)).await;
    }

    pub async fn del_raw(&self, key: &str) {
        let req = format!("DEL {key}\n");
        let mut guard = self.stream.lock().await;
        if guard.write_all(req.as_bytes()).await.is_err() {
            return;
        }
        self.read_simple_response(&mut *guard).await;
    }

    pub async fn delete_by_prefix(&self, prefix: &str) {
        let req = format!("DEL_PREFIX {prefix}\n");
        let mut guard = self.stream.lock().await;
        if guard.write_all(req.as_bytes()).await.is_err() {
            return;
        }
        self.read_simple_response(&mut *guard).await;
    }

    pub async fn flush(&self) {
        let mut guard = self.stream.lock().await;
        if guard.write_all(b"FLUSH\n").await.is_err() {
            return;
        }
        self.read_simple_response(&mut *guard).await;
    }

    async fn send_command_with_response(&self, req: &str) -> Option<Vec<u8>> {
        let mut guard = self.stream.lock().await;
        if guard.write_all(req.as_bytes()).await.is_err() {
            return None;
        }

        let reader = BufReader::new(&mut *guard);
        let mut reader = reader;

        let mut header = String::new();
        let read_result = tokio::time::timeout(Duration::from_millis(20), async {
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

        match read_result {
            Ok(Ok(())) => {}
            _ => return None,
        }

        if let Some(len_str) = header.trim().strip_prefix("HIT ") {
            if let Ok(len) = len_str.trim().parse::<usize>() {
                let mut payload = vec![0u8; len];
                if reader.read_exact(&mut payload).await.is_ok() {
                    let mut trail = vec![0u8; 1];
                    let _ = reader.read_exact(&mut trail).await;
                    return Some(payload);
                }
            }
        }

        None
    }

    async fn read_simple_response(&self, stream: &mut UnixStream) {
        let reader = BufReader::new(stream);
        let mut reader = reader;
        let mut line = String::new();
        let _ = tokio::time::timeout(Duration::from_millis(20), reader.read_line(&mut line)).await;
    }
}
