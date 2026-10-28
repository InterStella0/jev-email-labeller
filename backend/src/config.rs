use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::crypto;

#[derive(Debug, Clone)]
pub struct OAuthApp {
    pub client_id: String,
    pub client_secret: String,
    /// Only used by Microsoft (`common`, `consumers`, `organizations` or a tenant id).
    pub tenant: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub data_dir: PathBuf,
    pub database_url: Option<String>,
    pub public_url: String,
    pub admin_password: String,
    pub secret_key: [u8; 32],
    pub cookie_secure: bool,

    pub jev_api_key: Option<String>,
    pub jev_model: String,
    pub jev_base_url: String,

    pub google: Option<OAuthApp>,
    pub microsoft: Option<OAuthApp>,

    // Seed values for the settings table on first boot.
    pub default_threshold: f64,
    pub body_chars: usize,
    pub dry_run: bool,
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> Result<T> {
    match env(key) {
        Some(v) => v.parse().map_err(|_| anyhow::anyhow!("{key} has an invalid value: {v}")),
        None => Ok(default),
    }
}

fn env_bool(key: &str, default: bool) -> bool {
    match env(key).as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some("1" | "true" | "yes" | "on") => true,
        Some("0" | "false" | "no" | "off") => false,
        _ => default,
    }
}

pub fn data_dir() -> PathBuf {
    env("DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/app/data"))
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let admin_password = env("ADMIN_PASSWORD").context("ADMIN_PASSWORD must be set")?;
        let app_secret = env("APP_SECRET").context("APP_SECRET must be set (e.g. `openssl rand -hex 32`)")?;
        if app_secret.len() < 16 {
            bail!("APP_SECRET must be at least 16 characters");
        }

        let public_url = env("PUBLIC_URL").unwrap_or_else(|| "http://localhost:8080".into());
        let public_url = public_url.trim_end_matches('/').to_string();

        let google = match (env("GOOGLE_CLIENT_ID"), env("GOOGLE_CLIENT_SECRET")) {
            (Some(client_id), Some(client_secret)) => {
                Some(OAuthApp { client_id, client_secret, tenant: String::new() })
            }
            _ => None,
        };
        let microsoft = match (env("MS_CLIENT_ID"), env("MS_CLIENT_SECRET")) {
            (Some(client_id), Some(client_secret)) => {
                Some(OAuthApp { client_id, client_secret, tenant: env("MS_TENANT").unwrap_or_else(|| "common".into()) })
            }
            _ => None,
        };

        let default_threshold: f64 = env_parse("DEFAULT_THRESHOLD", 0.7)?;
        if !(0.0..=1.0).contains(&default_threshold) {
            bail!("DEFAULT_THRESHOLD must be between 0 and 1");
        }

        Ok(Self {
            bind: env("BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            data_dir: data_dir(),
            database_url: env("DATABASE_URL"),
            cookie_secure: env_bool("COOKIE_SECURE", public_url.starts_with("https://")),
            public_url,
            admin_password,
            secret_key: crypto::derive_key(&app_secret),
            jev_api_key: env("JEV_API_KEY"),
            jev_model: env("JEV_MODEL").unwrap_or_else(|| "jev-latest".into()),
            jev_base_url: env("JEV_BASE_URL").unwrap_or_else(|| "https://api.typesafe.ai".into()),
            google,
            microsoft,
            default_threshold,
            body_chars: env_parse("BODY_CHARS", 2000)?,
            dry_run: env_bool("DRY_RUN", true),
        })
    }

    pub fn redirect_uri(&self, provider: &str) -> String {
        format!("{}/api/oauth/{provider}/callback", self.public_url)
    }
}
