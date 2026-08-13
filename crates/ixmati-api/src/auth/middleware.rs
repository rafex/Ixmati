use axum::{
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use prost::Message;

use crate::auth::session::{ApiKey, AuthCredentials, AuthIdentity};
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
    mut request: Request,
    next: Next,
) -> Result<Response, Response> {
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
            if let Some(api_key) = crate::auth::session::validate_api_key(&key, &config.valid_keys)
            {
                request.extensions_mut().insert(AuthIdentity::from(api_key));
                return Ok(next.run(request).await);
            }
            Err(auth_error(&request, "invalid API key"))
        }
        Some(AuthCredentials::BearerToken(token)) => {
            if let Some(api_key) =
                crate::auth::session::validate_api_key(&token, &config.valid_keys)
            {
                request.extensions_mut().insert(AuthIdentity::from(api_key));
                return Ok(next.run(request).await);
            }
            Err(auth_error(&request, "invalid session token"))
        }
        None => Err(auth_error(&request, "Authorization header required")),
    }
}

fn auth_error(request: &Request, detail: &str) -> Response {
    if wants_protobuf(request) {
        let mut bytes = Vec::new();
        crate::grpc::pb::ErrorDetail {
            error: "UNAUTHORIZED".into(),
            detail: detail.into(),
            store: String::new(),
            idempotency_key: String::new(),
        }
        .encode(&mut bytes)
        .expect("protobuf encoding cannot fail");
        return (
            StatusCode::UNAUTHORIZED,
            [(header::CONTENT_TYPE, crate::grpc::PROTOBUF_CONTENT_TYPE)],
            bytes,
        )
            .into_response();
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(Error::Internal {
            detail: detail.into(),
        }),
    )
        .into_response()
}

fn wants_protobuf(request: &Request) -> bool {
    [header::ACCEPT, header::CONTENT_TYPE]
        .into_iter()
        .filter_map(|name| request.headers().get(name))
        .filter_map(|value| value.to_str().ok())
        .any(|value| {
            value
                .split(',')
                .any(|item| item.trim().starts_with(crate::grpc::PROTOBUF_CONTENT_TYPE))
        })
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
            .route_layer(axum::middleware::from_fn_with_state(config, require_auth));

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
            .route_layer(axum::middleware::from_fn_with_state(config, require_auth));

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
            .route_layer(axum::middleware::from_fn_with_state(config, require_auth));

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
    async fn scoped_api_key_identity_is_attached_to_request() {
        let config = AuthConfig::new(vec![ApiKey {
            key_id: "orders-key".into(),
            store_access: vec!["orders".into()],
            created_at: Utc::now(),
        }]);

        let app =
            Router::new()
                .route(
                    "/test",
                    get(
                        |axum::extract::Extension(identity): axum::extract::Extension<
                            AuthIdentity,
                        >| async move {
                            if identity.allows_store("orders") && !identity.allows_store("users") {
                                StatusCode::OK
                            } else {
                                StatusCode::INTERNAL_SERVER_ERROR
                            }
                        },
                    ),
                )
                .route_layer(axum::middleware::from_fn_with_state(config, require_auth));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("Authorization", "ApiKey orders-key")
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
            .route_layer(axum::middleware::from_fn_with_state(config, require_auth));

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
