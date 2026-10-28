use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::types::Json as SqlJson;

use super::{ApiError, ApiResult, bad_request, double_option};
use crate::db::{self, Rule};
use crate::decision::{MIN_ACTION_PROBABILITY, RuleAction};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct RuleInput {
    label_id: Option<i64>,
    action: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    folder: Option<Option<String>>,
    min_probability: Option<f64>,
    enabled: Option<bool>,
}

async fn validate(state: &AppState, rule: &Rule) -> ApiResult<()> {
    let label = db::get_label(&state.db, rule.label_id).await?.ok_or_else(|| bad_request("label not found"))?;
    if label.is_system {
        return Err(bad_request("rules can't be attached to the Review label"));
    }
    let action = RuleAction::parse(&rule.action).ok_or_else(|| bad_request("unknown action"))?;
    if action == RuleAction::MoveToFolder
        && rule.params.get("folder").and_then(Value::as_str).is_none_or(|f| f.trim().is_empty())
    {
        return Err(bad_request("move_to_folder needs a folder"));
    }
    if !(MIN_ACTION_PROBABILITY..=1.0).contains(&rule.min_probability) {
        return Err(bad_request(format!("min_probability must be between {MIN_ACTION_PROBABILITY} and 1")));
    }
    Ok(())
}

fn merge(rule: &mut Rule, input: RuleInput) {
    if let Some(v) = input.label_id {
        rule.label_id = v;
    }
    if let Some(v) = input.action {
        rule.action = v;
    }
    if let Some(folder) = input.folder {
        rule.params = SqlJson(match folder.map(|f| f.trim().to_string()).filter(|f| !f.is_empty()) {
            Some(f) => json!({ "folder": f }),
            None => json!({}),
        });
    }
    if let Some(v) = input.min_probability {
        rule.min_probability = v;
    }
    if let Some(v) = input.enabled {
        rule.enabled = v;
    }
}

pub async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<Rule>>> {
    Ok(Json(db::list_rules(&state.db).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<RuleInput>,
) -> ApiResult<(StatusCode, Json<Rule>)> {
    let now = db::now();
    let mut rule = Rule {
        id: 0,
        label_id: input.label_id.ok_or_else(|| bad_request("label_id is required"))?,
        action: String::new(),
        params: SqlJson(json!({})),
        min_probability: MIN_ACTION_PROBABILITY,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    };
    merge(&mut rule, input);
    validate(&state, &rule).await?;

    let id = sqlx::query(
        "INSERT INTO rules (label_id, action, params, min_probability, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(rule.label_id)
    .bind(&rule.action)
    .bind(&rule.params)
    .bind(rule.min_probability)
    .bind(rule.enabled)
    .bind(&rule.created_at)
    .bind(&rule.updated_at)
    .execute(&state.db)
    .await?
    .last_insert_rowid();

    Ok((StatusCode::CREATED, Json(db::get_rule(&state.db, id).await?.ok_or(ApiError::NotFound)?)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<RuleInput>,
) -> ApiResult<Json<Rule>> {
    let mut rule = db::get_rule(&state.db, id).await?.ok_or(ApiError::NotFound)?;
    merge(&mut rule, input);
    validate(&state, &rule).await?;

    sqlx::query("UPDATE rules SET label_id = ?, action = ?, params = ?, min_probability = ?, enabled = ?, updated_at = ? WHERE id = ?")
        .bind(rule.label_id)
        .bind(&rule.action)
        .bind(&rule.params)
        .bind(rule.min_probability)
        .bind(rule.enabled)
        .bind(db::now())
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(db::get_rule(&state.db, id).await?.ok_or(ApiError::NotFound)?))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let res = sqlx::query("DELETE FROM rules WHERE id = ?").bind(id).execute(&state.db).await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
