//! Client for TypeSafe's Jev System One endpoint.
//!
//! One request sends the email as `state` and asks a single `choice` question
//! whose criteria are the configured labels (name -> description).

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Jev accepts at most this many options per Choice question.
pub const MAX_OPTIONS: usize = 255;
const QUESTION_ID: &str = "label";
const MAX_ATTEMPTS: u32 = 4;

#[derive(Debug, Clone, Serialize)]
pub struct LabelCriterion {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Decision {
    pub choice: String,
    pub probabilities: BTreeMap<String, f64>,
    pub confidence: Option<f64>,
    pub model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[async_trait]
pub trait JevClient: Send + Sync {
    async fn classify(&self, state: &str, instructions: &str, labels: &[LabelCriterion]) -> Result<Decision>;
}

pub struct TypeSafeClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl TypeSafeClient {
    pub fn new(http: reqwest::Client, base_url: &str, api_key: Option<String>, model: &str) -> Self {
        Self { http, base_url: base_url.trim_end_matches('/').to_string(), api_key, model: model.to_string() }
    }
}

pub fn request_body(model: &str, state: &str, instructions: &str, labels: &[LabelCriterion]) -> Value {
    let criteria: Map<String, Value> = labels
        .iter()
        .map(|l| {
            let desc = l.description.trim();
            (l.name.clone(), if desc.is_empty() { Value::Null } else { Value::String(desc.to_string()) })
        })
        .collect();
    json!({
        "model": model,
        "state": state,
        "questions": {
            QUESTION_ID: { "type": "choice", "instructions": instructions, "criteria": criteria }
        }
    })
}

#[derive(Debug, Deserialize)]
struct SystemOneResponse {
    model: Option<String>,
    answers: HashMap<String, ChoiceAnswer>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct ChoiceAnswer {
    choice: Option<String>,
    #[serde(default)]
    probabilities: BTreeMap<String, f64>,
    confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

pub fn parse_response(body: &str) -> Result<Decision> {
    let resp: SystemOneResponse = serde_json::from_str(body).context("unexpected Jev response shape")?;
    let answer = resp.answers.get(QUESTION_ID).ok_or_else(|| anyhow!("Jev response has no `{QUESTION_ID}` answer"))?;
    let choice = match &answer.choice {
        Some(c) => c.clone(),
        None => answer
            .probabilities
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(k, _)| k.clone())
            .ok_or_else(|| anyhow!("Jev answer has neither a choice nor probabilities"))?,
    };
    Ok(Decision {
        choice,
        probabilities: answer.probabilities.clone(),
        confidence: answer.confidence,
        model: resp.model,
        input_tokens: resp.usage.as_ref().and_then(|u| u.input_tokens),
        output_tokens: resp.usage.as_ref().and_then(|u| u.output_tokens),
    })
}

/// Builds the text Jev sees: headers plus the body trimmed to `body_chars` characters.
pub fn build_state(sender: &str, subject: &str, body: &str, body_chars: usize) -> String {
    let mut trimmed: String = body.chars().take(body_chars).collect();
    if body.chars().nth(body_chars).is_some() {
        trimmed.push_str(" …");
    }
    format!("From: {sender}\nSubject: {subject}\n\n{trimmed}")
}

#[async_trait]
impl JevClient for TypeSafeClient {
    async fn classify(&self, state: &str, instructions: &str, labels: &[LabelCriterion]) -> Result<Decision> {
        let key = self.api_key.as_deref().ok_or_else(|| anyhow!("JEV_API_KEY is not set"))?;
        if labels.is_empty() {
            bail!("no enabled labels to choose from");
        }
        if labels.len() > MAX_OPTIONS {
            bail!("Jev supports at most {MAX_OPTIONS} labels, {} are enabled", labels.len());
        }

        let url = format!("{}/v1/systemone", self.base_url);
        let body = request_body(&self.model, state, instructions, labels);
        let mut delay = Duration::from_millis(500);

        for attempt in 1..=MAX_ATTEMPTS {
            let last = attempt == MAX_ATTEMPTS;
            match self.http.post(&url).bearer_auth(key).json(&body).send().await {
                Ok(res) if res.status().is_success() => {
                    let text = res.text().await?;
                    return parse_response(&text);
                }
                Ok(res)
                    if !last && (res.status() == StatusCode::TOO_MANY_REQUESTS || res.status().is_server_error()) =>
                {
                    if let Some(secs) = res
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                    {
                        delay = Duration::from_secs(secs.min(60));
                    }
                    tracing::warn!(status = %res.status(), attempt, "Jev request failed, retrying in {delay:?}");
                }
                Ok(res) => {
                    let status = res.status();
                    let text: String = res.text().await.unwrap_or_default().chars().take(500).collect();
                    bail!("Jev API returned {status}: {text}");
                }
                Err(e) if !last && (e.is_timeout() || e.is_connect() || e.is_request()) => {
                    tracing::warn!(error = %e, attempt, "Jev request error, retrying in {delay:?}");
                }
                Err(e) => return Err(anyhow!(e).context("calling Jev API")),
            }
            tokio::time::sleep(delay).await;
            delay *= 2;
        }
        unreachable!("loop returns on the last attempt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const FIXTURE: &str = r#"{
        "model": "jev-1.13.0",
        "answers": {
            "label": {
                "type": "choice",
                "choice": "Receipts",
                "confidence": 0.82,
                "probabilities": { "Receipts": 0.91, "Promos": 0.07, "Updates": 0.02 }
            }
        },
        "usage": { "input_tokens": 328, "output_tokens": 34 }
    }"#;

    fn labels() -> Vec<LabelCriterion> {
        vec![
            LabelCriterion { name: "Receipts".into(), description: "Invoices and receipts".into() },
            LabelCriterion { name: "Promos".into(), description: "".into() },
            LabelCriterion { name: "Updates".into(), description: "Notifications".into() },
        ]
    }

    #[test]
    fn parses_fixture() {
        let d = parse_response(FIXTURE).unwrap();
        assert_eq!(d.choice, "Receipts");
        assert_eq!(d.probabilities["Receipts"], 0.91);
        assert_eq!(d.confidence, Some(0.82));
        assert_eq!(d.model.as_deref(), Some("jev-1.13.0"));
        assert_eq!(d.input_tokens, Some(328));
    }

    #[test]
    fn falls_back_to_argmax_without_choice() {
        let body = r#"{"answers":{"label":{"probabilities":{"A":0.2,"B":0.8}}}}"#;
        assert_eq!(parse_response(body).unwrap().choice, "B");
    }

    #[test]
    fn request_body_uses_null_for_empty_descriptions() {
        let body = request_body("jev-latest", "state", "pick one", &labels());
        let criteria = &body["questions"]["label"]["criteria"];
        assert_eq!(criteria["Receipts"], "Invoices and receipts");
        assert!(criteria["Promos"].is_null());
        assert_eq!(body["questions"]["label"]["type"], "choice");
    }

    #[test]
    fn build_state_trims_on_char_boundary() {
        let body = "héllo wörld ".repeat(500);
        let state = build_state("a@b.c", "Hi", &body, 7);
        assert!(state.ends_with("héllo w …"));
        assert!(state.starts_with("From: a@b.c\nSubject: Hi\n\n"));
        assert!(!build_state("a", "b", "short", 2000).contains('…'));
    }

    #[tokio::test]
    async fn classify_against_mock_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_string(FIXTURE))
            .expect(1)
            .mount(&server)
            .await;

        let client = TypeSafeClient::new(reqwest::Client::new(), &server.uri(), Some("test-key".into()), "jev-latest");
        let d = client.classify("From: x", "pick", &labels()).await.unwrap();
        assert_eq!(d.choice, "Receipts");
    }

    #[tokio::test]
    async fn classify_retries_server_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(503)).up_to_n_times(1).mount(&server).await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(FIXTURE))
            .mount(&server)
            .await;

        let client = TypeSafeClient::new(reqwest::Client::new(), &server.uri(), Some("k".into()), "jev-latest");
        assert_eq!(client.classify("s", "i", &labels()).await.unwrap().choice, "Receipts");
    }

    #[tokio::test]
    async fn classify_reports_client_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("bad key"))
            .mount(&server)
            .await;
        let client = TypeSafeClient::new(reqwest::Client::new(), &server.uri(), Some("k".into()), "jev-latest");
        let err = client.classify("s", "i", &labels()).await.unwrap_err().to_string();
        assert!(err.contains("401"), "{err}");
    }
}
