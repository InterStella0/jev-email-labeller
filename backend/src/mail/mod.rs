//! Mailbox providers. The trait deliberately has no delete or trash operation.

pub mod gmail;
pub mod imap;
pub mod oauth;
pub mod outlook;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use mail_parser::{MessageParser, PartType};

use crate::crypto;
use crate::db::Account;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct MsgRef {
    pub id: String,
}

#[derive(Debug, Clone, Default)]
pub struct Message {
    pub id: String,
    pub thread_id: Option<String>,
    pub sender: String,
    pub subject: String,
    /// Plain text; HTML-only mail is converted.
    pub body: String,
    pub received_at: Option<String>,
}

#[async_trait]
pub trait MailProvider: Send {
    /// Candidate messages oldest first (at most `limit`) and the cursor to save once all are handled.
    async fn list_new(&mut self, cursor: Option<&str>, limit: usize) -> Result<(Vec<MsgRef>, Option<String>)>;
    async fn fetch(&mut self, id: &str) -> Result<Message>;
    /// Creates the label in the mailbox on first use.
    async fn apply_label(&mut self, id: &str, label: &str) -> Result<()>;
    async fn remove_label(&mut self, id: &str, label: &str) -> Result<()>;
    async fn archive(&mut self, id: &str) -> Result<()>;
    async fn mark_read(&mut self, id: &str) -> Result<()>;
    async fn star(&mut self, id: &str) -> Result<()>;
    async fn move_to(&mut self, id: &str, folder: &str) -> Result<()>;
    /// Checks connectivity and returns a human-readable summary.
    async fn test(&mut self) -> Result<String>;
    async fn close(&mut self) {}
}

pub async fn connect(app: &AppState, account: &Account) -> Result<Box<dyn MailProvider>> {
    match account.provider.as_str() {
        "imap" => {
            let settings: imap::ImapSettings =
                serde_json::from_value(account.settings.0.clone()).context("invalid IMAP settings")?;
            let sealed = account.secret_enc.as_deref().context("no IMAP password saved")?;
            let password = String::from_utf8(crypto::open(&app.cfg.secret_key, sealed)?)?;
            Ok(Box::new(imap::ImapProvider::new(settings, password)))
        }
        "gmail" => {
            let session = oauth::OAuthSession::load(app, account, oauth::OAuthProvider::Google)?;
            Ok(Box::new(gmail::GmailProvider::new(session, app.http.clone())))
        }
        "outlook" => {
            let session = oauth::OAuthSession::load(app, account, oauth::OAuthProvider::Microsoft)?;
            Ok(Box::new(outlook::OutlookProvider::new(session, app.http.clone())))
        }
        other => bail!("unknown provider {other}"),
    }
}

pub struct ParsedMail {
    pub sender: String,
    pub subject: String,
    pub body: String,
    pub date: Option<String>,
}

/// Parses a raw RFC 822 message into the fields Jev needs.
pub fn parse_rfc822(raw: &[u8]) -> ParsedMail {
    let Some(msg) = MessageParser::default().parse(raw) else {
        return ParsedMail { sender: String::new(), subject: String::new(), body: String::new(), date: None };
    };

    let sender = msg
        .from()
        .and_then(|a| a.first())
        .map(|a| match (a.name(), a.address()) {
            (Some(name), Some(addr)) => format!("{name} <{addr}>"),
            (None, Some(addr)) => addr.to_string(),
            (Some(name), None) => name.to_string(),
            (None, None) => String::new(),
        })
        .unwrap_or_default();

    let first_text_part = msg.text_body.first().and_then(|&i| msg.parts.get(i as usize));
    let body = match first_text_part.map(|p| &p.body) {
        Some(PartType::Text(t)) => t.to_string(),
        Some(PartType::Html(h)) => html_to_text(h),
        _ => match msg.body_html(0) {
            Some(h) => html_to_text(&h),
            None => String::new(),
        },
    };

    ParsedMail {
        sender,
        subject: msg.subject().unwrap_or_default().to_string(),
        body: normalize_whitespace(&body),
        date: msg.date().map(|d| d.to_rfc3339()),
    }
}

pub fn html_to_text(html: &str) -> String {
    html2text::from_read(html.as_bytes(), 120).unwrap_or_default()
}

/// Trims lines and collapses runs of blank lines, so the body budget isn't spent on whitespace.
pub fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;
    for line in text.lines().map(str::trim_end) {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

pub async fn ensure_success(res: reqwest::Response) -> Result<reqwest::Response> {
    let status = res.status();
    if status.is_success() {
        return Ok(res);
    }
    let body: String = res.text().await.unwrap_or_default().chars().take(400).collect();
    bail!("{status}: {body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_text_mail() {
        let raw = b"From: Alice Smith <alice@example.com>\r\nSubject: Lunch?\r\nDate: Tue, 1 Oct 2024 10:00:00 +0000\r\n\r\nAre you free\r\n\r\n\r\n\r\ntomorrow?\r\n";
        let m = parse_rfc822(raw);
        assert_eq!(m.sender, "Alice Smith <alice@example.com>");
        assert_eq!(m.subject, "Lunch?");
        assert_eq!(m.body, "Are you free\n\ntomorrow?");
        assert!(m.date.unwrap().starts_with("2024-10-01T10:00:00"));
    }

    #[test]
    fn converts_html_only_mail() {
        let raw = b"From: shop@example.com\r\nSubject: Sale\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><h1>50% off</h1><p>Today <b>only</b>.</p><script>x()</script></body></html>";
        let m = parse_rfc822(raw);
        assert_eq!(m.sender, "shop@example.com");
        assert!(m.body.contains("50% off"), "{}", m.body);
        assert!(m.body.contains("Today"));
        assert!(!m.body.contains("<p>"));
    }

    #[test]
    fn prefers_text_part_in_multipart_alternative() {
        let raw = b"From: a@b.c\r\nSubject: s\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=XX\r\n\r\n--XX\r\nContent-Type: text/plain\r\n\r\nplain version\r\n--XX\r\nContent-Type: text/html\r\n\r\n<p>html version</p>\r\n--XX--\r\n";
        assert_eq!(parse_rfc822(raw).body, "plain version");
    }

    #[test]
    fn garbage_does_not_panic() {
        let m = parse_rfc822(b"");
        assert!(m.subject.is_empty());
    }
}
