use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug, Serialize)]
pub enum Error {
    #[error("version conflict: {detail}")]
    VersionConflict {
        detail: String,
        store: String,
        idempotency_key: String,
    },

    #[error("duplicate: {detail}")]
    Duplicate {
        detail: String,
        store: String,
        idempotency_key: String,
    },

    #[error("store not found: {store}")]
    StoreNotFound { store: String },

    #[error("entity not found: {entity} in {store}")]
    EntityNotFound { store: String, entity: String },

    #[error("write rejected: {detail}")]
    WriteRejected {
        detail: String,
        store: String,
        idempotency_key: String,
    },

    #[error("permission denied for store {store}: {detail}")]
    PermissionDenied { store: String, detail: String },

    #[error("queue full: {store}")]
    QueueFull { store: String },

    #[error("outbox backlog over threshold: {store} ({depth} > {threshold})")]
    OutboxBacklog {
        store: String,
        depth: i64,
        threshold: i64,
    },

    #[error("projection error: {detail}")]
    ProjectionError { detail: String },

    #[error("internal error: {detail}")]
    Internal { detail: String },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::VersionConflict { .. } => "VERSION_CONFLICT",
            Error::Duplicate { .. } => "DUPLICATE",
            Error::StoreNotFound { .. } => "STORE_NOT_FOUND",
            Error::EntityNotFound { .. } => "ENTITY_NOT_FOUND",
            Error::WriteRejected { .. } => "WRITE_REJECTED",
            Error::PermissionDenied { .. } => "PERMISSION_DENIED",
            Error::QueueFull { .. } => "QUEUE_FULL",
            Error::OutboxBacklog { .. } => "OUTBOX_BACKLOG",
            Error::ProjectionError { .. } => "PROJECTION_ERROR",
            Error::Internal { .. } => "INTERNAL",
        }
    }
}
