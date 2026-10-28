use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::types::Json as SqlJson;
use sqlx::{QueryBuilder, Sqlite};

use super::{ApiError, ApiResult, bad_request};
use crate::db::{self, CLASSIFICATION_SELECT, Classification};
use crate::decision;
use crate::mail::{self, MailProvider};
use crate::pipeline::{self, Execution};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    account_id: Option<i64>,
    label_id: Option<i64>,
    status: Option<String>,
    q: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Serialize)]
pub struct Page {
    items: Vec<Classification>,
    total: i64,
    page: i64,
    per_page: i64,
}

fn push_filters(qb: &mut QueryBuilder<'_, Sqlite>, q: &ListQuery) {
    if let Some(id) = q.account_id {
        qb.push(" AND c.account_id = ").push_bind(id);
    }
    if let Some(id) = q.label_id {
        qb.push(" AND c.label_id = ").push_bind(id);
    }
    if let Some(status) = q.status.as_deref().filter(|s| !s.is_empty()) {
        qb.push(" AND c.status = ").push_bind(status.to_string());
    }
    if let Some(text) = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let like = format!("%{}%", text.replace('%', "\\%").replace('_', "\\_"));
        qb.push(" AND (c.subject LIKE ")
            .push_bind(like.clone())
            .push(" ESCAPE '\\' OR c.sender LIKE ")
            .push_bind(like.clone())
            .push(" ESCAPE '\\' OR c.snippet LIKE ")
            .push_bind(like)
            .push(" ESCAPE '\\')");
    }
}

