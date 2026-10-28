//! Microsoft Graph (Outlook / Microsoft 365). Labels become Outlook categories.

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::oauth::{OAuthSession, send_authed};
use super::{MailProvider, Message, MsgRef, ensure_success, html_to_text, normalize_whitespace};

const BASE: &str = "https://graph.microsoft.com/v1.0";
const CURSOR_OVERLAP_SECS: i64 = 3600;
/// Immutable ids survive moves between folders, so overrides keep working after archive.
const PREFER: &str = "IdType=\"ImmutableId\", outlook.body-content-type=\"text\"";

pub struct OutlookProvider {
    auth: OAuthSession,
    http: reqwest::Client,
    categories: Option<HashSet<String>>,
}

#[derive(Deserialize)]
struct Page<T> {
    value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
}

#[derive(Deserialize)]
struct IdOnly {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphMessage {
    id: String,
    conversation_id: Option<String>,
    subject: Option<String>,
    from: Option<Recipient>,
    body: Option<ItemBody>,
    received_date_time: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Recipient {
    email_address: EmailAddress,
}

#[derive(Deserialize)]
struct EmailAddress {
    name: Option<String>,
    address: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemBody {
    content_type: Option<String>,
    content: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Category {
    display_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub mail: Option<String>,
    pub user_principal_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Folder {
    id: String,
}

impl OutlookProvider {
    pub fn new(auth: OAuthSession, http: reqwest::Client) -> Self {
        Self { auth, http, categories: None }
    }

    async fn send(&mut self, method: Method, url: &str, body: Option<Value>) -> Result<reqwest::Response> {
        let url = if url.starts_with("https://") { url.to_string() } else { format!("{BASE}{url}") };
        let http = self.http.clone();
        let res = send_authed(&mut self.auth, |token| {
            let mut req = http.request(method.clone(), &url).bearer_auth(token).header("Prefer", PREFER);
            if let Some(b) = &body {
                req = req.json(b);
            }
            req
        })
        .await?;
        ensure_success(res).await.with_context(|| format!("Graph {method} {url}"))
    }

    async fn call<T: DeserializeOwned>(&mut self, method: Method, url: &str, body: Option<Value>) -> Result<T> {
        Ok(self.send(method, url, body).await?.json().await?)
    }

    pub async fn me(&mut self) -> Result<Me> {
        self.call(Method::GET, "/me?$select=mail,userPrincipalName", None).await
    }

    async fn ensure_category(&mut self, name: &str) {
        // Categories work on messages without a master entry; this only adds colour in Outlook.
        if self.categories.is_none() {
            match self.call::<Page<Category>>(Method::GET, "/me/outlook/masterCategories", None).await {
                Ok(p) => self.categories = Some(p.value.into_iter().map(|c| c.display_name.to_lowercase()).collect()),
                Err(e) => {
                    tracing::debug!(error = %e, "listing master categories failed");
                    self.categories = Some(HashSet::new());
                }
            }
        }
        if self.categories.as_ref().is_some_and(|c| c.contains(&name.to_lowercase())) {
            return;
        }
        let body = json!({ "displayName": name, "color": "preset7" });
        if let Err(e) = self.send(Method::POST, "/me/outlook/masterCategories", Some(body)).await {
            tracing::debug!(error = %e, "creating master category failed");
        }
        self.categories.get_or_insert_default().insert(name.to_lowercase());
    }

    async fn categories_of(&mut self, id: &str) -> Result<Vec<String>> {
        let m: GraphMessage = self.call(Method::GET, &format!("/me/messages/{id}?$select=id,categories"), None).await?;
        Ok(m.categories)
    }

    async fn patch(&mut self, id: &str, body: Value) -> Result<()> {
        self.send(Method::PATCH, &format!("/me/messages/{id}"), Some(body)).await?;
        Ok(())
    }

    async fn move_to_folder_id(&mut self, id: &str, destination: &str) -> Result<()> {
        self.send(Method::POST, &format!("/me/messages/{id}/move"), Some(json!({ "destinationId": destination })))
            .await?;
        Ok(())
    }

    async fn folder_id(&mut self, name: &str) -> Result<String> {
        let escaped = name.replace('\'', "''");
        let url = format!("/me/mailFolders?$filter=displayName eq '{escaped}'&$select=id");
        let page: Page<Folder> = self.call(Method::GET, &url, None).await?;
        if let Some(f) = page.value.into_iter().next() {
            return Ok(f.id);
        }
        let f: Folder = self.call(Method::POST, "/me/mailFolders", Some(json!({ "displayName": name }))).await?;
        Ok(f.id)
    }
}

#[async_trait]
impl MailProvider for OutlookProvider {
    async fn list_new(&mut self, cursor: Option<&str>, limit: usize) -> Result<(Vec<MsgRef>, Option<String>)> {
        let started = Utc::now();
        let since = cursor
            .and_then(|c| DateTime::parse_from_rfc3339(c).ok())
            .map(|d| d.with_timezone(&Utc) - chrono::Duration::seconds(CURSOR_OVERLAP_SECS))
            .unwrap_or_else(|| started - chrono::Duration::days(7));

        let mut url = format!(
            "/me/mailFolders/inbox/messages?$select=id&$orderby=receivedDateTime asc&$top=100&$filter=receivedDateTime ge {}",
            since.to_rfc3339_opts(SecondsFormat::Secs, true)
        );
        let mut ids = Vec::new();
        let mut truncated = false;
        loop {
            let page: Page<IdOnly> = self.call(Method::GET, &url, None).await?;
            ids.extend(page.value.into_iter().map(|m| m.id));
            match page.next_link {
                Some(next) if ids.len() < limit => url = next,
                Some(_) => {
                    truncated = true;
                    break;
                }
                None => break,
            }
        }
        if ids.len() > limit {
            ids.truncate(limit);
            truncated = true;
        }
        let next = if truncated {
            cursor.map(str::to_string)
        } else {
            Some(started.to_rfc3339_opts(SecondsFormat::Secs, true))
        };
        Ok((ids.into_iter().map(|id| MsgRef { id }).collect(), next))
    }

    async fn fetch(&mut self, id: &str) -> Result<Message> {
        let m: GraphMessage = self
            .call(
                Method::GET,
                &format!("/me/messages/{id}?$select=id,conversationId,subject,from,body,receivedDateTime,categories"),
                None,
            )
            .await?;
        let sender = m
            .from
            .map(|r| match (r.email_address.name, r.email_address.address) {
                (Some(n), Some(a)) if n != a => format!("{n} <{a}>"),
                (_, Some(a)) => a,
                (Some(n), None) => n,
                (None, None) => String::new(),
            })
            .unwrap_or_default();
        let body = match m.body {
            Some(ItemBody { content_type: Some(t), content: Some(c) }) if t.eq_ignore_ascii_case("html") => {
                html_to_text(&c)
            }
            Some(ItemBody { content: Some(c), .. }) => c,
            _ => String::new(),
        };
        Ok(Message {
            id: m.id,
            thread_id: m.conversation_id,
            sender,
            subject: m.subject.unwrap_or_default(),
            body: normalize_whitespace(&body),
            received_at: m.received_date_time,
        })
    }

    async fn apply_label(&mut self, id: &str, label: &str) -> Result<()> {
        self.ensure_category(label).await;
        let mut cats = self.categories_of(id).await?;
        if cats.iter().any(|c| c.eq_ignore_ascii_case(label)) {
            return Ok(());
        }
        cats.push(label.to_string());
        self.patch(id, json!({ "categories": cats })).await
    }

    async fn remove_label(&mut self, id: &str, label: &str) -> Result<()> {
        let cats = self.categories_of(id).await?;
        let kept: Vec<_> = cats.iter().filter(|c| !c.eq_ignore_ascii_case(label)).collect();
        if kept.len() == cats.len() {
            return Ok(());
        }
        self.patch(id, json!({ "categories": kept })).await
    }

    async fn archive(&mut self, id: &str) -> Result<()> {
        self.move_to_folder_id(id, "archive").await
    }

    async fn mark_read(&mut self, id: &str) -> Result<()> {
        self.patch(id, json!({ "isRead": true })).await
    }

    async fn star(&mut self, id: &str) -> Result<()> {
        self.patch(id, json!({ "flag": { "flagStatus": "flagged" } })).await
    }

    async fn move_to(&mut self, id: &str, folder: &str) -> Result<()> {
        if folder.trim().is_empty() {
            bail!("no destination folder configured");
        }
        let folder_id = self.folder_id(folder.trim()).await?;
        self.move_to_folder_id(id, &folder_id).await
    }

    async fn test(&mut self) -> Result<String> {
        let me = self.me().await?;
        let who = me.mail.or(me.user_principal_name).unwrap_or_else(|| "unknown".into());
        Ok(format!("Connected to Outlook as {who}."))
    }
}
