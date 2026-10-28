//! Generic IMAP. Labels become keywords (`$Jev_<label>`) when the server allows
//! custom flags, otherwise the message is copied into a `Jev/<label>` folder.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use async_imap::Session;
use async_imap::types::Flag;
use async_trait::async_trait;
use chrono::Utc;
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use tokio_rustls::rustls::crypto::{CryptoProvider, ring};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};

use super::{MailProvider, Message, MsgRef, parse_rfc822};

const TIMEOUT: Duration = Duration::from_secs(30);
const FIRST_RUN_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    #[default]
    Tls,
    Starttls,
    /// Plaintext; only for local test servers.
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapSettings {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub security: Security,
    /// For self-signed certificates (e.g. Proton Mail Bridge).
    #[serde(default)]
    pub accept_invalid_certs: bool,
    #[serde(default = "default_mailbox")]
    pub mailbox: String,
    #[serde(default = "default_archive")]
    pub archive_folder: String,
}

fn default_port() -> u16 {
    993
}
fn default_mailbox() -> String {
    "INBOX".into()
}
fn default_archive() -> String {
    "Archive".into()
}

pub trait ImapStream: AsyncRead + AsyncWrite + Unpin + Send + Sync + std::fmt::Debug {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync + std::fmt::Debug> ImapStream for T {}

type ImapSession = Session<Box<dyn ImapStream>>;

struct Selected {
    uid_validity: u32,
    exists: u32,
    keyword_mode: bool,
}

pub struct ImapProvider {
    settings: ImapSettings,
    password: String,
    session: Option<ImapSession>,
    selected: Option<Selected>,
    delimiter: Option<String>,
}

/// IMAP keyword for a label: `$Jev_` plus the name with non-alphanumerics replaced.
pub fn keyword_for(label: &str) -> String {
    let body: String = label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("$Jev_{body}")
}

/// Folder-safe label name (ASCII, no hierarchy delimiters or wildcards).
pub fn folder_name_for(label: &str) -> String {
    label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' })
        .collect::<String>()
        .trim()
        .to_string()
}

fn parse_id(id: &str) -> Result<(u32, u32)> {
    let (uv, uid) = id.split_once(':').ok_or_else(|| anyhow!("bad IMAP message id {id}"))?;
    Ok((uv.parse()?, uid.parse()?))
}

fn parse_cursor(cursor: Option<&str>) -> Option<(u32, u32)> {
    cursor.and_then(|c| parse_id(c).ok())
}

impl ImapProvider {
    pub fn new(settings: ImapSettings, password: String) -> Self {
        Self { settings, password, session: None, selected: None, delimiter: None }
    }

    async fn open(&self) -> Result<ImapSession> {
        let s = &self.settings;
        let tcp = timeout(TIMEOUT, TcpStream::connect((s.host.as_str(), s.port)))
            .await
            .context("IMAP connect timed out")?
            .with_context(|| format!("connecting to {}:{}", s.host, s.port))?;

        let stream: Box<dyn ImapStream> = match s.security {
            Security::Tls => Box::new(tls_wrap(&s.host, s.accept_invalid_certs, tcp).await?),
            Security::None => Box::new(tcp),
            Security::Starttls => {
                let mut client = async_imap::Client::new(tcp);
                read_greeting(&mut client).await?;
                client.run_command_and_check_ok("STARTTLS", None).await.context("STARTTLS")?;
                Box::new(tls_wrap(&s.host, s.accept_invalid_certs, client.into_inner()).await?)
            }
        };

        let mut client = async_imap::Client::new(stream);
        if s.security != Security::Starttls {
            read_greeting(&mut client).await?;
        }
        timeout(TIMEOUT, client.login(&s.username, &self.password))
            .await
            .context("IMAP login timed out")?
            .map_err(|(e, _)| anyhow!("IMAP login failed: {e}"))
    }

    async fn session(&mut self) -> Result<&mut ImapSession> {
        if self.session.is_none() {
            self.session = Some(self.open().await?);
        }
        Ok(self.session.as_mut().expect("just set"))
    }

    async fn select(&mut self) -> Result<&Selected> {
        if self.selected.is_none() {
            let mailbox = self.settings.mailbox.clone();
            let mb = self.session().await?.select(&mailbox).await.with_context(|| format!("selecting {mailbox}"))?;
            self.selected = Some(Selected {
                uid_validity: mb.uid_validity.unwrap_or(0),
                exists: mb.exists,
                keyword_mode: mb.permanent_flags.iter().any(|f| matches!(f, Flag::MayCreate)),
            });
        }
        Ok(self.selected.as_ref().expect("just set"))
    }

