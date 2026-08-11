//! Hilo de sistema operativo dedicado, dueño exclusivo de la conexión
//! SQLite de escritura (ver DEC-0052).
//!
//! Reemplaza `Arc<Mutex<Connection>>` + `spawn_blocking` por llamada
//! (Opción H, DEC-0050) y corrige el modo de falla del `WriteActor`
//! original (DEC-0051): la comunicación con este hilo usa únicamente
//! `std::sync::mpsc` en ambas direcciones — sin `tokio::sync::oneshot` ni
//! ningún otro canal de tokio cruzando la frontera. El hilo que llama a
//! `process_batch` es un hilo de SO normal (el loop principal en
//! `main.rs`, que ya no corre dentro de un runtime async), así que no hay
//! ninguna tarea de tokio bloqueada esperando una respuesta — el
//! mecanismo que DEC-0051 identificó como la causa más probable del
//! cuelgue silencioso simplemente no puede ocurrir porque no existe.

use ixmati_core::WriteEnvelope;

use crate::BatchResult;
use crate::db;
use crate::write_engine::WriteEngine;

struct Job {
    cmds: Vec<WriteEnvelope>,
    reply: std::sync::mpsc::Sender<rusqlite::Result<BatchResult>>,
}

#[derive(Clone)]
pub struct WriteHandle {
    tx: std::sync::mpsc::Sender<Job>,
}

impl WriteHandle {
    /// Arranca el hilo de escritura. Abre la conexión SQLite en el propio
    /// hilo (no antes) para que quede exclusivamente en su posesión desde
    /// el primer momento.
    pub fn spawn(db_path: String) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();

        std::thread::Builder::new()
            .name("ixmati-write".into())
            .spawn(move || {
                let mut conn = db::open_with_pragmas(&db_path)
                    .unwrap_or_else(|e| panic!("write thread: failed to open {db_path}: {e}"));

                while let Ok(job) = rx.recv() {
                    let result = WriteEngine::process_batch(&mut conn, &job.cmds);
                    // Si quien pidió el batch ya no está esperando (p.ej. se
                    // cayó el hilo llamador), no hay nada que hacer con el
                    // error de `send` — el hilo de escritura sigue vivo para
                    // el próximo job.
                    let _ = job.reply.send(result);
                }

                tracing::warn!("write thread: canal de jobs cerrado, terminando");
            })
            .expect("failed to spawn write thread");

        Self { tx }
    }

    /// Envía un batch al hilo de escritura y bloquea el hilo llamador hasta
    /// tener el resultado. El hilo llamador es un hilo de SO normal (no una
    /// tarea de tokio) — bloquearlo acá es exactamente lo que se busca:
    /// serializar la escritura sin involucrar a ningún runtime async.
    pub fn process_batch(&self, cmds: Vec<WriteEnvelope>) -> rusqlite::Result<BatchResult> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Job {
                cmds,
                reply: reply_tx,
            })
            .expect("write thread died");
        reply_rx
            .recv()
            .expect("write thread dropped the reply channel")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cmd(key: &str, version: u64) -> WriteEnvelope {
        WriteEnvelope {
            op: "upsert".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: key.into(),
            version,
            ts: "2026-07-30T00:00:00Z".into(),
            idempotency_key: format!("idem-{key}-{version}"),
            ack_mode: "committed".into(),
            payload: serde_json::json!({"n": 1}),
        }
    }

    #[test]
    fn processes_a_batch_and_returns_the_result() {
        let path = std::env::temp_dir().join(format!(
            "ixmati-write-thread-test-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path_str = path.to_str().unwrap().to_string();

        {
            let conn = db::open_with_pragmas(&path_str).unwrap();
            WriteEngine::ensure_schema(&conn, "pedidos").unwrap();
        }

        let handle = WriteHandle::spawn(path_str.clone());
        let result = handle
            .process_batch(vec![make_cmd("p1", 1), make_cmd("p2", 1)])
            .unwrap();

        assert_eq!(result.committed, 2);
        assert_eq!(result.duplicates, 0);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn processes_multiple_batches_sequentially_on_the_same_connection() {
        let path = std::env::temp_dir().join(format!(
            "ixmati-write-thread-test-seq-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path_str = path.to_str().unwrap().to_string();

        {
            let conn = db::open_with_pragmas(&path_str).unwrap();
            WriteEngine::ensure_schema(&conn, "pedidos").unwrap();
        }

        let handle = WriteHandle::spawn(path_str.clone());
        handle.process_batch(vec![make_cmd("p1", 1)]).unwrap();
        let second = handle.process_batch(vec![make_cmd("p1", 2)]).unwrap();

        assert_eq!(second.committed, 1);
        assert_eq!(second.duplicates, 0);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
