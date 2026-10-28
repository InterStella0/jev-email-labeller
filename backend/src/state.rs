use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use sqlx::SqlitePool;

use crate::config::Config;
use crate::jev::JevClient;
use crate::mail::oauth::OAuthProvider;
use crate::worker::WorkerHandle;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub cfg: Arc<Config>,
    pub http: reqwest::Client,
    pub jev: Arc<dyn JevClient>,
    pub worker: WorkerHandle,
    pub oauth_states: Arc<Mutex<HashMap<String, PendingOAuth>>>,
    pub login_guard: Arc<Mutex<LoginGuard>>,
}

#[derive(Debug, Clone)]
pub struct PendingOAuth {
    pub account_id: i64,
    pub provider: OAuthProvider,
    pub created: Instant,
}

#[derive(Debug, Default)]
pub struct LoginGuard {
    pub failures: u32,
    pub locked_until: Option<Instant>,
}