    /// Selects the mailbox and returns the UID for `id`, checking UIDVALIDITY.
    async fn uid_for(&mut self, id: &str) -> Result<u32> {
        let (uv, uid) = parse_id(id)?;
        if self.select().await?.uid_validity != uv {
            bail!("mailbox UIDVALIDITY changed; message {id} no longer exists under that id");
        }
        Ok(uid)
    }

    async fn store(&mut self, id: &str, query: &str) -> Result<()> {
        let uid = self.uid_for(id).await?;
        let session = self.session().await?;
        let _: Vec<_> = session.uid_store(uid.to_string(), query).await?.try_collect().await?;
        Ok(())
    }

    async fn folder(&mut self, name: &str) -> Result<String> {
        if self.delimiter.is_none() {
            let session = self.session().await?;
            let names: Vec<_> = session.list(Some(""), Some("")).await?.try_collect().await?;
            self.delimiter = Some(names.first().and_then(|n| n.delimiter()).unwrap_or("/").to_string());
        }
        let delim = self.delimiter.as_deref().unwrap_or("/");
        Ok(format!("Jev{delim}{}", folder_name_for(name)))
    }

    async fn ensure_folder(&mut self, folder: &str) -> Result<()> {
        // CREATE fails when the folder exists; that's fine.
        if let Err(e) = self.session().await?.create(folder).await {
            tracing::debug!(folder, error = %e, "CREATE failed (probably exists)");
        }
        Ok(())
    }

