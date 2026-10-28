use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::types::Json;
use sqlx::{FromRow, SqlitePool};

use crate::config::Config;

pub async fn connect(cfg: &Config) -> Result<SqlitePool> {
    let opts = match &cfg.database_url {
        Some(url) => SqliteConnectOptions::from_str(url)?,
        None => SqliteConnectOptions::new().filename(cfg.data_dir.join("app.db")),
    }
    .create_if_missing(true)
    .foreign_keys(true)
    .journal_mode(SqliteJournalMode::Wal)
    .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(opts).await.context("opening database")?;
    sqlx::migrate!("./migrations").run(&pool).await.context("running migrations")?;
    Ok(pool)
}

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Label {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub color: String,
    pub threshold: Option<f64>,
    pub enabled: bool,
    pub is_system: bool,
    pub sort: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Rule {
    pub id: i64,
    pub label_id: i64,
    pub action: String,
    pub params: Json<Value>,
    pub min_probability: f64,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub provider: String,
    pub settings: Json<Value>,
    pub secret_enc: Option<Vec<u8>>,
    pub enabled: bool,
    pub dry_run: bool,
    pub poll_interval_secs: i64,
    pub cursor: Option<String>,
    pub last_polled_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Classification {
    pub id: i64,
    pub account_id: i64,
    pub provider_msg_id: String,
    pub thread_id: Option<String>,
    pub sender: String,
    pub subject: String,
    pub snippet: String,
    pub received_at: Option<String>,
    pub label_id: Option<i64>,
    pub applied_label: Option<String>,
    pub top_label: Option<String>,
    pub top_probability: Option<f64>,
    pub confidence: Option<f64>,
    pub probabilities: Json<Value>,
    pub status: String,
    pub actions_taken: Json<Value>,
    pub error: Option<String>,
    pub model: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    // Joined for display.
    pub label_name: Option<String>,
    pub label_color: Option<String>,
    pub account_name: Option<String>,
}

pub const CLASSIFICATION_SELECT: &str = "SELECT c.*, l.name AS label_name, l.color AS label_color, a.name AS account_name \
     FROM classifications c \
     LEFT JOIN labels l ON l.id = c.label_id \
     LEFT JOIN accounts a ON a.id = c.account_id";

// ---------- labels & rules ----------

pub async fn list_labels(db: &SqlitePool) -> Result<Vec<Label>> {
    Ok(sqlx::query_as("SELECT * FROM labels ORDER BY sort, name COLLATE NOCASE").fetch_all(db).await?)
}

pub async fn get_label(db: &SqlitePool, id: i64) -> Result<Option<Label>> {
    Ok(sqlx::query_as("SELECT * FROM labels WHERE id = ?").bind(id).fetch_optional(db).await?)
}

pub async fn list_rules(db: &SqlitePool) -> Result<Vec<Rule>> {
    Ok(sqlx::query_as("SELECT * FROM rules ORDER BY label_id, id").fetch_all(db).await?)
}

pub async fn get_rule(db: &SqlitePool, id: i64) -> Result<Option<Rule>> {
    Ok(sqlx::query_as("SELECT * FROM rules WHERE id = ?").bind(id).fetch_optional(db).await?)
}

// ---------- accounts ----------

pub async fn list_accounts(db: &SqlitePool) -> Result<Vec<Account>> {
    Ok(sqlx::query_as("SELECT * FROM accounts ORDER BY name COLLATE NOCASE").fetch_all(db).await?)
}

pub async fn get_account(db: &SqlitePool, id: i64) -> Result<Option<Account>> {
    Ok(sqlx::query_as("SELECT * FROM accounts WHERE id = ?").bind(id).fetch_optional(db).await?)
}

pub async fn mark_polled(db: &SqlitePool, id: i64, error: Option<&str>) -> Result<()> {
    sqlx::query("UPDATE accounts SET last_polled_at = ?, last_error = ? WHERE id = ?")
        .bind(now())
        .bind(error)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn save_cursor(db: &SqlitePool, id: i64, cursor: &str) -> Result<()> {
    sqlx::query("UPDATE accounts SET cursor = ? WHERE id = ?").bind(cursor).bind(id).execute(db).await?;
    Ok(())
}

pub async fn save_secret(db: &SqlitePool, id: i64, sealed: &[u8]) -> Result<()> {
    sqlx::query("UPDATE accounts SET secret_enc = ?, updated_at = ? WHERE id = ?")
        .bind(sealed)
        .bind(now())
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

// ---------- classifications ----------

pub async fn is_processed(db: &SqlitePool, account_id: i64, msg_id: &str) -> Result<bool> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM classifications WHERE account_id = ? AND provider_msg_id = ?")
            .bind(account_id)
            .bind(msg_id)
            .fetch_optional(db)
            .await?;
    Ok(row.is_some())
}

pub async fn get_classification(db: &SqlitePool, id: i64) -> Result<Option<Classification>> {
    Ok(sqlx::query_as(&format!("{CLASSIFICATION_SELECT} WHERE c.id = ?")).bind(id).fetch_optional(db).await?)
}

#[derive(Debug, Default)]
pub struct NewClassification {
    pub account_id: i64,
    pub provider_msg_id: String,
    pub thread_id: Option<String>,
    pub sender: String,
    pub subject: String,
    pub snippet: String,
    pub received_at: Option<String>,
    pub label_id: Option<i64>,
    pub applied_label: Option<String>,
    pub top_label: Option<String>,
    pub top_probability: Option<f64>,
    pub confidence: Option<f64>,
    pub probabilities: Value,
    pub status: &'static str,
    pub actions_taken: Value,
    pub error: Option<String>,
    pub model: Option<String>,
}

pub async fn insert_classification(db: &SqlitePool, c: NewClassification) -> Result<()> {
    let ts = now();
    sqlx::query(
        "INSERT INTO classifications (account_id, provider_msg_id, thread_id, sender, subject, snippet, received_at, \
         label_id, applied_label, top_label, top_probability, confidence, probabilities, status, actions_taken, error, model, \
         created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (account_id, provider_msg_id) DO NOTHING",
    )
    .bind(c.account_id)
    .bind(c.provider_msg_id)
    .bind(c.thread_id)
    .bind(c.sender)
    .bind(c.subject)
    .bind(c.snippet)
    .bind(c.received_at)
    .bind(c.label_id)
    .bind(c.applied_label)
    .bind(c.top_label)
    .bind(c.top_probability)
    .bind(c.confidence)
    .bind(Json(c.probabilities))
    .bind(c.status)
    .bind(Json(c.actions_taken))
    .bind(c.error)
    .bind(c.model)
    .bind(&ts)
    .bind(&ts)
    .execute(db)
    .await?;
    Ok(())
}

// ---------- settings ----------

#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    pub threshold: f64,
    pub body_chars: usize,
    pub dry_run: bool,
    pub max_per_poll: usize,
    pub instructions: String,
}

pub const DEFAULT_INSTRUCTIONS: &str = "Which label best describes this email from the recipient's point of view?";

pub async fn seed_settings(db: &SqlitePool, cfg: &Config) -> Result<()> {
    let defaults = [
        ("threshold", cfg.default_threshold.to_string()),
        ("body_chars", cfg.body_chars.to_string()),
        ("dry_run", cfg.dry_run.to_string()),
        ("max_per_poll", "50".to_string()),
        ("instructions", DEFAULT_INSTRUCTIONS.to_string()),
    ];
    for (key, value) in defaults {
        sqlx::query("INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(db)
            .await?;
    }
    Ok(())
}

pub async fn load_settings(db: &SqlitePool) -> Result<Settings> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings").fetch_all(db).await?;
    let map: HashMap<String, String> = rows.into_iter().collect();
    let get = |k: &str| map.get(k).map(String::as_str);
    Ok(Settings {
        threshold: get("threshold").and_then(|v| v.parse().ok()).unwrap_or(0.7),
        body_chars: get("body_chars").and_then(|v| v.parse().ok()).unwrap_or(2000),
        dry_run: get("dry_run").map(|v| v != "false").unwrap_or(true),
        max_per_poll: get("max_per_poll").and_then(|v| v.parse().ok()).unwrap_or(50),
        instructions: get("instructions").unwrap_or(DEFAULT_INSTRUCTIONS).to_string(),
    })
}

pub async fn save_setting(db: &SqlitePool, key: &str, value: String) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(db)
    .await?;
    Ok(())
}
