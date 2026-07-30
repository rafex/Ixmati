use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{Json, Response},
};

use crate::auth::session::{ApiKey, AuthCredentials};
use ixmati_core::Error;

#[derive(Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub valid_keys: Vec<ApiKey>,
}

impl AuthConfig {
    pub fn new(keys: Vec<ApiKey>) -> Self {
        Self {
            enabled: true,
            valid_keys: keys,
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            valid_keys: Vec::new(),
        }
    }
}

pub async fn require_auth(
    axum::extract::State(config): axum::extract::State<AuthConfig>,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<Error>)> {
    if !config.enabled {
        return Ok(next.run(request).await);
    }

    let credentials = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    let creds = AuthCredentials::from_header(credentials);

    match creds {
        Some(AuthCredentials::ApiKey(key)) => {
            if crate::auth::session::validate_api_key(&key, &config.valid_keys).is_some() {
                return Ok(next.run(request).await);
            }
            Err((
                StatusCode::UNAUTHORIZED,
                Json(Error::Internal {
                    detail: "invalid API key".into(),
                }),
            ))
        }
        Some(AuthCredentials::BearerToken(token)) => {
            if crate::auth::session::validate_api_key(&token, &config.valid_keys).is_some() {
                return Ok(next.run(request).await);
            }
            Err((
                StatusCode::UNAUTHORIZED,
                Json(Error::Internal {
                    detail: "invalid session token".into(),
                }),
            ))
        }
        None => Err((
            StatusCode::UNAUTHORIZED,
            Json(Error::Internal {
                detail: "Authorization header required".into(),
            }),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use chrono::Utc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn auth_disabled_allows_all() {
        let config = AuthConfig::disabled();

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .route_layer(axum::middleware::from_fn_with_state(
                config,
                require_auth,
            ));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_auth_header_is_rejected() {
        let config = AuthConfig::new(vec![]);

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .route_layer(axum::middleware::from_fn_with_state(
                config,
                require_auth,
            ));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn valid_api_key_is_allowed() {
        let config = AuthConfig::new(vec![ApiKey {
            key_id: "ix-key-1".into(),
            store_access: vec!["pedidos".into()],
            created_at: Utc::now(),
        }]);

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .route_layer(axum::middleware::from_fn_with_state(
                config,
                require_auth,
            ));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("Authorization", "ApiKey ix-key-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_api_key_is_rejected() {
        let config = AuthConfig::new(vec![ApiKey {
            key_id: "ix-key-1".into(),
            store_access: vec!["pedidos".into()],
            created_at: Utc::now(),
        }]);

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .route_layer(axum::middleware::from_fn_with_state(
                config,
                require_auth,
            ));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("Authorization", "ApiKey bad-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
