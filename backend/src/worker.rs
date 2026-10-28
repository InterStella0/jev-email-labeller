//! Background polling: one task per enabled account, managed by a supervisor.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use serde_json::json;
use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;

use crate::config;
use crate::db::{self, Account, NewClassification};
use crate::mail::{self, MailProvider, MsgRef};
use crate::pipeline::{self, Context};
use crate::state::AppState;

const HEARTBEAT_FILE: &str = "heartbeat";
const HEARTBEAT_EVERY: Duration = Duration::from_secs(30);
const HEARTBEAT_MAX_AGE: Duration = Duration::from_secs(180);
/// How many candidate ids to ask a provider for in one poll.
const LIST_LIMIT: usize = 500;

struct AccountTask {
    handle: JoinHandle<()>,
    stop: Arc<AtomicBool>,
    wake: Arc<Notify>,
}

#[derive(Clone)]
pub struct WorkerHandle {
    reload: Arc<watch::Sender<u64>>,
    wakers: Arc<Mutex<HashMap<i64, Arc<Notify>>>>,
}

impl WorkerHandle {
    pub fn new() -> (Self, watch::Receiver<u64>) {
        let (tx, rx) = watch::channel(0);
        (Self { reload: Arc::new(tx), wakers: Arc::default() }, rx)
    }

    /// Re-reads the account list; call after any account change.
    pub fn reload(&self) {
        self.reload.send_modify(|v| *v += 1);
    }

    /// Wakes an account's task for an immediate poll. Returns false if it isn't running.
    pub fn poll_now(&self, account_id: i64) -> bool {
        match self.wakers.lock().unwrap().get(&account_id) {
            Some(n) => {
                n.notify_one();
                true
            }
            None => false,
        }
    }
}

pub fn touch_heartbeat(data_dir: &Path) {
    if let Err(e) = std::fs::write(data_dir.join(HEARTBEAT_FILE), db::now()) {
        tracing::warn!(error = %e, "could not write heartbeat");
    }
}

/// Exit code for the `healthcheck` subcommand: 0 if the heartbeat is fresh.
pub fn healthcheck() -> i32 {
    let path = config::data_dir().join(HEARTBEAT_FILE);
    let fresh = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < HEARTBEAT_MAX_AGE);
    if fresh { 0 } else { 1 }
}

pub async fn supervise(app: AppState, mut reload: watch::Receiver<u64>) {
    let mut tasks: HashMap<i64, AccountTask> = HashMap::new();
    let mut heartbeat = tokio::time::interval(HEARTBEAT_EVERY);

    loop {
        sync_tasks(&app, &mut tasks).await;
        loop {
            tokio::select! {
                changed = reload.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    break;
                }
                _ = heartbeat.tick() => touch_heartbeat(&app.cfg.data_dir),
            }
        }
    }
}

async fn sync_tasks(app: &AppState, tasks: &mut HashMap<i64, AccountTask>) {
    let wanted: Vec<i64> = match db::list_accounts(&app.db).await {
        Ok(accounts) => accounts.into_iter().filter(|a| a.enabled).map(|a| a.id).collect(),
        Err(e) => {
            tracing::error!(error = %e, "listing accounts failed");
            return;
        }
    };

    tasks.retain(|id, task| {
        let keep = wanted.contains(id) && !task.handle.is_finished();
        if !keep {
            // Let the task finish its current message instead of aborting mid-write.
            task.stop.store(true, Ordering::SeqCst);
            task.wake.notify_one();
        }
        keep
    });

    for id in wanted {
        tasks.entry(id).or_insert_with(|| {
            let stop = Arc::new(AtomicBool::new(false));
            let wake = Arc::new(Notify::new());
            let handle = tokio::spawn(run_account(app.clone(), id, stop.clone(), wake.clone()));
            AccountTask { handle, stop, wake }
        });
    }

    let mut wakers = app.worker.wakers.lock().unwrap();
    wakers.clear();
    wakers.extend(tasks.iter().map(|(id, t)| (*id, t.wake.clone())));
}

