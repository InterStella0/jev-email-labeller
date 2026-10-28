mod api;
mod config;
mod crypto;
mod db;
mod decision;
mod jev;
mod mail;
mod pipeline;
mod state;
mod worker;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

use crate::jev::{JevClient, TypeSafeClient};
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        std::process::exit(worker::healthcheck());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,jev_labeller=debug")),
        )
        .init();

    let cfg = Arc::new(config::Config::from_env()?);
    std::fs::create_dir_all(&cfg.data_dir).with_context(|| format!("creating {}", cfg.data_dir.display()))?;

    let db = db::connect(&cfg).await?;
    db::seed_settings(&db, &cfg).await?;

    let http = reqwest::Client::builder()
        .user_agent(concat!("jev-labeller/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .build()?;
    let jev: Arc<dyn JevClient> =
        Arc::new(TypeSafeClient::new(http.clone(), &cfg.jev_base_url, cfg.jev_api_key.clone(), &cfg.jev_model));
    if cfg.jev_api_key.is_none() {
        tracing::warn!("JEV_API_KEY is not set; classification will fail until it is");
    }

    let (worker, reload_rx) = worker::WorkerHandle::new();
    let state = AppState {
        db,
        cfg: cfg.clone(),
        http,
        jev,
        worker,
        oauth_states: Default::default(),
        login_guard: Default::default(),
    };
    worker::touch_heartbeat(&cfg.data_dir);
    tokio::spawn(worker::supervise(state.clone(), reload_rx));

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await.with_context(|| format!("binding {}", cfg.bind))?;
    tracing::info!("listening on {}", cfg.bind);
    axum::serve(listener, api::router(state)).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
