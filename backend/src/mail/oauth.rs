//! OAuth 2.0 authorization-code flow and token refresh for Google and Microsoft.

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::config::{Config, OAuthApp};
use crate::crypto;
use crate::db::{self, Account};
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthProvider {
    Google,
    Microsoft,
}

impl OAuthProvider {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "google" => Some(Self::Google),
            "microsoft" => Some(Self::Microsoft),
            _ => None,
        }
    }

    pub fn for_account(provider: &str) -> Option<Self> {
        match provider {
            "gmail" => Some(Self::Google),
            "outlook" => Some(Self::Microsoft),
            _ => None,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Microsoft => "microsoft",
        }
    }

    fn scopes(self) -> &'static str {
        match self {
            // gmail.modify can read and relabel; this app never calls trash/delete.
            Self::Google => "https://www.googleapis.com/auth/gmail.modify",
            Self::Microsoft => "offline_access User.Read Mail.ReadWrite MailboxSettings.ReadWrite",
        }
    }

    fn app(self, cfg: &Config) -> Result<&OAuthApp> {
        match self {
            Self::Google => cfg.google.as_ref().context("GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET are not set"),
            Self::Microsoft => cfg.microsoft.as_ref().context("MS_CLIENT_ID / MS_CLIENT_SECRET are not set"),
        }
    }

    fn auth_endpoint(self, app: &OAuthApp) -> String {
        match self {
            Self::Google => "https://accounts.google.com/o/oauth2/v2/auth".into(),
            Self::Microsoft => format!("https://login.microsoftonline.com/{}/oauth2/v2.0/authorize", app.tenant),
        }
    }

    fn token_endpoint(self, app: &OAuthApp) -> String {
        match self {
            Self::Google => "https://oauth2.googleapis.com/token".into(),
            Self::Microsoft => format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", app.tenant),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub refresh_token: String,
    pub access_token: Option<String>,
    /// Unix seconds.
    pub expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
}

pub fn authorize_url(cfg: &Config, provider: OAuthProvider, state: &str) -> Result<String> {
    let app = provider.app(cfg)?;
    let mut url = Url::parse(&provider.auth_endpoint(app))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &app.client_id)
            .append_pair("redirect_uri", &cfg.redirect_uri(provider.slug()))
            .append_pair("response_type", "code")
            .append_pair("scope", provider.scopes())
            .append_pair("state", state);
        match provider {
            OAuthProvider::Google => {
                q.append_pair("access_type", "offline").append_pair("prompt", "consent");
            }
            OAuthProvider::Microsoft => {
                q.append_pair("response_mode", "query").append_pair("prompt", "select_account");
            }
        }
    }
    Ok(url.into())
}

async fn token_request(
    http: &reqwest::Client,
    cfg: &Config,
    provider: OAuthProvider,
    params: &[(&str, &str)],
) -> Result<TokenResponse> {
    let app = provider.app(cfg)?;
    let mut form: Vec<(&str, &str)> = vec![("client_id", &app.client_id), ("client_secret", &app.client_secret)];
    form.extend_from_slice(params);
    if provider == OAuthProvider::Microsoft {
        form.push(("scope", provider.scopes()));
    }
    let res = http.post(provider.token_endpoint(app)).form(&form).send().await?;
    let res = super::ensure_success(res).await.context("token endpoint")?;
    Ok(res.json().await?)
}

pub async fn exchange_code(
    http: &reqwest::Client,
    cfg: &Config,
    provider: OAuthProvider,
    code: &str,
) -> Result<TokenSet> {
    let redirect = cfg.redirect_uri(provider.slug());
    let t = token_request(
        http,
        cfg,
        provider,
        &[("grant_type", "authorization_code"), ("code", code), ("redirect_uri", &redirect)],
    )
    .await?;
    Ok(TokenSet {
        refresh_token: t.refresh_token.ok_or_else(|| anyhow!("provider did not return a refresh token"))?,
        access_token: Some(t.access_token),
        expires_at: Utc::now().timestamp() + t.expires_in.unwrap_or(3600),
    })
}

/// Holds an account's tokens and refreshes (and persists) them when needed.
pub struct OAuthSession {
    app: AppState,
    account_id: i64,
    provider: OAuthProvider,
    tokens: TokenSet,
}

impl OAuthSession {
    pub fn load(app: &AppState, account: &Account, provider: OAuthProvider) -> Result<Self> {
        let sealed = account.secret_enc.as_deref().context("account is not connected yet; click Connect")?;
        let tokens: TokenSet = serde_json::from_slice(&crypto::open(&app.cfg.secret_key, sealed)?)?;
        Ok(Self { app: app.clone(), account_id: account.id, provider, tokens })
    }

    pub fn new(app: &AppState, account_id: i64, provider: OAuthProvider, tokens: TokenSet) -> Self {
        Self { app: app.clone(), account_id, provider, tokens }
    }

    pub async fn bearer(&mut self) -> Result<String> {
        if let Some(token) = &self.tokens.access_token
            && self.tokens.expires_at > Utc::now().timestamp() + 60
        {
            return Ok(token.clone());
        }

        let refresh = self.tokens.refresh_token.clone();
        let t = token_request(
            &self.app.http,
            &self.app.cfg,
            self.provider,
            &[("grant_type", "refresh_token"), ("refresh_token", &refresh)],
        )
        .await
        .context("refreshing access token; reconnect the account if this persists")?;

        self.tokens.access_token = Some(t.access_token.clone());
        self.tokens.expires_at = Utc::now().timestamp() + t.expires_in.unwrap_or(3600);
        if let Some(rt) = t.refresh_token {
            self.tokens.refresh_token = rt;
        }
        self.persist().await?;
        Ok(t.access_token)
    }

    pub async fn persist(&self) -> Result<()> {
        let sealed = crypto::seal(&self.app.cfg.secret_key, &serde_json::to_vec(&self.tokens)?)?;
        db::save_secret(&self.app.db, self.account_id, &sealed).await
    }
}

/// Retries once with a fresh token on 401, which covers tokens revoked early.
pub async fn send_authed(
    auth: &mut OAuthSession,
    build: impl Fn(&str) -> reqwest::RequestBuilder,
) -> Result<reqwest::Response> {
    let token = auth.bearer().await?;
    let res = build(&token).send().await?;
    if res.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(res);
    }
    auth.tokens.access_token = None;
    let token = auth.bearer().await?;
    let res = build(&token).send().await?;
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        bail!("provider rejected the access token; reconnect the account");
    }
    Ok(res)
}