    async fn uid_move(&mut self, id: &str, folder: &str) -> Result<()> {
        let uid = self.uid_for(id).await?;
        self.ensure_folder(folder).await?;
        self.session()
            .await?
            .uid_mv(uid.to_string(), folder)
            .await
            .with_context(|| format!("moving to {folder} (the server must support MOVE)"))?;
        Ok(())
    }
}

async fn read_greeting<T: ImapStream>(client: &mut async_imap::Client<T>) -> Result<()> {
    timeout(TIMEOUT, client.read_response())
        .await
        .context("IMAP greeting timed out")?
        .ok_or_else(|| anyhow!("server closed the connection before greeting"))??;
    Ok(())
}

async fn tls_wrap(
    host: &str,
    accept_invalid: bool,
    tcp: TcpStream,
) -> Result<tokio_rustls::client::TlsStream<TcpStream>> {
    let provider = Arc::new(ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider.clone()).with_safe_default_protocol_versions()?;
    let config = if accept_invalid {
        builder.dangerous().with_custom_certificate_verifier(Arc::new(AcceptAnyCert(provider))).with_no_client_auth()
    } else {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        builder.with_root_certificates(roots).with_no_client_auth()
    };
    let name = ServerName::try_from(host.to_string()).context("invalid IMAP host name")?;
    timeout(TIMEOUT, TlsConnector::from(Arc::new(config)).connect(name, tcp))
        .await
        .context("TLS handshake timed out")?
        .context("TLS handshake failed")
}

#[derive(Debug)]
struct AcceptAnyCert(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, tokio_rustls::rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        tokio_rustls::rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        tokio_rustls::rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

#[async_trait]
impl MailProvider for ImapProvider {
    async fn list_new(&mut self, cursor: Option<&str>, limit: usize) -> Result<(Vec<MsgRef>, Option<String>)> {
        let uv = self.select().await?.uid_validity;
        let session = self.session().await?;

        let mut uids: Vec<u32> = match parse_cursor(cursor) {
            Some((cur_uv, last)) if cur_uv == uv => {
                // `n:*` always matches the highest UID, even when it is below n.
                session.uid_search(format!("UID {}:*", last + 1)).await?.into_iter().filter(|u| *u > last).collect()
            }
            _ => {
                let since = (Utc::now() - chrono::Duration::days(FIRST_RUN_DAYS)).format("%d-%b-%Y");
                session.uid_search(format!("SINCE {since}")).await?.into_iter().collect()
            }
        };
        uids.sort_unstable();
        uids.truncate(limit);

        let next_cursor = match uids.last() {
            Some(max) => Some(format!("{uv}:{max}")),
            None => match parse_cursor(cursor) {
                Some((cur_uv, _)) if cur_uv == uv => cursor.map(str::to_string),
                _ => Some(format!("{uv}:0")),
            },
        };
        Ok((uids.into_iter().map(|u| MsgRef { id: format!("{uv}:{u}") }).collect(), next_cursor))
    }

    async fn fetch(&mut self, id: &str) -> Result<Message> {
        let uid = self.uid_for(id).await?;
        let session = self.session().await?;
        // BODY.PEEK leaves the \Seen flag alone.
        let fetches: Vec<_> = session.uid_fetch(uid.to_string(), "(UID BODY.PEEK[])").await?.try_collect().await?;
        let raw = fetches.iter().find_map(|f| f.body()).ok_or_else(|| anyhow!("message {id} not found"))?;
        let parsed = parse_rfc822(raw);
        Ok(Message {
            id: id.to_string(),
            thread_id: None,
            sender: parsed.sender,
            subject: parsed.subject,
            body: parsed.body,
            received_at: parsed.date,
        })
    }

    async fn apply_label(&mut self, id: &str, label: &str) -> Result<()> {
        if self.select().await?.keyword_mode {
            return self.store(id, &format!("+FLAGS ({})", keyword_for(label))).await;
        }
        let uid = self.uid_for(id).await?;
        let folder = self.folder(label).await?;
        self.ensure_folder(&folder).await?;
        self.session()
            .await?
            .uid_copy(uid.to_string(), &folder)
            .await
            .with_context(|| format!("copying to {folder}"))?;
        Ok(())
    }

    async fn remove_label(&mut self, id: &str, label: &str) -> Result<()> {
        if self.select().await?.keyword_mode {
            return self.store(id, &format!("-FLAGS ({})", keyword_for(label))).await;
        }
        // Removing a folder copy would mean deleting a message, which this app never does.
        tracing::warn!(id, label, "folder-mode label left in place; remove the copy manually if needed");
        Ok(())
    }

    async fn archive(&mut self, id: &str) -> Result<()> {
        let folder = self.settings.archive_folder.clone();
        self.uid_move(id, &folder).await
    }

    async fn mark_read(&mut self, id: &str) -> Result<()> {
        self.store(id, "+FLAGS (\\Seen)").await
    }

    async fn star(&mut self, id: &str) -> Result<()> {
        self.store(id, "+FLAGS (\\Flagged)").await
    }

    async fn move_to(&mut self, id: &str, folder: &str) -> Result<()> {
        if folder.trim().is_empty() {
            bail!("no destination folder configured");
        }
        self.uid_move(id, folder.trim()).await
    }

    async fn test(&mut self) -> Result<String> {
        let mailbox = self.settings.mailbox.clone();
        let sel = self.select().await?;
        let mode = if sel.keyword_mode { "keywords" } else { "Jev/<label> folders" };
        Ok(format!("Connected to {mailbox} ({} messages). Labels will be stored as {mode}.", sel.exists))
    }

    async fn close(&mut self) {
        if let Some(mut s) = self.session.take() {
            let _ = timeout(Duration::from_secs(5), s.logout()).await;
        }
        self.selected = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_are_atom_safe() {
        assert_eq!(keyword_for("Needs reply"), "$Jev_Needs_reply");
        assert_eq!(keyword_for("Promos/Deals (50%)"), "$Jev_Promos_Deals__50__");
        assert_eq!(keyword_for("Café"), "$Jev_Caf_");
        let kw = keyword_for("a\"b\\c]d{e}*%");
        assert!(kw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$'), "{kw}");
    }

    #[test]
    fn folder_names_drop_delimiters() {
        assert_eq!(folder_name_for("Work/Clients.Big"), "Work_Clients_Big");
        assert_eq!(folder_name_for(" Needs reply "), "Needs reply");
    }

    #[test]
    fn ids_and_cursors_round_trip() {
        assert_eq!(parse_id("42:1001").unwrap(), (42, 1001));
        assert!(parse_id("nope").is_err());
        assert_eq!(parse_cursor(None), None);
    }

    #[test]
    fn settings_defaults() {
        let s: ImapSettings = serde_json::from_str(r#"{"host":"imap.example.com","username":"me"}"#).unwrap();
        assert_eq!(s.port, 993);
        assert_eq!(s.security, Security::Tls);
        assert_eq!(s.mailbox, "INBOX");
        assert_eq!(s.archive_folder, "Archive");
    }
}
