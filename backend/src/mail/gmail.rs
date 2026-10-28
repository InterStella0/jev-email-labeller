//! Gmail REST API with native labels.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::oauth::{OAuthSession, send_authed};
use super::{MailProvider, Message, MsgRef, ensure_success, parse_rfc822};

const BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
/// Overlap on the time cursor so late-delivered mail isn't missed; duplicates are skipped by the DB.
const CURSOR_OVERLAP_SECS: i64 = 3600;

pub struct GmailProvider {
    auth: OAuthSession,
    http: reqwest::Client,
    labels: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListResponse {
    #[serde(default)]
    messages: Vec<IdOnly>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct IdOnly {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMessage {
    id: String,
    thread_id: Option<String>,
    raw: String,
}

#[derive(Deserialize)]
struct LabelList {
    #[serde(default)]
    labels: Vec<GmailLabel>,
}

#[derive(Deserialize)]
struct GmailLabel {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub email_address: String,
    pub messages_total: Option<u64>,
}

impl GmailProvider {
    pub fn new(auth: OAuthSession, http: reqwest::Client) -> Self {
        Self { auth, http, labels: None }
    }

    async fn call<T: DeserializeOwned>(
        &mut self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<T> {
        let url = format!("{BASE}{path}");
        let http = self.http.clone();
        let res = send_authed(&mut self.auth, |token| {
            let mut req = http.request(method.clone(), &url).bearer_auth(token).query(query);
            if let Some(b) = &body {
                req = req.json(b);
            }
            req
        })
        .await?;
        let res = ensure_success(res).await.with_context(|| format!("Gmail {method} {path}"))?;
        Ok(res.json().await?)
    }

    pub async fn profile(&mut self) -> Result<Profile> {
        self.call(Method::GET, "/profile", &[], None).await
    }

    async fn label_id(&mut self, name: &str) -> Result<String> {
        if self.labels.is_none() {
            let list: LabelList = self.call(Method::GET, "/labels", &[], None).await?;
            self.labels = Some(list.labels.into_iter().map(|l| (l.name.to_lowercase(), l.id)).collect());
        }
        if let Some(id) = self.labels.as_ref().and_then(|m| m.get(&name.to_lowercase())) {
            return Ok(id.clone());
        }
        let created: GmailLabel = self
            .call(
                Method::POST,
                "/labels",
                &[],
                Some(json!({ "name": name, "labelListVisibility": "labelShow", "messageListVisibility": "show" })),
            )
            .await
            .with_context(|| format!("creating Gmail label {name}"))?;
        self.labels.get_or_insert_default().insert(created.name.to_lowercase(), created.id.clone());
        Ok(created.id)
    }

    async fn modify(&mut self, id: &str, add: &[String], remove: &[String]) -> Result<()> {
        let _: Value = self
            .call(
                Method::POST,
                &format!("/messages/{id}/modify"),
                &[],
                Some(json!({ "addLabelIds": add, "removeLabelIds": remove })),
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl MailProvider for GmailProvider {
    async fn list_new(&mut self, cursor: Option<&str>, limit: usize) -> Result<(Vec<MsgRef>, Option<String>)> {
        let started = Utc::now().timestamp();
        let q = match cursor.and_then(|c| c.parse::<i64>().ok()) {
            Some(after) => format!("in:inbox after:{}", after - CURSOR_OVERLAP_SECS),
            None => "in:inbox newer_than:7d".to_string(),
        };

        let mut ids = Vec::new();
        let mut page: Option<String> = None;
        let mut truncated = false;
        loop {
            let mut query = vec![("q", q.clone()), ("maxResults", "100".to_string())];
            if let Some(p) = &page {
                query.push(("pageToken", p.clone()));
            }
            let list: ListResponse = self.call(Method::GET, "/messages", &query, None).await?;
            ids.extend(list.messages.into_iter().map(|m| m.id));
            page = list.next_page_token;
            if page.is_none() {
                break;
            }
            if ids.len() >= limit {
                truncated = true;
                break;
            }
        }
        // Gmail lists newest first; process oldest first.
        ids.truncate(limit);
        ids.reverse();
        let next = if truncated { cursor.map(str::to_string) } else { Some(started.to_string()) };
        Ok((ids.into_iter().map(|id| MsgRef { id }).collect(), next))
    }

    async fn fetch(&mut self, id: &str) -> Result<Message> {
        let m: RawMessage =
            self.call(Method::GET, &format!("/messages/{id}"), &[("format", "raw".into())], None).await?;
        let raw = URL_SAFE_NO_PAD.decode(m.raw.trim_end_matches('=')).context("decoding Gmail raw message")?;
        let parsed = parse_rfc822(&raw);
        Ok(Message {
            id: m.id,
            thread_id: m.thread_id,
            sender: parsed.sender,
            subject: parsed.subject,
            body: parsed.body,
            received_at: parsed.date,
        })
    }

    async fn apply_label(&mut self, id: &str, label: &str) -> Result<()> {
        let label_id = self.label_id(label).await?;
        self.modify(id, &[label_id], &[]).await
    }

    async fn remove_label(&mut self, id: &str, label: &str) -> Result<()> {
        let label_id = self.label_id(label).await?;
        self.modify(id, &[], &[label_id]).await
    }

    async fn archive(&mut self, id: &str) -> Result<()> {
        self.modify(id, &[], &["INBOX".into()]).await
    }

    async fn mark_read(&mut self, id: &str) -> Result<()> {
        self.modify(id, &[], &["UNREAD".into()]).await
    }

    async fn star(&mut self, id: &str) -> Result<()> {
        self.modify(id, &["STARRED".into()], &[]).await
    }

    async fn move_to(&mut self, id: &str, folder: &str) -> Result<()> {
        if folder.trim().is_empty() {
            bail!("no destination label configured");
        }
        // Gmail has no folders: "moving" means adding the label and leaving the inbox.
        let label_id = self.label_id(folder.trim()).await?;
        self.modify(id, &[label_id], &["INBOX".into()]).await
    }

    async fn test(&mut self) -> Result<String> {
        let p = self.profile().await?;
        Ok(format!("Connected to Gmail as {} ({} messages).", p.email_address, p.messages_total.unwrap_or(0)))
    }
}
