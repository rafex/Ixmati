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

    pub async fn get(&self, store: &str, entity: &str, key: &str) -> Option<Vec<u8>> {
        let cache_key = format!("c:{}:{}:{}", store, entity, key);
        let req = format!("GET {}\n", cache_key);

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
}
