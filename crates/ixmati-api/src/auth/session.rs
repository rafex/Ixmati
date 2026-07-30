use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub api_key_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub key_id: String,
    pub store_access: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl Session {
    pub fn new(api_key_id: &str, ttl_seconds: i64) -> Self {
        let now = Utc::now();
        Self {
            token: Uuid::new_v4().to_string(),
            api_key_id: api_key_id.to_string(),
            created_at: now,
            expires_at: now + Duration::seconds(ttl_seconds),
        }
    }

    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }

    pub fn is_expired(&self) -> bool {
        !self.is_valid()
    }
}

pub fn validate_api_key<'a>(key: &str, valid_keys: &'a [ApiKey]) -> Option<&'a ApiKey> {
    valid_keys.iter().find(|k| k.key_id == key)
}

#[derive(Debug, Clone)]
pub enum AuthCredentials {
    BearerToken(String),
    ApiKey(String),
}

impl AuthCredentials {
    pub fn from_header(header: Option<&str>) -> Option<Self> {
        let header = header?;

        if let Some(token) = header.strip_prefix("Bearer ") {
            return Some(AuthCredentials::BearerToken(token.to_string()));
        }

        if let Some(key) = header.strip_prefix("ApiKey ") {
            return Some(AuthCredentials::ApiKey(key.to_string()));
        }

        Some(AuthCredentials::ApiKey(header.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_is_valid_when_not_expired() {
        let session = Session::new("key-1", 3600);
        assert!(session.is_valid());
        assert!(!session.is_expired());
    }

    #[test]
    fn session_expires_after_ttl() {
        let session = Session::new("key-1", 0);
        assert!(session.is_expired());
    }

    #[test]
    fn validate_known_api_key() {
        let keys = vec![ApiKey {
            key_id: "ix-key-1".into(),
            store_access: vec!["pedidos".into()],
            created_at: Utc::now(),
        }];

        let found = validate_api_key("ix-key-1", &keys);
        assert!(found.is_some());
        assert_eq!(found.unwrap().store_access, vec!["pedidos"]);
    }

    #[test]
    fn unknown_api_key_returns_none() {
        let keys = vec![ApiKey {
            key_id: "ix-key-1".into(),
            store_access: vec!["pedidos".into()],
            created_at: Utc::now(),
        }];

        assert!(validate_api_key("unknown", &keys).is_none());
    }

    #[test]
    fn credentials_from_bearer_header() {
        let creds = AuthCredentials::from_header(Some("Bearer abc123"));
        assert!(matches!(creds, Some(AuthCredentials::BearerToken(t)) if t == "abc123"));
    }

    #[test]
    fn credentials_from_api_key_header() {
        let creds = AuthCredentials::from_header(Some("ApiKey ix-key-1"));
        assert!(matches!(creds, Some(AuthCredentials::ApiKey(k)) if k == "ix-key-1"));
    }

    #[test]
    fn credentials_from_raw_header() {
        let creds = AuthCredentials::from_header(Some("ix-key-raw"));
        assert!(matches!(creds, Some(AuthCredentials::ApiKey(k)) if k == "ix-key-raw"));
    }

    #[test]
    fn no_header_returns_none() {
        assert!(AuthCredentials::from_header(None).is_none());
    }
}
