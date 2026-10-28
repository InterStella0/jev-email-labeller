use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::{SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::json;
use subtle::ConstantTimeEq;

use super::{ApiError, ApiResult};
use crate::crypto;
use crate::db;
use crate::state::AppState;

const COOKIE: &str = "jev_session";
const SESSION_DAYS: i64 = 7;
const FREE_ATTEMPTS: u32 = 5;

#[derive(Deserialize)]
pub struct LoginInput {
    password: String,
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
}

fn cookie(state: &AppState, value: &str, max_age: i64) -> HeaderValue {
    let secure = if state.cfg.cookie_secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}"))
        .expect("cookie is ASCII")
}

pub async fn login(State(state): State<AppState>, Json(input): Json<LoginInput>) -> ApiResult<Response> {
    {
        let guard = state.login_guard.lock().unwrap();
        if let Some(until) = guard.locked_until
            && until > Instant::now()
        {
            let secs = (until - Instant::now()).as_secs() + 1;
            return Err(ApiError::TooManyRequests(format!("too many attempts; try again in {secs}s")));
        }
    }

    let ok: bool = input.password.as_bytes().ct_eq(state.cfg.admin_password.as_bytes()).into();
    if !ok {
        let mut guard = state.login_guard.lock().unwrap();
        guard.failures += 1;
        if guard.failures >= FREE_ATTEMPTS {
            let backoff = 2u64.pow((guard.failures - FREE_ATTEMPTS).min(6));
            guard.locked_until = Some(Instant::now() + Duration::from_secs(backoff));
        }
        tracing::warn!(failures = guard.failures, "failed login");
        return Err(ApiError::Unauthorized);
    }
    *state.login_guard.lock().unwrap() = Default::default();

    let token = crypto::random_token();
    let expires = (Utc::now() + chrono::Duration::days(SESSION_DAYS)).to_rfc3339_opts(SecondsFormat::Secs, true);
    sqlx::query("DELETE FROM sessions WHERE expires_at < ?").bind(db::now()).execute(&state.db).await?;
    sqlx::query("INSERT INTO sessions (token_hash, expires_at) VALUES (?, ?)")
        .bind(crypto::sha256_hex(&token))
        .bind(expires)
        .execute(&state.db)
        .await?;

    let mut res = Json(json!({ "ok": true })).into_response();
    res.headers_mut().insert(header::SET_COOKIE, cookie(&state, &token, SESSION_DAYS * 86400));
    Ok(res)
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = session_token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(crypto::sha256_hex(&token))
            .execute(&state.db)
            .await?;
    }
    let mut res = Json(json!({ "ok": true })).into_response();
    res.headers_mut().insert(header::SET_COOKIE, cookie(&state, "", 0));
    Ok(res)
}

pub async fn me() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

pub async fn require_session(State(state): State<AppState>, req: Request, next: Next) -> ApiResult<Response> {
    let token = session_token(req.headers()).ok_or(ApiError::Unauthorized)?;
    let found: Option<(String,)> =
        sqlx::query_as("SELECT token_hash FROM sessions WHERE token_hash = ? AND expires_at > ?")
            .bind(crypto::sha256_hex(&token))
            .bind(db::now())
            .fetch_optional(&state.db)
            .await?;
    if found.is_none() {
        return Err(ApiError::Unauthorized);
    }
    Ok(next.run(req).await)
}