async fn run_account(app: AppState, id: i64, stop: Arc<AtomicBool>, wake: Arc<Notify>) {
    tracing::info!(account = id, "worker started");
    while !stop.load(Ordering::SeqCst) {
        let account = match db::get_account(&app.db, id).await {
            Ok(Some(a)) if a.enabled => a,
            Ok(_) => break,
            Err(e) => {
                tracing::error!(account = id, error = %e, "loading account failed");
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }
        };

        let error = match poll_account(&app, &account, &stop).await {
            Ok(n) => {
                if n > 0 {
                    tracing::info!(account = id, processed = n, "poll finished");
                }
                None
            }
            Err(e) => {
                tracing::warn!(account = id, error = format!("{e:#}"), "poll failed");
                Some(format!("{e:#}"))
            }
        };
        if let Err(e) = db::mark_polled(&app.db, id, error.as_deref()).await {
            tracing::error!(account = id, error = %e, "saving poll status failed");
        }
        touch_heartbeat(&app.cfg.data_dir);

        let wait = Duration::from_secs(account.poll_interval_secs.max(15) as u64);
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = wake.notified() => {}
        }
    }
    tracing::info!(account = id, "worker stopped");
}

/// One poll: list, skip processed, classify and act on the rest. Returns how many were processed.
pub async fn poll_account(app: &AppState, account: &Account, stop: &AtomicBool) -> Result<usize> {
    let ctx = pipeline::load_context(&app.db).await?;
    let mut provider = mail::connect(app, account).await?;
    let result = poll_with(app, &ctx, account, provider.as_mut(), stop).await;
    provider.close().await;
    result
}

async fn poll_with(
    app: &AppState,
    ctx: &Context,
    account: &Account,
    provider: &mut dyn MailProvider,
    stop: &AtomicBool,
) -> Result<usize> {
    let (refs, next_cursor) = provider.list_new(account.cursor.as_deref(), LIST_LIMIT).await?;

    let mut todo: Vec<MsgRef> = Vec::new();
    for r in refs {
        if !db::is_processed(&app.db, account.id, &r.id).await? {
            todo.push(r);
        }
    }
    let complete = todo.len() <= ctx.settings.max_per_poll;

    let mut processed = 0;
    for r in todo.into_iter().take(ctx.settings.max_per_poll) {
        if stop.load(Ordering::SeqCst) {
            return Ok(processed);
        }
        process_one(app, ctx, account, provider, &r).await?;
        processed += 1;
    }

    // Only advance once everything up to the cursor has been handled; the DB dedups the rest.
    if complete && let Some(cursor) = next_cursor {
        db::save_cursor(&app.db, account.id, &cursor).await?;
    }
    Ok(processed)
}

async fn process_one(
    app: &AppState,
    ctx: &Context,
    account: &Account,
    provider: &mut dyn MailProvider,
    r: &MsgRef,
) -> Result<()> {
    // Fetch and Jev errors abort the poll (likely transient); the message is retried next time.
    let msg = provider.fetch(&r.id).await?;
    let classified = pipeline::classify(app, ctx, &msg.sender, &msg.subject, &msg.body).await?;
    let mut row: NewClassification = pipeline::record(account.id, &msg, &classified);

    let dry_run = ctx.settings.dry_run || account.dry_run;
    if !dry_run {
        let exec = pipeline::execute(provider, &msg.id, &classified.outcome).await;
        row.status = exec.status(&classified.outcome);
        row.applied_label = exec.label_applied.then(|| classified.outcome.label_name.clone());
        row.actions_taken = json!(exec.actions);
        row.error = exec.error;
    }

    tracing::debug!(
        account = account.id,
        msg = %msg.id,
        label = %classified.outcome.label_name,
        p = classified.outcome.top_probability,
        status = row.status,
        "classified"
    );
    db::insert_classification(&app.db, row).await
}
