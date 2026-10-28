use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{ApiError, ApiResult, bad_request};
use crate::db::{self, CLASSIFICATION_SELECT, Classification, Settings};
use crate::pipeline;
use crate::state::AppState;

pub async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

#[derive(Serialize)]
pub struct SettingsDto {
    #[serde(flatten)]
    settings: Settings,
    jev_configured: bool,
    jev_model: String,
    google_configured: bool,
    microsoft_configured: bool,
    public_url: String,
}

pub async fn get_settings(State(state): State<AppState>) -> ApiResult<Json<SettingsDto>> {
    Ok(Json(SettingsDto {
        settings: db::load_settings(&state.db).await?,
        jev_configured: state.cfg.jev_api_key.is_some(),
        jev_model: state.cfg.jev_model.clone(),
        google_configured: state.cfg.google.is_some(),
        microsoft_configured: state.cfg.microsoft.is_some(),
        public_url: state.cfg.public_url.clone(),
    }))
}

#[derive(Deserialize)]
pub struct SettingsInput {
    threshold: Option<f64>,
    body_chars: Option<usize>,
    dry_run: Option<bool>,
    max_per_poll: Option<usize>,
    instructions: Option<String>,
}

pub async fn put_settings(
    State(state): State<AppState>,
    Json(input): Json<SettingsInput>,
) -> ApiResult<Json<SettingsDto>> {
    if let Some(t) = input.threshold {
        if !(0.0..=1.0).contains(&t) {
            return Err(bad_request("threshold must be between 0 and 1"));
        }
        db::save_setting(&state.db, "threshold", t.to_string()).await?;
    }
    if let Some(n) = input.body_chars {
        if !(100..=20_000).contains(&n) {
            return Err(bad_request("body_chars must be between 100 and 20000"));
        }
        db::save_setting(&state.db, "body_chars", n.to_string()).await?;
    }
    if let Some(d) = input.dry_run {
        db::save_setting(&state.db, "dry_run", d.to_string()).await?;
    }
    if let Some(n) = input.max_per_poll {
        if !(1..=500).contains(&n) {
            return Err(bad_request("max_per_poll must be between 1 and 500"));
        }
        db::save_setting(&state.db, "max_per_poll", n.to_string()).await?;
    }
    if let Some(i) = input.instructions {
        let i = i.trim();
        let i = if i.is_empty() { db::DEFAULT_INSTRUCTIONS } else { i };
        db::save_setting(&state.db, "instructions", i.to_string()).await?;
    }
    get_settings(State(state)).await
}

#[derive(Serialize, sqlx::FromRow)]
struct LabelCount {
    id: i64,
    name: String,
    color: String,
    is_system: bool,
    count: i64,
}

pub async fn stats(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let by_status: Vec<(String, i64)> =
        sqlx::query_as("SELECT status, COUNT(*) FROM classifications GROUP BY status").fetch_all(&state.db).await?;
    let by_label: Vec<LabelCount> = sqlx::query_as(
        "SELECT l.id, l.name, l.color, l.is_system, COUNT(c.id) AS count FROM labels l \
         LEFT JOIN classifications c ON c.label_id = l.id GROUP BY l.id ORDER BY l.sort, l.name",
    )
    .fetch_all(&state.db)
    .await?;
    let since = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let (last_24h,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM classifications WHERE created_at >= ?")
        .bind(since)
        .fetch_one(&state.db)
        .await?;
    let recent: Vec<Classification> =
        sqlx::query_as(&format!("{CLASSIFICATION_SELECT} ORDER BY c.created_at DESC, c.id DESC LIMIT 8"))
            .fetch_all(&state.db)
            .await?;

    let total: i64 = by_status.iter().map(|(_, n)| n).sum();
    let status_map: serde_json::Map<String, Value> = by_status.into_iter().map(|(s, n)| (s, json!(n))).collect();
    Ok(Json(json!({
        "total": total,
        "last_24h": last_24h,
        "by_status": status_map,
        "by_label": by_label,
        "recent": recent,
    })))
}

#[derive(Deserialize)]
pub struct PlaygroundInput {
    #[serde(default)]
    sender: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    body: String,
}

/// Classifies arbitrary text against the current labels without touching any mailbox.
pub async fn playground(
    State(state): State<AppState>,
    Json(input): Json<PlaygroundInput>,
) -> ApiResult<Json<pipeline::Classified>> {
    if input.subject.trim().is_empty() && input.body.trim().is_empty() {
        return Err(bad_request("enter a subject or body"));
    }
    let ctx = pipeline::load_context(&state.db).await?;
    let classified = pipeline::classify(&state, &ctx, &input.sender, &input.subject, &input.body)
        .await
        .map_err(|e| ApiError::Upstream(format!("{e:#}")))?;
    Ok(Json(classified))
}
