//! Shared steps used by the worker and the API: load config, classify, execute.

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::db::{self, NewClassification, Settings};
use crate::decision::{self, LabelInfo, Outcome, RuleAction, RuleInfo};
use crate::jev::{self, Decision, LabelCriterion};
use crate::mail::{MailProvider, Message};
use crate::state::AppState;

pub struct Context {
    pub settings: Settings,
    pub labels: Vec<LabelInfo>,
    pub criteria: Vec<LabelCriterion>,
    pub review: LabelInfo,
    pub rules: Vec<RuleInfo>,
}

pub async fn load_context(db: &SqlitePool) -> Result<Context> {
    let settings = db::load_settings(db).await?;
    let all = db::list_labels(db).await?;
    let review = all.iter().find(|l| l.is_system).context("the system Review label is missing")?;
    let active: Vec<_> = all.iter().filter(|l| l.enabled && !l.is_system).collect();

    let rules = db::list_rules(db)
        .await?
        .into_iter()
        .filter(|r| r.enabled)
        .filter_map(|r| {
            Some(RuleInfo {
                id: r.id,
                label_id: r.label_id,
                action: RuleAction::parse(&r.action)?,
                folder: r.params.get("folder").and_then(Value::as_str).map(str::to_string),
                min_probability: r.min_probability,
            })
        })
        .collect();

    Ok(Context {
        labels: active.iter().map(|l| LabelInfo { id: l.id, name: l.name.clone(), threshold: l.threshold }).collect(),
        criteria: active
            .iter()
            .map(|l| LabelCriterion { name: l.name.clone(), description: l.description.clone() })
            .collect(),
        review: LabelInfo { id: review.id, name: review.name.clone(), threshold: None },
        settings,
        rules,
    })
}

#[derive(Debug, Serialize)]
pub struct Classified {
    pub state: String,
    pub decision: Decision,
    pub outcome: Outcome,
}

pub async fn classify(app: &AppState, ctx: &Context, sender: &str, subject: &str, body: &str) -> Result<Classified> {
    let state = jev::build_state(sender, subject, body, ctx.settings.body_chars);
    let decision = app.jev.classify(&state, &ctx.settings.instructions, &ctx.criteria).await?;
    let outcome = decide(ctx, &decision);
    Ok(Classified { state, decision, outcome })
}

pub fn decide(ctx: &Context, decision: &Decision) -> Outcome {
    decision::decide(&decision.probabilities, &ctx.labels, &ctx.review, ctx.settings.threshold, &ctx.rules)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub action: RuleAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn planned_records(outcome: &Outcome) -> Vec<ActionRecord> {
    outcome
        .actions
        .iter()
        .map(|a| ActionRecord { action: a.action, folder: a.folder.clone(), done: false, error: None })
        .collect()
}

pub struct Execution {
    pub label_applied: bool,
    pub actions: Vec<ActionRecord>,
    pub error: Option<String>,
}

impl Execution {
    pub fn status(&self, outcome: &Outcome) -> &'static str {
        match (self.label_applied, outcome.is_review) {
            (false, _) => "error",
            (true, true) => "review",
            (true, false) => "applied",
        }
    }
}

/// Applies the outcome's label, then its planned actions in order. Never deletes anything.
pub async fn execute(provider: &mut dyn MailProvider, msg_id: &str, outcome: &Outcome) -> Execution {
    if let Err(e) = provider.apply_label(msg_id, &outcome.label_name).await {
        return Execution {
            label_applied: false,
            actions: planned_records(outcome),
            error: Some(format!("applying label: {e:#}")),
        };
    }

    let mut actions = Vec::new();
    let mut first_error = None;
    for planned in &outcome.actions {
        let result = match planned.action {
            RuleAction::Star => provider.star(msg_id).await,
            RuleAction::MarkRead => provider.mark_read(msg_id).await,
            RuleAction::Archive => provider.archive(msg_id).await,
            RuleAction::MoveToFolder => provider.move_to(msg_id, planned.folder.as_deref().unwrap_or_default()).await,
        };
        let error = result.err().map(|e| format!("{e:#}"));
        if first_error.is_none() {
            first_error = error.as_ref().map(|e| format!("{:?}: {e}", planned.action));
        }
        actions.push(ActionRecord {
            action: planned.action,
            folder: planned.folder.clone(),
            done: error.is_none(),
            error,
        });
    }
    Execution { label_applied: true, actions, error: first_error }
}

pub fn snippet(body: &str) -> String {
    let collapsed = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut s: String = collapsed.chars().take(240).collect();
    if collapsed.chars().nth(240).is_some() {
        s.push('…');
    }
    s
}

pub fn record(account_id: i64, msg: &Message, c: &Classified) -> NewClassification {
    NewClassification {
        account_id,
        provider_msg_id: msg.id.clone(),
        thread_id: msg.thread_id.clone(),
        sender: msg.sender.clone(),
        subject: msg.subject.clone(),
        snippet: snippet(&msg.body),
        received_at: msg.received_at.clone(),
        label_id: Some(c.outcome.label_id),
        top_label: c.outcome.top_label.clone(),
        top_probability: Some(c.outcome.top_probability),
        confidence: c.decision.confidence,
        probabilities: json!(c.decision.probabilities),
        model: c.decision.model.clone(),
        status: "dry_run",
        actions_taken: json!(planned_records(&c.outcome)),
        ..Default::default()
    }
}
