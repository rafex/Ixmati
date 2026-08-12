//! Offline, atomic store lifecycle operations.
//!
//! The tool deliberately has no MQTT, HTTP or service-management dependency:
//! an operator must quiesce the topology first, then this library transforms
//! SQLite files and publishes complete destinations atomically.

use rusqlite::types::ValueRef;
use rusqlite::{Connection, Row, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

const HASH_ALGORITHM: &str = "sha256-key-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub operation: Operation,
    pub sources: Vec<SourceSpec>,
    #[serde(default)]
    pub target: Option<TargetSpec>,
    #[serde(default)]
    pub targets: Vec<TargetSpec>,
    #[serde(default = "default_hash_algorithm")]
    pub hash_algorithm: String,
    pub evidence_dir: PathBuf,
    #[serde(default)]
    pub quiesced: bool,
}

fn default_hash_algorithm() -> String {
    HASH_ALGORITHM.to_owned()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Rename,
    Merge,
    Split,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpec {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetSpec {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Plan,
    Verify,
    Execute,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Report {
    pub operation: Option<Operation>,
    pub sources: Vec<String>,
    pub targets: Vec<String>,
    pub payload_rows: usize,
    pub tombstone_rows: usize,
    pub idempotency_rows: usize,
    pub outbox_rows: usize,
    pub conflicts: usize,
    pub deduplicated_idempotency: usize,
    pub bucket_counts: BTreeMap<String, usize>,
    pub checksums: BTreeMap<String, String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("invalid manifest: {0}")]
    Manifest(String),
    #[error("precondition failed: {0}")]
    Precondition(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
struct StateRow {
    entity: String,
    key: String,
    version: u64,
    timestamp: String,
    source: String,
    payload: Option<Vec<u8>>,
    event_id: Option<String>,
}

#[derive(Debug, Clone)]
struct IdempotencyRow {
    idempotency_key: String,
    entity: String,
    key: String,
    version: u64,
    operation: Option<String>,
    digest: Option<String>,
    applied_at: String,
    source: String,
}

#[derive(Debug, Clone)]
struct OutboxRow {
    event_id: String,
    event_type: String,
    entity: String,
    key: String,
    version: u64,
    occurred_at: String,
    payload: Vec<u8>,
    published_at: Option<String>,
}

pub fn load_manifest(path: &Path) -> Result<Manifest, MigrationError> {
    let content = fs::read_to_string(path)?;
    let manifest: Manifest = toml::from_str(&content)
        .map_err(|e| MigrationError::Manifest(format!("{}: {e}", path.display())))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn validate_manifest(manifest: &Manifest) -> Result<(), MigrationError> {
    if manifest.sources.is_empty() {
        return Err(MigrationError::Manifest(
            "sources no puede estar vacío".into(),
        ));
    }
    if !manifest.quiesced {
        return Err(MigrationError::Manifest(
            "quiesced=true es obligatorio: la herramienta no detiene servicios".into(),
        ));
    }
    if manifest.hash_algorithm != HASH_ALGORITHM {
        return Err(MigrationError::Manifest(format!(
            "hash_algorithm debe ser {HASH_ALGORITHM}"
        )));
    }
    let mut names = std::collections::HashSet::new();
    let specs = manifest
        .sources
        .iter()
        .map(|source| (&source.name, &source.path))
        .chain(
            manifest
                .target
                .iter()
                .map(|target| (&target.name, &target.path)),
        )
        .chain(
            manifest
                .targets
                .iter()
                .map(|target| (&target.name, &target.path)),
        );
    for (name, path) in specs {
        validate_store_name(name)?;
        if !names.insert(name.clone()) && !matches!(manifest.operation, Operation::Merge) {
            return Err(MigrationError::Manifest(format!(
                "nombre duplicado: {name}"
            )));
        }
        if path.as_os_str().is_empty() {
            return Err(MigrationError::Manifest(format!("ruta vacía para {name}")));
        }
    }
    match manifest.operation {
        Operation::Rename | Operation::Merge => {
            if manifest.target.is_none() || !manifest.targets.is_empty() {
                return Err(MigrationError::Manifest(
                    "rename/merge requieren target y no targets".into(),
                ));
            }
        }
        Operation::Split => {
            if manifest.sources.len() != 1
                || manifest.target.is_some()
                || manifest.targets.len() < 2
            {
                return Err(MigrationError::Manifest(
                    "split requiere exactamente un source y al menos dos targets".into(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_store_name(name: &str) -> Result<(), MigrationError> {
    if name.is_empty()
        || name.len() > 63
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(MigrationError::Manifest(format!(
            "identificador de store inválido: {name:?}"
        )));
    }
    Ok(())
}

pub fn run(manifest: &Manifest, mode: Mode) -> Result<Report, MigrationError> {
    validate_manifest(manifest)?;
    for source in &manifest.sources {
        preflight_source(source, mode == Mode::Execute)?;
    }
    let mut report = Report {
        operation: Some(manifest.operation),
        sources: manifest.sources.iter().map(|s| s.name.clone()).collect(),
        targets: target_specs(manifest)
            .iter()
            .map(|s| s.name.clone())
            .collect(),
        ..Report::default()
    };

    match mode {
        Mode::Plan | Mode::Verify => {
            let (states, conflicts) = collect_states(manifest)?;
            report.conflicts = conflicts;
            report.payload_rows = states.iter().filter(|s| s.payload.is_some()).count();
            report.tombstone_rows = states.iter().filter(|s| s.payload.is_none()).count();
            let (idempotency, deduplicated) = collect_idempotency(manifest)?;
            report.idempotency_rows = idempotency.len();
            report.deduplicated_idempotency = deduplicated;
            report.outbox_rows = collect_outbox(manifest)?.len();
            if mode == Mode::Verify {
                validate_destinations(manifest, &mut report)?;
            }
        }
        Mode::Execute => execute(manifest, &mut report)?,
    }
    Ok(report)
}

fn target_specs(manifest: &Manifest) -> Vec<TargetSpec> {
    manifest
        .target
        .iter()
        .cloned()
        .chain(manifest.targets.iter().cloned())
        .collect()
}

fn preflight_source(source: &SourceSpec, write_checkpoint: bool) -> Result<(), MigrationError> {
    if !source.path.is_file() {
        return Err(MigrationError::Precondition(format!(
            "no existe SQLite: {}",
            source.path.display()
        )));
    }
    let conn = Connection::open(&source.path)?;
    let check: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if check != "ok" {
        return Err(MigrationError::Precondition(format!(
            "quick_check de {}: {check}",
            source.name
        )));
    }
    if table_exists(&conn, "_outbox")? {
        let pending: i64 = conn.query_row(
            "SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        if pending != 0 {
            return Err(MigrationError::Precondition(format!(
                "{} tiene {pending} eventos outbox pendientes",
                source.name
            )));
        }
    }
    if write_checkpoint {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA integrity_check;")?;
    }
    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, MigrationError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

fn collect_states(manifest: &Manifest) -> Result<(Vec<StateRow>, usize), MigrationError> {
    let mut states = HashMap::<(String, String), StateRow>::new();
    let mut conflicts = 0;
    for source in &manifest.sources {
        let conn = Connection::open(&source.path)?;
        let table = quote_ident(&format!("payload_{}", source.name));
        if table_exists(&conn, &format!("payload_{}", source.name))? {
            let sql = format!("SELECT entity,key,version,payload,updated_at FROM {table}");
            let rows = conn
                .prepare(&sql)?
                .query_map([], |row| {
                    Ok(StateRow {
                        entity: row.get(0)?,
                        key: row.get(1)?,
                        version: row.get::<_, i64>(2)? as u64,
                        payload: Some(read_blob(row, 3)?),
                        timestamp: row.get(4)?,
                        source: source.name.clone(),
                        event_id: None,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for row in rows {
                conflicts += usize::from(merge_state(&mut states, row)?);
            }
        }
        if table_exists(&conn, "_tombstones")? {
            let rows = conn
                .prepare("SELECT entity,key,version,deleted_at,event_id FROM _tombstones")?
                .query_map([], |row| {
                    Ok(StateRow {
                        entity: row.get(0)?,
                        key: row.get(1)?,
                        version: row.get::<_, i64>(2)? as u64,
                        payload: None,
                        timestamp: row.get(3)?,
                        source: source.name.clone(),
                        event_id: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for row in rows {
                conflicts += usize::from(merge_state(&mut states, row)?);
            }
        }
    }
    Ok((states.into_values().collect(), conflicts))
}

fn merge_state(
    states: &mut HashMap<(String, String), StateRow>,
    candidate: StateRow,
) -> Result<bool, MigrationError> {
    let key = (candidate.entity.clone(), candidate.key.clone());
    match states.get(&key) {
        None => {
            states.insert(key, candidate);
            Ok(false)
        }
        Some(current) => {
            let conflict = current.version != candidate.version
                || current.payload != candidate.payload
                || current.timestamp != candidate.timestamp;
            if compare_state(&candidate, current) == std::cmp::Ordering::Greater {
                states.insert(key, candidate);
            }
            Ok(conflict)
        }
    }
}

fn compare_state(a: &StateRow, b: &StateRow) -> std::cmp::Ordering {
    a.version
        .cmp(&b.version)
        .then_with(|| a.timestamp.cmp(&b.timestamp))
        .then_with(|| b.source.cmp(&a.source)) // lexicographically smaller source wins
}

fn collect_idempotency(
    manifest: &Manifest,
) -> Result<(Vec<IdempotencyRow>, usize), MigrationError> {
    let mut rows_by_key = HashMap::<String, IdempotencyRow>::new();
    let mut deduplicated = 0;
    for source in &manifest.sources {
        let conn = Connection::open(&source.path)?;
        if !table_exists(&conn, "_idempotency")? {
            continue;
        }
        let columns = columns(&conn, "_idempotency")?;
        let operation = columns.contains("operation");
        let digest = columns.contains("command_digest");
        let sql = format!(
            "SELECT idempotency_key,entity,key,version,{}, {},applied_at FROM _idempotency",
            if operation { "operation" } else { "NULL" },
            if digest { "command_digest" } else { "NULL" },
        );
        let rows = conn
            .prepare(&sql)?
            .query_map([], |row| {
                Ok(IdempotencyRow {
                    idempotency_key: row.get(0)?,
                    entity: row.get(1)?,
                    key: row.get(2)?,
                    version: row.get::<_, i64>(3)? as u64,
                    operation: row.get(4)?,
                    digest: row.get(5)?,
                    applied_at: row.get(6)?,
                    source: source.name.clone(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for row in rows {
            if let Some(existing) = rows_by_key.get(&row.idempotency_key) {
                if !same_idempotency(existing, &row) {
                    return Err(MigrationError::Conflict(format!(
                        "idempotency_key {} diverge entre {} y {}",
                        row.idempotency_key, existing.source, row.source
                    )));
                }
                deduplicated += 1;
            } else {
                rows_by_key.insert(row.idempotency_key.clone(), row);
            }
        }
    }
    Ok((rows_by_key.into_values().collect(), deduplicated))
}

fn same_idempotency(a: &IdempotencyRow, b: &IdempotencyRow) -> bool {
    a.entity == b.entity
        && a.key == b.key
        && a.version == b.version
        && a.operation == b.operation
        && a.digest == b.digest
}

fn collect_outbox(manifest: &Manifest) -> Result<Vec<OutboxRow>, MigrationError> {
    let mut by_event = HashMap::<String, OutboxRow>::new();
    for source in &manifest.sources {
        let conn = Connection::open(&source.path)?;
        if !table_exists(&conn, "_outbox")? {
            continue;
        }
        let rows = conn.prepare(
            "SELECT event_id,event_type,entity,key,version,occurred_at,payload,published_at FROM _outbox"
        )?.query_map([], |row| Ok(OutboxRow {
            event_id: row.get(0)?, event_type: row.get(1)?, entity: row.get(2)?, key: row.get(3)?,
            version: row.get::<_, i64>(4)? as u64, occurred_at: row.get(5)?, payload: read_blob(row, 6)?, published_at: row.get(7)?,
        }))?.collect::<Result<Vec<_>, _>>()?;
        for row in rows {
            if let Some(existing) = by_event.get(&row.event_id) {
                if existing.event_type != row.event_type || existing.payload != row.payload {
                    return Err(MigrationError::Conflict(format!(
                        "event_id {} tiene payload divergente",
                        row.event_id
                    )));
                }
            } else {
                by_event.insert(row.event_id.clone(), row);
            }
        }
    }
    Ok(by_event.into_values().collect())
}

fn columns(
    conn: &Connection,
    table: &str,
) -> Result<std::collections::HashSet<String>, MigrationError> {
    Ok(conn
        .prepare(&format!("PRAGMA table_info({})", quote_ident(table)))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<_, _>>()?)
}

fn read_blob(row: &Row<'_>, index: usize) -> rusqlite::Result<Vec<u8>> {
    match row.get_ref(index)? {
        ValueRef::Blob(bytes) | ValueRef::Text(bytes) => Ok(bytes.to_vec()),
        ValueRef::Null => Ok(Vec::new()),
        value => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            value.data_type(),
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "payload must be BLOB, TEXT or NULL",
            )),
        )),
    }
}

fn execute(manifest: &Manifest, report: &mut Report) -> Result<(), MigrationError> {
    fs::create_dir_all(&manifest.evidence_dir)?;
    let (states, conflicts) = collect_states(manifest)?;
    report.conflicts = conflicts;
    let (idempotency, deduplicated) = collect_idempotency(manifest)?;
    let outbox = collect_outbox(manifest)?;
    report.payload_rows = states.iter().filter(|s| s.payload.is_some()).count();
    report.tombstone_rows = states.iter().filter(|s| s.payload.is_none()).count();
    report.idempotency_rows = idempotency.len();
    report.deduplicated_idempotency = deduplicated;
    report.outbox_rows = outbox.len();

    for source in &manifest.sources {
        let backup = manifest
            .evidence_dir
            .join(format!("{}.pre-migration.db", source.name));
        fs::copy(&source.path, backup)?;
    }

    let destinations = target_specs(manifest);
    for destination in &destinations {
        if destination.path.exists() {
            return Err(MigrationError::Precondition(format!(
                "destino ya existe: {}",
                destination.path.display()
            )));
        }
    }
    let temp_paths: Vec<PathBuf> = destinations
        .iter()
        .map(|d| temporary_path(&d.path))
        .collect();
    let result = write_destinations(
        manifest,
        &states,
        &idempotency,
        &outbox,
        &destinations,
        &temp_paths,
        report,
    );
    if let Err(error) = result {
        for path in &temp_paths {
            let _ = fs::remove_file(path);
        }
        return Err(error);
    }
    for (temp, destination) in temp_paths.iter().zip(&destinations) {
        fs::rename(temp, &destination.path)?;
        report
            .checksums
            .insert(destination.name.clone(), file_checksum(&destination.path)?);
    }
    fs::write(
        manifest.evidence_dir.join("migration-report.json"),
        serde_json::to_vec_pretty(report)?,
    )?;
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(format!(".ixmati-migrate-{}-tmp", std::process::id()));
    PathBuf::from(value)
}

fn write_destinations(
    manifest: &Manifest,
    states: &[StateRow],
    idempotency: &[IdempotencyRow],
    outbox: &[OutboxRow],
    destinations: &[TargetSpec],
    temp_paths: &[PathBuf],
    report: &mut Report,
) -> Result<(), MigrationError> {
    let source = &manifest.sources[0];
    let mut connections = Vec::new();
    for (destination, path) in destinations.iter().zip(temp_paths) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        create_schema(&conn, &destination.name)?;
        connections.push(conn);
    }

    let state_targets = |state: &StateRow| -> Vec<usize> {
        match manifest.operation {
            Operation::Split => vec![bucket_for_key(&state.key, destinations.len())],
            Operation::Rename | Operation::Merge => vec![0],
        }
    };
    for state in states {
        for index in state_targets(state) {
            insert_state(&connections[index], &destinations[index].name, state)?;
            *report
                .bucket_counts
                .entry(destinations[index].name.clone())
                .or_default() += 1;
        }
    }
    for row in idempotency {
        let index = if manifest.operation == Operation::Split {
            bucket_for_key(&row.key, destinations.len())
        } else {
            0
        };
        insert_idempotency(&connections[index], &destinations[index].name, row)?;
    }
    for row in outbox {
        let index = if manifest.operation == Operation::Split {
            bucket_for_key(&row.key, destinations.len())
        } else {
            0
        };
        insert_outbox(&connections[index], &destinations[index].name, row)?;
    }
    for conn in connections {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA integrity_check;")?;
    }
    let _ = source;
    Ok(())
}

fn create_schema(conn: &Connection, store: &str) -> Result<(), MigrationError> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA busy_timeout=5000;
         PRAGMA foreign_keys=ON;",
    )?;
    let payload = quote_ident(&format!("payload_{store}"));
    conn.execute_batch(&format!(
        "CREATE TABLE _idempotency (idempotency_key TEXT NOT NULL, store TEXT NOT NULL, entity TEXT NOT NULL, key TEXT NOT NULL, version INTEGER NOT NULL, operation TEXT, command_digest TEXT, applied_at TEXT NOT NULL, PRIMARY KEY(store,idempotency_key));
         CREATE INDEX idx_idempotency_entity_key_version ON _idempotency(store,entity,key,version);
         CREATE TABLE _tombstones (entity TEXT NOT NULL, key TEXT NOT NULL, version INTEGER NOT NULL, deleted_at TEXT NOT NULL, event_id TEXT, PRIMARY KEY(entity,key));
         CREATE INDEX idx_tombstones_entity_key_version ON _tombstones(entity,key,version);
         CREATE TABLE _outbox (id INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL, event_type TEXT NOT NULL, store TEXT NOT NULL, entity TEXT NOT NULL, key TEXT NOT NULL, version INTEGER NOT NULL, occurred_at TEXT NOT NULL, payload BLOB NOT NULL, published_at TEXT);
         CREATE INDEX idx_outbox_published ON _outbox(published_at);
         CREATE INDEX idx_outbox_store ON _outbox(store,published_at);
         CREATE TABLE {payload} (entity TEXT NOT NULL, key TEXT NOT NULL, version INTEGER NOT NULL, payload BLOB NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(entity,key));"
    ))?;
    Ok(())
}

fn insert_state(conn: &Connection, _store: &str, state: &StateRow) -> Result<(), MigrationError> {
    if let Some(payload) = &state.payload {
        conn.execute(
            &format!(
                "INSERT INTO {} (entity,key,version,payload,updated_at) VALUES (?1,?2,?3,?4,?5)",
                quote_ident(&format!("payload_{}", _store))
            ),
            params![
                state.entity,
                state.key,
                state.version as i64,
                payload,
                state.timestamp
            ],
        )?;
    } else {
        conn.execute(
            "INSERT INTO _tombstones(entity,key,version,deleted_at,event_id) VALUES(?1,?2,?3,?4,?5)",
            params![state.entity, state.key, state.version as i64, state.timestamp, state.event_id],
        )?;
    }
    Ok(())
}

fn insert_idempotency(
    conn: &Connection,
    store: &str,
    row: &IdempotencyRow,
) -> Result<(), MigrationError> {
    conn.execute(
        "INSERT INTO _idempotency(idempotency_key,store,entity,key,version,operation,command_digest,applied_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![row.idempotency_key, store, row.entity, row.key, row.version as i64, row.operation, row.digest, row.applied_at],
    )?;
    Ok(())
}

fn insert_outbox(conn: &Connection, store: &str, row: &OutboxRow) -> Result<(), MigrationError> {
    conn.execute(
        "INSERT INTO _outbox(event_id,event_type,store,entity,key,version,occurred_at,payload,published_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![row.event_id, row.event_type, store, row.entity, row.key, row.version as i64, row.occurred_at, row.payload, row.published_at],
    )?;
    Ok(())
}

pub fn bucket_for_key(key: &str, buckets: usize) -> usize {
    assert!(buckets > 0);
    let digest = Sha256::digest(key.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    (u64::from_be_bytes(bytes) % buckets as u64) as usize
}

fn validate_destinations(manifest: &Manifest, report: &mut Report) -> Result<(), MigrationError> {
    for destination in target_specs(manifest) {
        if !destination.path.is_file() {
            continue;
        }
        let conn = Connection::open(&destination.path)?;
        let check: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if check != "ok" {
            return Err(MigrationError::Precondition(format!(
                "integrity_check destino {}: {check}",
                destination.name
            )));
        }
        report
            .checksums
            .insert(destination.name, file_checksum(&destination.path)?);
    }
    Ok(())
}

fn file_checksum(path: &Path) -> Result<String, MigrationError> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn bucket_mapping_is_stable() {
        assert_eq!(bucket_for_key("pedido-1", 3), bucket_for_key("pedido-1", 3));
        assert!(bucket_for_key("pedido-1", 3) < 3);
    }

    #[test]
    fn manifest_rejects_non_quiesced_execution() {
        let manifest = Manifest {
            operation: Operation::Rename,
            sources: vec![SourceSpec {
                name: "old".into(),
                path: PathBuf::from("old.db"),
            }],
            target: Some(TargetSpec {
                name: "new".into(),
                path: PathBuf::from("new.db"),
            }),
            targets: vec![],
            hash_algorithm: HASH_ALGORITHM.into(),
            evidence_dir: PathBuf::from("evidence"),
            quiesced: false,
        };
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn creates_expected_schema() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn, "new").unwrap();
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='_tombstones'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1);
    }

    #[test]
    fn rename_round_trip_is_atomic() {
        let dir =
            std::env::temp_dir().join(format!("ixmati-store-migrate-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source_path = dir.join("old.db");
        let target_path = dir.join("new.db");
        let evidence = dir.join("evidence");
        let conn = Connection::open(&source_path).unwrap();
        conn.execute_batch("CREATE TABLE \"payload_old\" (entity TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,payload BLOB NOT NULL,updated_at TEXT NOT NULL,PRIMARY KEY(entity,key)); CREATE TABLE _idempotency (idempotency_key TEXT NOT NULL,store TEXT NOT NULL,entity TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,operation TEXT,command_digest TEXT,applied_at TEXT NOT NULL,PRIMARY KEY(store,idempotency_key)); CREATE TABLE _outbox (id INTEGER PRIMARY KEY,event_id TEXT,event_type TEXT,store TEXT,entity TEXT,key TEXT,version INTEGER,occurred_at TEXT,payload BLOB,published_at TEXT); INSERT INTO payload_old VALUES('pedido','p1',1,'{}','2026-01-01'); INSERT INTO _idempotency VALUES('i1','old','pedido','p1',1,'upsert','d','2026-01-01');").unwrap();
        drop(conn);
        let manifest = Manifest {
            operation: Operation::Rename,
            sources: vec![SourceSpec {
                name: "old".into(),
                path: source_path,
            }],
            target: Some(TargetSpec {
                name: "new".into(),
                path: target_path.clone(),
            }),
            targets: vec![],
            hash_algorithm: HASH_ALGORITHM.into(),
            evidence_dir: evidence,
            quiesced: true,
        };
        run(&manifest, Mode::Execute).unwrap();
        let conn = Connection::open(target_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM payload_new", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        drop(conn);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn merge_uses_lww() {
        let dir = std::env::temp_dir().join(format!(
            "ixmati-store-migrate-merge-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let left = dir.join("left.db");
        let right = dir.join("right.db");
        let merged = dir.join("merged.db");
        for (path, store, version, payload) in
            [(&left, "left", 1, "left"), (&right, "right", 2, "right")]
        {
            let conn = Connection::open(path).unwrap();
            conn.execute_batch(&format!(
                "CREATE TABLE payload_{store}(entity TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,payload BLOB NOT NULL,updated_at TEXT NOT NULL,PRIMARY KEY(entity,key));
                 CREATE TABLE _idempotency(idempotency_key TEXT NOT NULL,store TEXT NOT NULL,entity TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,operation TEXT,command_digest TEXT,applied_at TEXT NOT NULL,PRIMARY KEY(store,idempotency_key));
                 CREATE TABLE _outbox(id INTEGER PRIMARY KEY,event_id TEXT,event_type TEXT,store TEXT,entity TEXT,key TEXT,version INTEGER,occurred_at TEXT,payload BLOB,published_at TEXT);
                 INSERT INTO payload_{store} VALUES('pedido','p1',{version},'{payload}','2026-01-01');"
            )).unwrap();
        }
        let manifest = Manifest {
            operation: Operation::Merge,
            sources: vec![
                SourceSpec {
                    name: "left".into(),
                    path: left,
                },
                SourceSpec {
                    name: "right".into(),
                    path: right,
                },
            ],
            target: Some(TargetSpec {
                name: "merged".into(),
                path: merged.clone(),
            }),
            targets: vec![],
            hash_algorithm: HASH_ALGORITHM.into(),
            evidence_dir: dir.join("merge-evidence"),
            quiesced: true,
        };
        let report = run(&manifest, Mode::Execute).unwrap();
        assert_eq!(report.conflicts, 1);
        let conn = Connection::open(merged).unwrap();
        let version: i64 = conn
            .query_row("SELECT version FROM payload_merged", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        drop(conn);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn split_routes_all_durable_rows_by_stable_hash() {
        let dir = std::env::temp_dir().join(format!(
            "ixmati-store-migrate-split-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source_path = dir.join("source.db");
        let conn = Connection::open(&source_path).unwrap();
        create_schema(&conn, "source").unwrap();
        let key_a = "key-a";
        let key_b = (0..100)
            .map(|index| format!("key-{index}"))
            .find(|key| bucket_for_key(key_a, 2) != bucket_for_key(key, 2))
            .unwrap();
        let keys = [key_a.to_owned(), key_b];
        for (index, key) in keys.iter().enumerate() {
            conn.execute(
                "INSERT INTO payload_source(entity,key,version,payload,updated_at) VALUES('pedido',?1,1,?2,'2026-01-01')",
                params![key, format!("{{\"n\":{index}}}")],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO _idempotency(idempotency_key,store,entity,key,version,operation,command_digest,applied_at) VALUES(?1,'source','pedido',?2,1,'upsert',?3,'2026-01-01')",
                params![format!("idem-{index}"), key, format!("digest-{index}")],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO _outbox(event_id,event_type,store,entity,key,version,occurred_at,payload,published_at) VALUES(?1,'upsert','source','pedido',?2,1,'2026-01-01',?3,'2026-01-01')",
                params![format!("event-{index}"), key, format!("{{\"n\":{index}}}")],
            )
            .unwrap();
        }
        drop(conn);

        let targets = vec![
            TargetSpec {
                name: "orders-0".into(),
                path: dir.join("orders-0.db"),
            },
            TargetSpec {
                name: "orders-1".into(),
                path: dir.join("orders-1.db"),
            },
        ];
        let manifest = Manifest {
            operation: Operation::Split,
            sources: vec![SourceSpec {
                name: "source".into(),
                path: source_path,
            }],
            target: None,
            targets: targets.clone(),
            hash_algorithm: HASH_ALGORITHM.into(),
            evidence_dir: dir.join("evidence"),
            quiesced: true,
        };
        let report = run(&manifest, Mode::Execute).unwrap();
        assert_eq!(report.payload_rows, 2);
        assert_eq!(report.idempotency_rows, 2);
        assert_eq!(report.outbox_rows, 2);
        assert_eq!(report.bucket_counts.values().sum::<usize>(), 2);

        for (index, target) in targets.iter().enumerate() {
            let conn = Connection::open(&target.path).unwrap();
            let payload_count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM \"payload_{}\"", target.name),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let idempotency_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM _idempotency", [], |row| row.get(0))
                .unwrap();
            let outbox_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM _outbox", [], |row| row.get(0))
                .unwrap();
            assert_eq!(payload_count, idempotency_count);
            assert_eq!(idempotency_count, outbox_count);
            assert_eq!(
                payload_count,
                *report.bucket_counts.get(&target.name).unwrap_or(&0) as i64
            );
            assert_eq!(
                payload_count,
                keys.iter()
                    .filter(|key| bucket_for_key(key, 2) == index)
                    .count() as i64
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn merge_rejects_divergent_idempotency_digest() {
        let dir = std::env::temp_dir().join(format!(
            "ixmati-store-migrate-idempotency-conflict-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let left = dir.join("left.db");
        let right = dir.join("right.db");
        for (path, store, digest) in [
            (&left, "left", "digest-left"),
            (&right, "right", "digest-right"),
        ] {
            let conn = Connection::open(path).unwrap();
            create_schema(&conn, store).unwrap();
            conn.execute(
                "INSERT INTO _idempotency(idempotency_key,store,entity,key,version,operation,command_digest,applied_at) VALUES('same-key',?1,'pedido','p1',1,'upsert',?2,'2026-01-01')",
                params![store, digest],
            )
            .unwrap();
        }
        let manifest = Manifest {
            operation: Operation::Merge,
            sources: vec![
                SourceSpec {
                    name: "left".into(),
                    path: left,
                },
                SourceSpec {
                    name: "right".into(),
                    path: right,
                },
            ],
            target: Some(TargetSpec {
                name: "merged".into(),
                path: dir.join("merged.db"),
            }),
            targets: vec![],
            hash_algorithm: HASH_ALGORITHM.into(),
            evidence_dir: dir.join("evidence"),
            quiesced: true,
        };
        let result = run(&manifest, Mode::Plan);
        assert!(
            matches!(result, Err(MigrationError::Conflict(message)) if message.contains("same-key"))
        );
        assert!(!dir.join("merged.db").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
