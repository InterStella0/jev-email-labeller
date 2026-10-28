use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Redirect;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::types::Json as SqlJson;

use super::{ApiError, ApiResult, bad_request};
use crate::crypto;
use crate::db::{self, Account};
use crate::mail;
use crate::mail::gmail::GmailProvider;
use crate::mail::imap::ImapSettings;
use crate::mail::oauth::{self, OAuthProvider, OAuthSession};
use crate::mail::outlook::OutlookProvider;
use crate::state::{AppState, PendingOAuth};

const OAUTH_STATE_TTL: Duration = Duration::from_secs(600);

#[derive(Serialize)]
pub struct AccountDto {
    id: i64,
    name: String,
    provider: String,
    settings: Value,
    /// Has a password (IMAP) or OAuth tokens.
    connected: bool,
    enabled: bool,
    dry_run: bool,
    poll_interval_secs: i64,
    last_polled_at: Option<String>,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<Account> for AccountDto {
    fn from(a: Account) -> Self {
        Self {
            id: a.id,
            name: a.name,
            provider: a.provider,
            settings: a.settings.0,
            connected: a.secret_enc.is_some(),
            enabled: a.enabled,
            dry_run: a.dry_run,
            poll_interval_secs: a.poll_interval_secs,
            last_polled_at: a.last_polled_at,
            last_error: a.last_error,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

#[derive(Deserialize)]
pub struct AccountInput {
    name: Option<String>,
    provider: Option<String>,
    settings: Option<Value>,
    /// IMAP only; write-only.
    password: Option<String>,
    enabled: Option<bool>,
    dry_run: Option<bool>,
    poll_interval_secs: Option<i64>,
}

fn validate_settings(provider: &str, settings: &Value) -> ApiResult<Value> {
    match provider {
        "imap" => {
            let s: ImapSettings = serde_json::from_value(settings.clone())
                .map_err(|e| bad_request(format!("invalid IMAP settings: {e}")))?;
            if s.host.trim().is_empty() || s.username.trim().is_empty() {
                return Err(bad_request("IMAP host and username are required"));
            }
            Ok(serde_json::to_value(s).map_err(anyhow::Error::from)?)
        }
        // OAuth accounts only carry what the server learns after connecting (the email address).
        "gmail" | "outlook" => Ok(json!({ "email": settings.get("email").cloned().unwrap_or(Value::Null) })),
        _ => Err(bad_request("provider must be gmail, outlook or imap")),
    }
}

async fn load(state: &AppState, id: i64) -> ApiResult<Account> {
    db::get_account(&state.db, id).await?.ok_or(ApiError::NotFound)
}

pub async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<AccountDto>>> {
    Ok(Json(db::list_accounts(&state.db).await?.into_iter().map(Into::into).collect()))
}

pub async fn get_one(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<AccountDto>> {
    Ok(Json(load(&state, id).await?.into()))
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<AccountInput>,
) -> ApiResult<(StatusCode, Json<AccountDto>)> {
    let provider = input.provider.ok_or_else(|| bad_request("provider is required"))?;
    let name = input
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| bad_request("name is required"))?;
    let settings = validate_settings(&provider, &input.settings.unwrap_or_else(|| json!({})))?;

    let secret = match (provider.as_str(), input.password.filter(|p| !p.is_empty())) {
        ("imap", Some(p)) => Some(crypto::seal(&state.cfg.secret_key, p.as_bytes())?),
        ("imap", None) => return Err(bad_request("IMAP password is required")),
        _ => None,
    };
    let interval = input.poll_interval_secs.unwrap_or(60);
    if interval < 15 {
        return Err(bad_request("poll interval must be at least 15 seconds"));
    }

    let now = db::now();
    let id = sqlx::query(
        "INSERT INTO accounts (name, provider, settings, secret_enc, enabled, dry_run, poll_interval_secs, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&name)
    .bind(&provider)
    .bind(SqlJson(settings))
    .bind(secret)
    .bind(input.enabled.unwrap_or(true))
    .bind(input.dry_run.unwrap_or(true))
    .bind(interval)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await?
    .last_insert_rowid();

    state.worker.reload();
    Ok((StatusCode::CREATED, Json(load(&state, id).await?.into())))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<AccountInput>,
) -> ApiResult<Json<AccountDto>> {
    let mut a = load(&state, id).await?;
    if input.provider.as_ref().is_some_and(|p| *p != a.provider) {
        return Err(bad_request("provider can't be changed; create a new account instead"));
    }
    if let Some(name) = input.name {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(bad_request("name is required"));
        }
        a.name = name;
    }
    let mut reset_cursor = false;
    if let Some(settings) = input.settings
        && a.provider == "imap"
    {
        let settings = validate_settings(&a.provider, &settings)?;
        // A different server or mailbox means the UID cursor is meaningless.
        reset_cursor = ["host", "port", "username", "mailbox"].iter().any(|k| settings.get(k) != a.settings.get(k));
        a.settings = SqlJson(settings);
    }
    if let Some(p) = input.password.filter(|p| !p.is_empty()) {
        if a.provider != "imap" {
            return Err(bad_request("only IMAP accounts have a password"));
        }
        a.secret_enc = Some(crypto::seal(&state.cfg.secret_key, p.as_bytes())?);
    }
    if let Some(v) = input.enabled {
        a.enabled = v;
    }
    if let Some(v) = input.dry_run {
        a.dry_run = v;
    }
    if let Some(v) = input.poll_interval_secs {
        if v < 15 {
            return Err(bad_request("poll interval must be at least 15 seconds"));
        }
        a.poll_interval_secs = v;
    }

    sqlx::query(
        "UPDATE accounts SET name = ?, settings = ?, secret_enc = ?, enabled = ?, dry_run = ?, poll_interval_secs = ?, \
         cursor = CASE WHEN ? THEN NULL ELSE cursor END, updated_at = ? WHERE id = ?",
    )
    .bind(&a.name)
    .bind(&a.settings)
    .bind(&a.secret_enc)
    .bind(a.enabled)
    .bind(a.dry_run)
    .bind(a.poll_interval_secs)
    .bind(reset_cursor)
    .bind(db::now())
    .bind(id)
    .execute(&state.db)
    .await?;

    state.worker.reload();
    Ok(Json(load(&state, id).await?.into()))
}

/// Forgets the account and its history; the mailbox itself is untouched.
pub async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let res = sqlx::query("DELETE FROM accounts WHERE id = ?").bind(id).execute(&state.db).await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    state.worker.reload();
    Ok(StatusCode::NO_CONTENT)
}

pub async fn test(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let account = load(&state, id).await?;
    let mut provider = mail::connect(&state, &account).await.map_err(|e| ApiError::Upstream(format!("{e:#}")))?;
    let result = provider.test().await;
    provider.close().await;
    let message = result.map_err(|e| ApiError::Upstream(format!("{e:#}")))?;
    Ok(Json(json!({ "ok": true, "message": message })))
}

pub async fn poll(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<(StatusCode, Json<Value>)> {
    load(&state, id).await?;
    if !state.worker.poll_now(id) {
        return Err(ApiError::Conflict("account is disabled".into()));
    }
    Ok((StatusCode::ACCEPTED, Json(json!({ "queued": true }))))
}

// ---------- OAuth ----------

#[derive(Deserialize)]
pub struct StartQuery {
    account_id: i64,
}

pub async fn oauth_start(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(q): Query<StartQuery>,
) -> ApiResult<Redirect> {
    let provider = OAuthProvider::parse(&provider).ok_or(ApiError::NotFound)?;
    let account = load(&state, q.account_id).await?;
    if OAuthProvider::for_account(&account.provider) != Some(provider) {
        return Err(bad_request("account provider doesn't match"));
    }

    let token = crypto::random_token();
    let url = oauth::authorize_url(&state.cfg, provider, &token)?;
    {
        let mut states = state.oauth_states.lock().unwrap();
        states.retain(|_, p| p.created.elapsed() < OAUTH_STATE_TTL);
        states.insert(token, PendingOAuth { account_id: account.id, provider, created: Instant::now() });
    }
    Ok(Redirect::to(&url))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

fn back_to_accounts(result: Result<String, String>) -> Redirect {
    let (status, message) = match result {
        Ok(m) => ("connected", m),
        Err(m) => ("error", m),
    };
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("oauth", status)
        .append_pair("message", &message)
        .finish();
    Redirect::to(&format!("/accounts?{query}"))
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(q): Query<CallbackQuery>,
) -> Redirect {
    back_to_accounts(finish_oauth(&state, &provider, q).await.map_err(|e| format!("{e:#}")))
}

async fn finish_oauth(state: &AppState, provider: &str, q: CallbackQuery) -> anyhow::Result<String> {
    if let Some(err) = q.error {
        anyhow::bail!("{}", q.error_description.unwrap_or(err));
    }
    let provider = OAuthProvider::parse(provider).ok_or_else(|| anyhow::anyhow!("unknown provider"))?;
    let pending = q
        .state
        .and_then(|s| state.oauth_states.lock().unwrap().remove(&s))
        .filter(|p| p.created.elapsed() < OAUTH_STATE_TTL && p.provider == provider)
        .ok_or_else(|| anyhow::anyhow!("sign-in link expired; try Connect again"))?;
    let code = q.code.ok_or_else(|| anyhow::anyhow!("no authorization code returned"))?;

    let tokens = oauth::exchange_code(&state.http, &state.cfg, provider, &code).await?;
    let session = OAuthSession::new(state, pending.account_id, provider, tokens);
    session.persist().await?;

    let email = match provider {
        OAuthProvider::Google => {
            GmailProvider::new(session, state.http.clone()).profile().await.map(|p| p.email_address)
        }
        OAuthProvider::Microsoft => OutlookProvider::new(session, state.http.clone())
            .me()
            .await
            .map(|m| m.mail.or(m.user_principal_name).unwrap_or_default()),
    }
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, "connected but could not read the account's address");
        String::new()
    });

    sqlx::query("UPDATE accounts SET settings = ?, last_error = NULL, updated_at = ? WHERE id = ?")
        .bind(SqlJson(json!({ "email": email })))
        .bind(db::now())
        .bind(pending.account_id)
        .execute(&state.db)
        .await?;
    state.worker.reload();
    Ok(if email.is_empty() { "Account connected".into() } else { format!("Connected {email}") })
}
