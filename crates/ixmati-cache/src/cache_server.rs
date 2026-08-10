use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::CacheBackend;

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
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut header = String::new();

    loop {
        header.clear();
        match reader.read_line(&mut header).await {
            Ok(0) => {
                tracing::debug!("handle_client: EOF, connection closed");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::debug!(error = %e, "handle_client read error, closing connection");
                break;
            }
        }

        let trimmed = header.trim();
        if trimmed.is_empty() {
            continue;
        }

        if process_command(Arc::clone(&cache), trimmed, &mut reader, &mut writer)
            .await
            .is_err()
        {
            break;
        }
    }
}

/// `CacheBackend` es un trait síncrono (bloquea en el `Mutex<Database>` de
/// Redb durante toda la transacción). Antes se llamaba directamente dentro
/// del handler async, bloqueando el hilo del executor de tokio que lo
/// atendía mientras sostenía el lock — bajo alta concurrencia eso puede
/// saturar el pool de hilos, no solo el propio Redb (ver DEC-0043). Cada
/// operación se despacha ahora a `spawn_blocking`, que corre en el pool de
/// hilos bloqueantes dedicado de tokio, dejando libres los hilos del
/// executor async para aceptar y despachar el resto de las conexiones.
async fn process_command(
    cache: Arc<dyn CacheBackend>,
    line: &str,
    reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
) -> std::io::Result<()> {
    if let Some(key) = line.strip_prefix("GET ") {
        let key = key.trim().to_string();
        let payload = tokio::task::spawn_blocking(move || cache.get("cache", &key, ""))
            .await
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "GET spawn_blocking panicked");
                None
            });
        match payload {
            Some(payload) => {
                let header = format!("HIT {}\n", payload.len());
                writer.write_all(header.as_bytes()).await?;
                writer.write_all(&payload).await?;
                writer.write_all(b"\n").await?;
            }
            None => {
                writer.write_all(b"MISS\n").await?;
            }
        }
    } else if let Some(rest) = line.strip_prefix("SET ") {
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() < 2 {
            writer.write_all(b"ERR invalid SET\n").await?;
            return Ok(());
        }
        let key = parts[0].to_string();
        let len: usize = match parts[1].parse() {
            Ok(l) => l,
            Err(_) => {
                writer.write_all(b"ERR invalid length\n").await?;
                return Ok(());
            }
        };

        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).await?;
        let mut newline = [0u8; 1];
        let _ = reader.read_exact(&mut newline).await;

        let _ = tokio::task::spawn_blocking(move || cache.set("cache", &key, "", &payload)).await;
        writer.write_all(b"OK\n").await?;
    } else if let Some(key) = line.strip_prefix("DEL ") {
        let key = key.trim().to_string();
        let _ = tokio::task::spawn_blocking(move || cache.del("cache", &key, "")).await;
        writer.write_all(b"OK\n").await?;
    } else if let Some(prefix) = line.strip_prefix("DEL_PREFIX ") {
        let prefix = prefix.trim().to_string();
        let _ =
            tokio::task::spawn_blocking(move || cache.delete_by_prefix("cache", &prefix)).await;
        writer.write_all(b"OK\n").await?;
    } else if line.trim() == "FLUSH" {
        let _ = tokio::task::spawn_blocking(move || cache.flush("cache")).await;
        writer.write_all(b"OK\n").await?;
    } else {
        writer.write_all(b"ERR unknown command\n").await?;
    }

    Ok(())
}
