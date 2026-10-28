mod accounts;
mod auth;
mod classifications;
mod labels;
mod misc;
mod rules;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router, middleware};
use serde::{Deserialize, Deserializer};
use serde_json::json;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/labels", get(labels::list).post(labels::create))
        .route("/labels/{id}", patch(labels::update).delete(labels::remove))
        .route("/rules", get(rules::list).post(rules::create))
        .route("/rules/{id}", patch(rules::update).delete(rules::remove))
        .route("/accounts", get(accounts::list).post(accounts::create))
        .route("/accounts/{id}", get(accounts::get_one).patch(accounts::update).delete(accounts::remove))
        .route("/accounts/{id}/test", post(accounts::test))
        .route("/accounts/{id}/poll", post(accounts::poll))
        .route("/oauth/{provider}/start", get(accounts::oauth_start))
        .route("/classifications", get(classifications::list))
        .route(
            "/classifications/{id}",
            get(classifications::get_one).patch(classifications::override_label).delete(classifications::remove),
        )
        .route("/classifications/{id}/apply", post(classifications::apply))
        .route("/classifications/{id}/reclassify", post(classifications::reclassify))
        .route("/settings", get(misc::get_settings).put(misc::put_settings))
        .route("/stats", get(misc::stats))
        .route("/playground/classify", post(misc::playground))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_session));

    let public = Router::new()
        .route("/health", get(misc::health))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        // Authenticated by the one-time `state` parameter; the session cookie is
        // SameSite=Strict and isn't sent on the provider's cross-site redirect.
        .route("/oauth/{provider}/callback", get(accounts::oauth_callback));

    Router::new().nest("/api", protected.merge(public)).layer(TraceLayer::new_for_http()).with_state(state)
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    TooManyRequests(String),
    #[error("{0}")]
    Upstream(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => Self::NotFound,
            sqlx::Error::Database(d) if d.is_unique_violation() => Self::Conflict("that name is already taken".into()),
            sqlx::Error::Database(d) if d.is_check_violation() => {
                Self::BadRequest(format!("invalid value: {}", d.message()))
            }
            _ => Self::Internal(e.into()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(e) => {
                tracing::error!(error = format!("{e:#}"), "request failed");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        let message = match &self {
            Self::Internal(e) => format!("{e:#}"),
            other => other.to_string(),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn bad_request(msg: impl Into<String>) -> ApiError {
    ApiError::BadRequest(msg.into())
}

/// Lets PATCH bodies distinguish "absent" (None) from "null" (Some(None)).
pub fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}