pub async fn list(State(state): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Json<Page>> {
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let page = q.page.unwrap_or(1).max(1);

    let mut count = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM classifications c WHERE 1 = 1");
    push_filters(&mut count, &q);
    let (total,): (i64,) = count.build_query_as().fetch_one(&state.db).await?;

    let mut select = QueryBuilder::<Sqlite>::new(format!("{CLASSIFICATION_SELECT} WHERE 1 = 1"));
    push_filters(&mut select, &q);
    select
        .push(" ORDER BY c.created_at DESC, c.id DESC LIMIT ")
        .push_bind(per_page)
        .push(" OFFSET ")
        .push_bind((page - 1) * per_page);
    let items = select.build_query_as().fetch_all(&state.db).await?;

    Ok(Json(Page { items, total, page, per_page }))
}

async fn load(state: &AppState, id: i64) -> ApiResult<Classification> {
    db::get_classification(&state.db, id).await?.ok_or(ApiError::NotFound)
}

pub async fn get_one(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Classification>> {
    Ok(Json(load(&state, id).await?))
}

async fn provider_for(state: &AppState, row: &Classification) -> ApiResult<Box<dyn MailProvider>> {
    let account = db::get_account(&state.db, row.account_id).await?.ok_or(ApiError::NotFound)?;
    mail::connect(state, &account).await.map_err(|e| ApiError::Upstream(format!("{e:#}")))
}

#[derive(Deserialize)]
pub struct OverrideInput {
    label_id: i64,
    /// Also swap the label in the mailbox (default true).
    apply_to_mailbox: Option<bool>,
}

/// Manual correction. Explicit user action, so it touches the mailbox even in dry-run mode.
pub async fn override_label(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<OverrideInput>,
) -> ApiResult<Json<Classification>> {
    let row = load(&state, id).await?;
    let label = db::get_label(&state.db, input.label_id).await?.ok_or_else(|| bad_request("label not found"))?;

    let mut applied = row.applied_label.clone();
    if input.apply_to_mailbox.unwrap_or(true) {
        let mut provider = provider_for(&state, &row).await?;
        let result = async {
            if let Some(old) = &row.applied_label
                && !old.eq_ignore_ascii_case(&label.name)
            {
                provider.remove_label(&row.provider_msg_id, old).await?;
            }
            provider.apply_label(&row.provider_msg_id, &label.name).await
        }
        .await;
        provider.close().await;
        result.map_err(|e| ApiError::Upstream(format!("{e:#}")))?;
        applied = Some(label.name.clone());
    }

    sqlx::query("UPDATE classifications SET label_id = ?, applied_label = ?, status = 'overridden', error = NULL, updated_at = ? WHERE id = ?")
        .bind(label.id)
        .bind(applied)
        .bind(db::now())
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(load(&state, id).await?))
}

async fn save_execution(
    state: &AppState,
    id: i64,
    label_id: i64,
    exec: &Execution,
    status: &str,
    label_name: &str,
) -> ApiResult<()> {
    sqlx::query("UPDATE classifications SET label_id = ?, status = ?, applied_label = ?, actions_taken = ?, error = ?, updated_at = ? WHERE id = ?")
        .bind(label_id)
        .bind(status)
        .bind(exec.label_applied.then_some(label_name))
        .bind(SqlJson(json!(exec.actions)))
        .bind(&exec.error)
        .bind(db::now())
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Commits a dry-run (or failed) decision, re-evaluated against the current labels, thresholds and rules.
pub async fn apply(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Classification>> {
    let row = load(&state, id).await?;
    if !matches!(row.status.as_str(), "dry_run" | "error") {
        return Err(bad_request("only dry-run or failed decisions can be applied"));
    }
    let probs: BTreeMap<String, f64> = serde_json::from_value(row.probabilities.0.clone()).unwrap_or_default();
    let ctx = pipeline::load_context(&state.db).await?;
    let outcome = decision::decide(&probs, &ctx.labels, &ctx.review, ctx.settings.threshold, &ctx.rules);

    let mut provider = provider_for(&state, &row).await?;
    let exec = pipeline::execute(provider.as_mut(), &row.provider_msg_id, &outcome).await;
    provider.close().await;

    save_execution(&state, id, outcome.label_id, &exec, exec.status(&outcome), &outcome.label_name).await?;
    Ok(Json(load(&state, id).await?))
}

/// Fetches the message again and asks Jev again. Follows the same dry-run rules as the worker.
pub async fn reclassify(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Classification>> {
    let row = load(&state, id).await?;
    let account = db::get_account(&state.db, row.account_id).await?.ok_or(ApiError::NotFound)?;
    let ctx = pipeline::load_context(&state.db).await?;
    let mut provider = provider_for(&state, &row).await?;

    let result = async {
        let msg = provider.fetch(&row.provider_msg_id).await?;
        let classified = pipeline::classify(&state, &ctx, &msg.sender, &msg.subject, &msg.body).await?;
        let mut rec = pipeline::record(row.account_id, &msg, &classified);
        rec.applied_label = row.applied_label.clone();

        if !(ctx.settings.dry_run || account.dry_run) {
            if let Some(old) = &row.applied_label
                && !old.eq_ignore_ascii_case(&classified.outcome.label_name)
            {
                provider.remove_label(&msg.id, old).await?;
            }
            let exec = pipeline::execute(provider.as_mut(), &msg.id, &classified.outcome).await;
            rec.status = exec.status(&classified.outcome);
            rec.applied_label = if exec.label_applied { Some(classified.outcome.label_name.clone()) } else { None };
            rec.actions_taken = json!(exec.actions);
            rec.error = exec.error;
        }
        anyhow::Ok(rec)
    }
    .await;
    provider.close().await;
    let rec = result.map_err(|e| ApiError::Upstream(format!("{e:#}")))?;

    sqlx::query(
        "UPDATE classifications SET sender = ?, subject = ?, snippet = ?, label_id = ?, applied_label = ?, top_label = ?, \
         top_probability = ?, confidence = ?, probabilities = ?, status = ?, actions_taken = ?, error = ?, model = ?, updated_at = ? \
         WHERE id = ?",
    )
    .bind(rec.sender)
    .bind(rec.subject)
    .bind(rec.snippet)
    .bind(rec.label_id)
    .bind(rec.applied_label)
    .bind(rec.top_label)
    .bind(rec.top_probability)
    .bind(rec.confidence)
    .bind(SqlJson(rec.probabilities))
    .bind(rec.status)
    .bind(SqlJson(rec.actions_taken))
    .bind(rec.error)
    .bind(rec.model)
    .bind(db::now())
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(Json(load(&state, id).await?))
}

/// Forgets the record. The mailbox is not changed.
pub async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let res = sqlx::query("DELETE FROM classifications WHERE id = ?").bind(id).execute(&state.db).await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
