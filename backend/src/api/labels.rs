use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use super::{ApiError, ApiResult, bad_request, double_option};
use crate::db::{self, Label};
use crate::jev::MAX_OPTIONS;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LabelInput {
    name: Option<String>,
    description: Option<String>,
    color: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    threshold: Option<Option<f64>>,
    enabled: Option<bool>,
    sort: Option<i64>,
}

fn validate(label: &Label) -> ApiResult<()> {
    let name = label.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(bad_request("name must be 1–64 characters"));
    }
    if label.description.chars().count() > 1000 {
        return Err(bad_request("description must be at most 1000 characters"));
    }
    let c = label.color.as_bytes();
    if !(c.len() == 7 && c[0] == b'#' && c[1..].iter().all(u8::is_ascii_hexdigit)) {
        return Err(bad_request("color must look like #22c55e"));
    }
    if let Some(t) = label.threshold
        && !(0.0..=1.0).contains(&t)
    {
        return Err(bad_request("threshold must be between 0 and 1"));
    }
    Ok(())
}

fn merge(label: &mut Label, input: LabelInput) {
    if let Some(v) = input.name {
        label.name = v.trim().to_string();
    }
    if let Some(v) = input.description {
        label.description = v.trim().to_string();
    }
    if let Some(v) = input.color {
        label.color = v.to_lowercase();
    }
    if let Some(v) = input.threshold {
        label.threshold = v;
    }
    if let Some(v) = input.enabled {
        label.enabled = v;
    }
    if let Some(v) = input.sort {
        label.sort = v;
    }
}

pub async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<Label>>> {
    Ok(Json(db::list_labels(&state.db).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<LabelInput>,
) -> ApiResult<(StatusCode, Json<Label>)> {
    let existing = db::list_labels(&state.db).await?;
    if existing.iter().filter(|l| !l.is_system).count() >= MAX_OPTIONS {
        return Err(bad_request(format!("Jev supports at most {MAX_OPTIONS} labels")));
    }
    let next_sort = existing.iter().filter(|l| !l.is_system).map(|l| l.sort).max().unwrap_or(0) + 10;
    let now = db::now();
    let mut label = Label {
        id: 0,
        name: String::new(),
        description: String::new(),
        color: "#64748b".into(),
        threshold: None,
        enabled: true,
        is_system: false,
        sort: next_sort,
        created_at: now.clone(),
        updated_at: now,
    };
    merge(&mut label, input);
    validate(&label)?;

    let id = sqlx::query(
        "INSERT INTO labels (name, description, color, threshold, enabled, is_system, sort, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)",
    )
    .bind(&label.name)
    .bind(&label.description)
    .bind(&label.color)
    .bind(label.threshold)
    .bind(label.enabled)
    .bind(label.sort)
    .bind(&label.created_at)
    .bind(&label.updated_at)
    .execute(&state.db)
    .await?
    .last_insert_rowid();

    let label = db::get_label(&state.db, id).await?.ok_or(ApiError::NotFound)?;
    Ok((StatusCode::CREATED, Json(label)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<LabelInput>,
) -> ApiResult<Json<Label>> {
    let mut label = db::get_label(&state.db, id).await?.ok_or(ApiError::NotFound)?;
    if label.is_system && (input.enabled == Some(false) || matches!(input.threshold, Some(Some(_)))) {
        return Err(bad_request("the Review label is always enabled and has no threshold"));
    }
    merge(&mut label, input);
    validate(&label)?;

    sqlx::query(
        "UPDATE labels SET name = ?, description = ?, color = ?, threshold = ?, enabled = ?, sort = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&label.name)
    .bind(&label.description)
    .bind(&label.color)
    .bind(label.threshold)
    .bind(label.enabled)
    .bind(label.sort)
    .bind(db::now())
    .bind(id)
    .execute(&state.db)
    .await?;

    Ok(Json(db::get_label(&state.db, id).await?.ok_or(ApiError::NotFound)?))
}

/// Removes the label from the app only; labels already in mailboxes are left alone.
pub async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let label = db::get_label(&state.db, id).await?.ok_or(ApiError::NotFound)?;
    if label.is_system {
        return Err(bad_request("the Review label can't be deleted"));
    }
    sqlx::query("DELETE FROM labels WHERE id = ?").bind(id).execute(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}
