use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use ixmati_cache::CacheBackend;

pub struct CacheServer {
    cache: Arc<dyn CacheBackend>,
    path: String,
}

impl CacheServer {
    pub fn new(cache: Arc<dyn CacheBackend>, path: &str) -> Self {
        Self {
            cache,
            path: path.to_string(),
        }
    }

    pub async fn run(&self) -> std::io::Result<()> {
        if Path::new(&self.path).exists() {
            let _ = std::fs::remove_file(&self.path);
        }
        if let Some(parent) = Path::new(&self.path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let listener = UnixListener::bind(&self.path)?;
        tracing::info!(path = %self.path, "CacheServer listening");

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let cache = Arc::clone(&self.cache);
                    tokio::spawn(async move {
                        handle_client(stream, cache).await;
                    });
                }
                Err(e) => {
                    tracing::error!(error = %e, "CacheServer accept error");
                }
            }
        }
    }
}

async fn handle_client(stream: UnixStream, cache: Arc<dyn CacheBackend>) {
    let (reader, writer) = stream.into_split();
    let reader = BufReader::new(reader);
    let writer = Arc::new(tokio::sync::Mutex::new(writer));
    let mut lines = reader.lines();

    loop {
        let line = match tokio::time::timeout(Duration::from_secs(30), lines.next_line()).await
        {
            Ok(Ok(Some(line))) => line,
            _ => break,
        };

        let trimmed = line.trim();
        if let Some(key) = trimmed.strip_prefix("GET ") {
            let key = key.trim();
            match cache.get("cache", key, "") {
                Some(payload) => {
                    let header = format!("HIT {}\n", payload.len());
                    let mut writer = writer.lock().await;
                    let _ = writer.write_all(header.as_bytes()).await;
                    let _ = writer.write_all(&payload).await;
                    let _ = writer.write_all(b"\n").await;
                }
                None => {
                    let mut writer = writer.lock().await;
                    let _ = writer.write_all(b"MISS\n").await;
                }
            }
        }
    }
}
