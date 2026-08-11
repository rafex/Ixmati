//! Cliente síncrono del cache-server, sin tokio (ver DEC-0052).
//!
//! Mismo protocolo de texto que `cache_client.rs` (`CacheClient`, async) —
//! son intercambiables a nivel de wire, el servidor no depende de que el
//! cliente use tokio. Este cliente existe para que `ixmati-writer` pueda
//! sincronizar con el cache-server sin necesitar un runtime async en el
//! proceso.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::time::Duration;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct SyncCacheClient {
    stream: Mutex<UnixStream>,
}

impl SyncCacheClient {
    pub fn connect(path: &str) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(RESPONSE_TIMEOUT))?;
        stream.set_write_timeout(Some(RESPONSE_TIMEOUT))?;
        tracing::info!(path = %path, "SyncCacheClient connected");
        Ok(Self {
            stream: Mutex::new(stream),
        })
    }

    pub fn cache_key(store: &str, entity: &str, key: &str) -> String {
        format!("c:{store}:{entity}:{key}")
    }

    pub fn projection_key(name: &str, key: &str) -> String {
        format!("p:{name}:{key}")
    }

    pub fn get(&self, store: &str, entity: &str, key: &str) -> Option<Vec<u8>> {
        self.get_raw(&Self::cache_key(store, entity, key))
    }

    pub fn get_raw(&self, key: &str) -> Option<Vec<u8>> {
        let req = format!("GET {key}\n");
        self.send_command_with_response(&req)
    }

    pub fn get_projection(&self, name: &str, key: &str) -> Option<Vec<u8>> {
        self.get_raw(&Self::projection_key(name, key))
    }

    pub fn set(&self, store: &str, entity: &str, key: &str, value: &[u8]) {
        self.set_raw(&Self::cache_key(store, entity, key), value);
    }

    pub fn set_raw(&self, key: &str, value: &[u8]) {
        let req = format!("SET {key} {}\n", value.len());
        let mut guard = self.stream.lock().unwrap();
        if guard.write_all(req.as_bytes()).is_err() {
            tracing::warn!("SyncCacheClient: SET write header failed");
            return;
        }
        if guard.write_all(value).is_err() {
            tracing::warn!("SyncCacheClient: SET write value failed");
            return;
        }
        let _ = guard.write_all(b"\n");
        Self::read_simple_response(&mut guard);
    }

    pub fn set_projection(&self, name: &str, key: &str, value: &[u8]) {
        self.set_raw(&Self::projection_key(name, key), value);
    }

    pub fn del(&self, store: &str, entity: &str, key: &str) {
        self.del_raw(&Self::cache_key(store, entity, key));
    }

    pub fn del_raw(&self, key: &str) {
        let req = format!("DEL {key}\n");
        let mut guard = self.stream.lock().unwrap();
        if guard.write_all(req.as_bytes()).is_err() {
            return;
        }
        Self::read_simple_response(&mut guard);
    }

    pub fn delete_by_prefix(&self, prefix: &str) {
        let req = format!("DEL_PREFIX {prefix}\n");
        let mut guard = self.stream.lock().unwrap();
        if guard.write_all(req.as_bytes()).is_err() {
            return;
        }
        Self::read_simple_response(&mut guard);
    }

    pub fn flush(&self) {
        let mut guard = self.stream.lock().unwrap();
        if guard.write_all(b"FLUSH\n").is_err() {
            return;
        }
        Self::read_simple_response(&mut guard);
    }

    fn send_command_with_response(&self, req: &str) -> Option<Vec<u8>> {
        let mut guard = self.stream.lock().unwrap();
        if guard.write_all(req.as_bytes()).is_err() {
            return None;
        }

        let mut reader = BufReader::new(&mut *guard);
        let mut header = String::new();
        loop {
            header.clear();
            match reader.read_line(&mut header) {
                Ok(0) => return None,
                Ok(_) => {}
                Err(_) => return None,
            }
            if !header.trim().is_empty() {
                break;
            }
        }

        if let Some(len_str) = header.trim().strip_prefix("HIT ") {
            if let Ok(len) = len_str.trim().parse::<usize>() {
                let mut payload = vec![0u8; len];
                if reader.read_exact(&mut payload).is_ok() {
                    let mut trail = [0u8; 1];
                    let _ = reader.read_exact(&mut trail);
                    return Some(payload);
                }
            }
        }

        None
    }

    fn read_simple_response(guard: &mut UnixStream) {
        let mut reader = BufReader::new(&mut *guard);
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(_) => {
                let resp = line.trim();
                if resp != "OK" {
                    tracing::warn!(response = %resp, "SyncCacheClient: unexpected response");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "SyncCacheClient: read response failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::os::unix::net::UnixListener;

    #[test]
    fn cache_key_generation() {
        assert_eq!(
            SyncCacheClient::cache_key("pedidos", "pedido", "ped_1"),
            "c:pedidos:pedido:ped_1"
        );
    }

    /// Servidor de juguete que habla el mismo protocolo de texto que
    /// `cache_server.rs`, para validar que `SyncCacheClient` interopera
    /// correctamente sin necesitar el cache-server real.
    fn spawn_toy_server() -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ixmati-sync-cache-{}-{}.sock",
            std::process::id(),
            unique
        ));
        let path_str = path.to_str().unwrap().to_string();
        let listener = UnixListener::bind(&path_str).unwrap();

        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            let mut stored: Option<Vec<u8>> = None;

            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let line = line.trim();
                if let Some(key) = line.strip_prefix("GET ") {
                    let _ = key;
                    match &stored {
                        Some(payload) => {
                            let _ = writer.write_all(
                                format!("HIT {}\n", payload.len()).as_bytes(),
                            );
                            let _ = writer.write_all(payload);
                            let _ = writer.write_all(b"\n");
                        }
                        None => {
                            let _ = writer.write_all(b"MISS\n");
                        }
                    }
                } else if let Some(rest) = line.strip_prefix("SET ") {
                    let mut parts = rest.rsplitn(2, ' ');
                    let len: usize = parts.next().unwrap().parse().unwrap();
                    let mut payload = vec![0u8; len];
                    reader.read_exact(&mut payload).unwrap();
                    let mut trail = [0u8; 1];
                    let _ = reader.read_exact(&mut trail);
                    stored = Some(payload);
                    let _ = writer.write_all(b"OK\n");
                } else if line.starts_with("DEL ") || line.starts_with("DEL_PREFIX ") {
                    stored = None;
                    let _ = writer.write_all(b"OK\n");
                } else {
                    let _ = writer.write_all(b"OK\n");
                }
            }
        });

        // Deja que el listener arranque antes de que el cliente se conecte.
        std::thread::sleep(Duration::from_millis(20));
        path_str
    }

    #[test]
    fn set_then_get_round_trips() {
        let socket_path = spawn_toy_server();
        let client = SyncCacheClient::connect(&socket_path).unwrap();

        assert_eq!(client.get_raw("c:pedidos:pedido:p1"), None);

        client.set_raw("c:pedidos:pedido:p1", b"hello");
        assert_eq!(
            client.get_raw("c:pedidos:pedido:p1"),
            Some(b"hello".to_vec())
        );

        client.del_raw("c:pedidos:pedido:p1");
        assert_eq!(client.get_raw("c:pedidos:pedido:p1"), None);

        let _ = std::fs::remove_file(&socket_path);
    }
}
