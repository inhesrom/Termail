use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Local;
use futures::StreamExt;
use imap_proto::types::Address as ImapAddress;

use crate::auth::google::{self, GoogleAuth};
use crate::backend::{EmailBackend, EmailFlag};
use crate::models::email::{Attachment, Email};
use crate::models::envelope::Envelope;
use crate::models::mailbox::Mailbox;

type ImapSession = async_imap::Session<async_native_tls::TlsStream<async_std::net::TcpStream>>;

/// Authentication credential for IMAP/SMTP connections.
pub enum ImapCredential {
    /// OAuth2 XOAUTH2 authentication.
    OAuth(Arc<GoogleAuth>),
    /// App password / PLAIN authentication.
    Password(String),
}

/// Gmail IMAP + SMTP backend.
pub struct ImapBackend {
    email: String,
    credential: ImapCredential,
}

impl ImapBackend {
    pub fn new(email: String, credential: ImapCredential) -> Self {
        Self { email, credential }
    }

    /// Connect and authenticate to Gmail IMAP.
    async fn connect(&self) -> Result<ImapSession> {
        tracing::info!("Connecting to imap.gmail.com:993...");
        let tls = async_native_tls::TlsConnector::new();
        let tcp = async_std::net::TcpStream::connect("imap.gmail.com:993").await?;
        let tls_stream = tls.connect("imap.gmail.com", tcp).await?;
        tracing::info!("TLS connection established");

        let client = async_imap::Client::new(tls_stream);

        match &self.credential {
            ImapCredential::Password(password) => {
                tracing::info!("Authenticating as {}...", self.email);
                let session = client
                    .login(&self.email, password)
                    .await
                    .map_err(|(e, _)| e)
                    .context("IMAP login authentication failed")?;
                tracing::info!("IMAP authentication successful for {}", self.email);
                Ok(session)
            }
            ImapCredential::OAuth(auth) => {
                tracing::info!("Authenticating via OAuth for {}...", self.email);
                let access_token = auth.get_access_token().await?;
                let xoauth2 = google::build_xoauth2_string(&self.email, &access_token);
                let session = client
                    .authenticate("XOAUTH2", XOAuth2Auth(xoauth2))
                    .await
                    .map_err(|(e, _)| e)
                    .context("IMAP XOAUTH2 authentication failed")?;
                tracing::info!("IMAP authentication successful for {}", self.email);
                Ok(session)
            }
        }
    }
}

/// Simple authenticator wrapper for async-imap XOAUTH2.
struct XOAuth2Auth(String);

impl async_imap::Authenticator for XOAuth2Auth {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        self.0.clone()
    }
}

#[async_trait]
impl EmailBackend for ImapBackend {
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>> {
        let mut session = self.connect().await?;
        let mailboxes_stream = session.list(Some(""), Some("*")).await?;
        let raw_mailboxes: Vec<_> = mailboxes_stream.collect().await;

        let mut mailboxes = Vec::new();
        for mb in raw_mailboxes {
            let mb = mb?;
            let name = mb.name().to_string();
            mailboxes.push(Mailbox {
                name: name.clone(),
                path: name,
                total: 0,
                unread: 0,
            });
        }

        tracing::debug!("Listed {} mailboxes", mailboxes.len());
        session.logout().await?;
        Ok(mailboxes)
    }

    async fn fetch_envelopes(
        &self,
        mailbox: &str,
        since_uid: Option<u32>,
        limit: Option<u32>,
    ) -> Result<Vec<Envelope>> {
        let mut session = self.connect().await?;
        let mailbox_info = session.select(mailbox).await?;

        let fetch_items = "(UID FLAGS ENVELOPE BODY.PEEK[TEXT]<0.200>)";
        let messages: Vec<_> = match since_uid {
            Some(uid) => {
                let range = format!("{}:*", uid + 1);
                session.uid_fetch(&range, fetch_items).await?.collect().await
            }
            None => {
                let limit = limit.unwrap_or(50);
                let total = mailbox_info.exists;
                if total == 0 {
                    session.logout().await?;
                    return Ok(vec![]);
                }
                let start = total.saturating_sub(limit - 1);
                let range = format!("{}:*", start);
                session.fetch(&range, fetch_items).await?.collect().await
            }
        };

        let mut envelopes = Vec::new();
        for msg in messages {
            let msg = msg?;
            if let Some(env) = parse_fetch_to_envelope(&msg) {
                envelopes.push(env);
            }
        }

        tracing::debug!("Fetched {} envelopes from {}", envelopes.len(), mailbox);
        session.logout().await?;
        envelopes.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(envelopes)
    }

    async fn fetch_email(&self, mailbox: &str, uid: u32) -> Result<Email> {
        tracing::debug!("Fetching full email uid={} from {}", uid, mailbox);
        let mut session = self.connect().await?;
        session.select(mailbox).await?;

        let messages_stream = session
            .uid_fetch(uid.to_string(), "(UID FLAGS ENVELOPE BODY[])")
            .await?;
        let messages: Vec<_> = messages_stream.collect().await;
        let msg = messages
            .into_iter()
            .next()
            .context("No message found")?
            .context("Failed to fetch message")?;

        let raw_body = msg.body().unwrap_or_default();
        let parsed = mailparse::parse_mail(raw_body)?;

        let body_text = extract_text_body(&parsed);
        let body_html = extract_html_body(&parsed);
        let attachments = extract_attachments(&parsed);

        let (is_read, is_starred) = parse_imap_flags(&msg);

        let (from_name, from_address, to, cc, subject, date, message_id) =
            if let Some(envelope) = msg.envelope() {
                let from = envelope.from.as_ref().and_then(|addrs| addrs.first());
                let from_name = from
                    .and_then(|a| a.name.as_ref())
                    .map(|n| String::from_utf8_lossy(n).to_string())
                    .unwrap_or_default();
                let from_address = addr_to_string(from);

                let to = extract_addresses(&envelope.to);
                let cc = extract_addresses(&envelope.cc);

                let subject = envelope
                    .subject
                    .as_ref()
                    .map(|s| String::from_utf8_lossy(s).to_string())
                    .unwrap_or_default();

                let date = envelope
                    .date
                    .as_ref()
                    .and_then(|d| {
                        let d = String::from_utf8_lossy(d);
                        chrono::DateTime::parse_from_rfc2822(&d).ok()
                    })
                    .map(|d| d.with_timezone(&Local))
                    .unwrap_or_else(Local::now);

                let message_id = envelope
                    .message_id
                    .as_ref()
                    .map(|m| String::from_utf8_lossy(m).to_string())
                    .unwrap_or_default();

                (from_name, from_address, to, cc, subject, date, message_id)
            } else {
                (
                    String::new(),
                    String::new(),
                    vec![],
                    vec![],
                    String::new(),
                    Local::now(),
                    String::new(),
                )
            };

        // Convert HTML body to text if no text body available
        let display_text = if body_text.is_empty() {
            body_html
                .as_ref()
                .and_then(|html| html2text::from_read(html.as_bytes(), 80).ok())
                .unwrap_or_default()
        } else {
            body_text
        };

        session.logout().await?;

        Ok(Email {
            uid,
            message_id,
            from_name,
            from_address,
            to,
            cc,
            subject,
            date,
            body_text: display_text,
            body_html,
            attachments,
            is_read,
            is_starred,
        })
    }

    async fn send_email(&self, to: &str, _cc: &str, subject: &str, body: &str) -> Result<()> {
        tracing::info!("Sending email to {}", to);
        let email = lettre::Message::builder()
            .from(self.email.parse()?)
            .to(to.parse()?)
            .subject(subject)
            .body(body.to_string())?;

        match &self.credential {
            ImapCredential::Password(password) => {
                let creds = lettre::transport::smtp::authentication::Credentials::new(
                    self.email.clone(),
                    password.clone(),
                );

                let mailer =
                    lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::relay("smtp.gmail.com")?
                        .credentials(creds)
                        .authentication(vec![
                            lettre::transport::smtp::authentication::Mechanism::Plain,
                        ])
                        .build();

                use lettre::AsyncTransport;
                mailer.send(email).await?;
            }
            ImapCredential::OAuth(auth) => {
                let access_token = auth.get_access_token().await?;

                let creds = lettre::transport::smtp::authentication::Credentials::new(
                    self.email.clone(),
                    access_token,
                );

                let mailer =
                    lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::relay("smtp.gmail.com")?
                        .credentials(creds)
                        .authentication(vec![
                            lettre::transport::smtp::authentication::Mechanism::Xoauth2,
                        ])
                        .build();

                use lettre::AsyncTransport;
                mailer.send(email).await?;
            }
        }

        Ok(())
    }

    async fn delete_email(&self, mailbox: &str, uid: u32) -> Result<()> {
        tracing::debug!("Deleting email uid={} from {}", uid, mailbox);
        let mut session = self.connect().await?;
        session.select(mailbox).await?;
        let _: Vec<_> = session
            .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
            .await?
            .collect()
            .await;
        let _: Vec<_> = session.expunge().await?.collect().await;
        session.logout().await?;
        Ok(())
    }

    async fn archive_email(&self, mailbox: &str, uid: u32) -> Result<()> {
        tracing::debug!("Archiving email uid={} from {}", uid, mailbox);
        let mut session = self.connect().await?;
        session.select(mailbox).await?;
        session
            .uid_mv(uid.to_string(), "[Gmail]/All Mail")
            .await?;
        session.logout().await?;
        Ok(())
    }

    async fn set_flag(
        &self,
        mailbox: &str,
        uid: u32,
        flag: EmailFlag,
        value: bool,
    ) -> Result<()> {
        tracing::debug!("Setting flag {:?}={} on uid={} in {}", flag, value, uid, mailbox);
        let mut session = self.connect().await?;
        session.select(mailbox).await?;

        let flag_str = match flag {
            EmailFlag::Seen => "\\Seen",
            EmailFlag::Starred => "\\Flagged",
            EmailFlag::Deleted => "\\Deleted",
        };

        let op = if value {
            format!("+FLAGS ({})", flag_str)
        } else {
            format!("-FLAGS ({})", flag_str)
        };

        let _: Vec<_> = session.uid_store(uid.to_string(), &op).await?.collect().await;
        session.logout().await?;
        Ok(())
    }

    async fn search_emails(&self, mailbox: &str, query: &str) -> Result<Vec<Envelope>> {
        tracing::info!("IMAP SEARCH in {} for {:?}", mailbox, query);
        let mut session = self.connect().await?;
        session.select(mailbox).await?;

        let search_query = format!(
            "OR OR SUBJECT \"{}\" FROM \"{}\" BODY \"{}\"",
            query, query, query
        );
        let uids: Vec<u32> = session.uid_search(&search_query).await?.into_iter().collect();

        if uids.is_empty() {
            session.logout().await?;
            return Ok(vec![]);
        }

        // Cap to 100 most recent UIDs (highest UID = most recent)
        let mut sorted_uids = uids;
        sorted_uids.sort_unstable();
        let capped: Vec<u32> = sorted_uids.into_iter().rev().take(100).collect();

        let uid_list: String = capped.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        let fetch_items = "(UID FLAGS ENVELOPE BODY.PEEK[TEXT]<0.200>)";
        let messages: Vec<_> = session.uid_fetch(&uid_list, fetch_items).await?.collect().await;

        let mut envelopes = Vec::new();
        for msg in messages {
            let msg = msg?;
            if let Some(env) = parse_fetch_to_envelope(&msg) {
                envelopes.push(env);
            }
        }

        session.logout().await?;
        envelopes.sort_by(|a, b| b.date.cmp(&a.date));
        tracing::debug!("IMAP SEARCH returned {} results", envelopes.len());
        Ok(envelopes)
    }

    fn provider_name(&self) -> &str {
        "Gmail"
    }
}

/// Parse a single IMAP FETCH response into an Envelope.
fn parse_fetch_to_envelope(msg: &async_imap::types::Fetch) -> Option<Envelope> {
    let uid = msg.uid.unwrap_or(0);
    if uid == 0 {
        return None;
    }

    let (is_read, is_starred) = parse_imap_flags(msg);

    let (from_name, from_address, subject, date) = if let Some(envelope) = msg.envelope() {
        let from = envelope.from.as_ref().and_then(|addrs| addrs.first());
        let from_name = from
            .and_then(|a| a.name.as_ref())
            .map(|n| String::from_utf8_lossy(n).to_string())
            .unwrap_or_default();
        let from_address = addr_to_string(from);

        let subject = envelope
            .subject
            .as_ref()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .unwrap_or_default();

        let date = envelope
            .date
            .as_ref()
            .and_then(|d| {
                let d = String::from_utf8_lossy(d);
                chrono::DateTime::parse_from_rfc2822(&d).ok()
            })
            .map(|d| d.with_timezone(&Local))
            .unwrap_or_else(Local::now);

        (from_name, from_address, subject, date)
    } else {
        (String::new(), String::new(), String::new(), Local::now())
    };

    let snippet = msg
        .text()
        .map(|t| {
            String::from_utf8_lossy(t)
                .chars()
                .take(100)
                .collect::<String>()
                .replace('\n', " ")
        })
        .unwrap_or_default();

    Some(Envelope {
        uid,
        from_name,
        from_address,
        subject,
        date,
        snippet,
        is_read,
        is_starred,
        has_attachments: false,
    })
}

/// Parse read and starred flags from an IMAP message.
fn parse_imap_flags(msg: &async_imap::types::Fetch) -> (bool, bool) {
    let is_read = msg.flags().any(|f| matches!(f, async_imap::types::Flag::Seen));
    let is_starred = msg.flags().any(|f| matches!(f, async_imap::types::Flag::Flagged));
    (is_read, is_starred)
}

fn addr_to_string(addr: Option<&ImapAddress<'_>>) -> String {
    addr.map(|a| {
        let mb = a
            .mailbox
            .as_ref()
            .map(|m| String::from_utf8_lossy(m).to_string())
            .unwrap_or_default();
        let host = a
            .host
            .as_ref()
            .map(|h| String::from_utf8_lossy(h).to_string())
            .unwrap_or_default();
        format!("{}@{}", mb, host)
    })
    .unwrap_or_default()
}

fn extract_addresses(addrs: &Option<Vec<ImapAddress<'_>>>) -> Vec<String> {
    addrs
        .as_ref()
        .map(|addrs| addrs.iter().map(|a| addr_to_string(Some(a))).collect())
        .unwrap_or_default()
}

fn extract_text_body(parsed: &mailparse::ParsedMail) -> String {
    if parsed.subparts.is_empty() && parsed.ctype.mimetype.starts_with("text/plain") {
        return parsed.get_body().unwrap_or_default();
    }

    for part in &parsed.subparts {
        let text = extract_text_body(part);
        if !text.is_empty() {
            return text;
        }
    }

    String::new()
}

fn extract_html_body(parsed: &mailparse::ParsedMail) -> Option<String> {
    if parsed.subparts.is_empty() && parsed.ctype.mimetype.starts_with("text/html") {
        return parsed.get_body().ok();
    }

    for part in &parsed.subparts {
        let html = extract_html_body(part);
        if html.is_some() {
            return html;
        }
    }

    None
}

fn extract_attachments(parsed: &mailparse::ParsedMail) -> Vec<Attachment> {
    let mut attachments = Vec::new();

    if parsed.subparts.is_empty() {
        let disposition = parsed.get_content_disposition();
        if disposition.disposition == mailparse::DispositionType::Attachment {
            let filename = disposition
                .params
                .get("filename")
                .cloned()
                .unwrap_or_else(|| "unnamed".to_string());
            attachments.push(Attachment {
                filename,
                mime_type: parsed.ctype.mimetype.clone(),
                size: parsed.get_body_raw().map(|b| b.len()).unwrap_or(0),
            });
        }
    }

    for part in &parsed.subparts {
        attachments.extend(extract_attachments(part));
    }

    attachments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_credentials() -> (String, String) {
        let email = std::env::var("TERMAIL_TEST_EMAIL")
            .expect("Set TERMAIL_TEST_EMAIL env var to your Gmail address");
        let password = std::env::var("TERMAIL_TEST_PASSWORD")
            .expect("Set TERMAIL_TEST_PASSWORD env var to your Gmail App Password");
        (email, password)
    }

    #[tokio::test]
    #[ignore]
    async fn test_gmail_login() {
        let (email, password) = test_credentials();
        let backend = ImapBackend::new(email, ImapCredential::Password(password));

        match backend.connect().await {
            Ok(mut session) => {
                println!("test_gmail_login: Login successful!");
                let _ = session.logout().await;
            }
            Err(err) => {
                println!("test_gmail_login: Login FAILED: {err}");
                for cause in err.chain().skip(1) {
                    println!("  Caused by: {cause}");
                }
                panic!("Gmail login failed");
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_gmail_list_mailboxes() {
        let (email, password) = test_credentials();
        let backend = ImapBackend::new(email, ImapCredential::Password(password));

        match backend.list_mailboxes().await {
            Ok(mailboxes) => {
                let names: Vec<&str> = mailboxes.iter().map(|m| m.name.as_str()).collect();
                println!(
                    "test_gmail_list_mailboxes: Found {} mailboxes: {}",
                    mailboxes.len(),
                    names.join(", ")
                );
                assert!(
                    names.contains(&"INBOX"),
                    "INBOX not found in mailbox list"
                );
            }
            Err(err) => {
                println!("test_gmail_list_mailboxes: FAILED: {err}");
                for cause in err.chain().skip(1) {
                    println!("  Caused by: {cause}");
                }
                panic!("Listing mailboxes failed");
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_gmail_fetch_envelopes() {
        let (email, password) = test_credentials();
        let backend = ImapBackend::new(email, ImapCredential::Password(password));

        match backend.fetch_envelopes("INBOX", None, Some(5)).await {
            Ok(envelopes) => {
                println!(
                    "test_gmail_fetch_envelopes: Fetched {} envelopes from INBOX",
                    envelopes.len()
                );
                for env in &envelopes {
                    println!("  - {}", env.subject);
                }
                assert!(!envelopes.is_empty(), "Expected at least 1 envelope");
            }
            Err(err) => {
                println!("test_gmail_fetch_envelopes: FAILED: {err}");
                for cause in err.chain().skip(1) {
                    println!("  Caused by: {cause}");
                }
                panic!("Fetching envelopes failed");
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_gmail_fetch_email() {
        let (email, password) = test_credentials();
        let backend = ImapBackend::new(email, ImapCredential::Password(password));

        let envelopes = backend
            .fetch_envelopes("INBOX", None, Some(1))
            .await
            .expect("Failed to fetch envelopes");
        assert!(!envelopes.is_empty(), "Need at least 1 email in INBOX");

        let uid = envelopes[0].uid;
        let email_msg = backend
            .fetch_email("INBOX", uid)
            .await
            .expect("Failed to fetch email");

        println!("test_gmail_fetch_email: subject = {}", email_msg.subject);
        println!("test_gmail_fetch_email: from    = {}", email_msg.from_address);
        println!("test_gmail_fetch_email: body len = {}", email_msg.body_text.len());
        println!(
            "test_gmail_fetch_email: attachments = {}",
            email_msg.attachments.len()
        );

        assert!(!email_msg.subject.is_empty(), "Subject should not be empty");
        assert!(
            !email_msg.from_address.is_empty(),
            "From address should not be empty"
        );
        assert!(
            !email_msg.body_text.is_empty(),
            "Body text should not be empty"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_gmail_set_flag() {
        let (email, password) = test_credentials();
        let backend = ImapBackend::new(email, ImapCredential::Password(password));

        let envelopes = backend
            .fetch_envelopes("INBOX", None, Some(1))
            .await
            .expect("Failed to fetch envelopes");
        assert!(!envelopes.is_empty(), "Need at least 1 email in INBOX");

        let uid = envelopes[0].uid;
        let original_is_read = envelopes[0].is_read;
        println!(
            "test_gmail_set_flag: uid={}, original is_read={}",
            uid, original_is_read
        );

        // Toggle the flag
        backend
            .set_flag("INBOX", uid, EmailFlag::Seen, !original_is_read)
            .await
            .expect("Failed to toggle Seen flag");
        println!("test_gmail_set_flag: toggled Seen to {}", !original_is_read);

        // Restore the original flag
        backend
            .set_flag("INBOX", uid, EmailFlag::Seen, original_is_read)
            .await
            .expect("Failed to restore Seen flag");
        println!(
            "test_gmail_set_flag: restored Seen to {}",
            original_is_read
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_gmail_send_email() {
        let (email, password) = test_credentials();
        let backend = ImapBackend::new(email.clone(), ImapCredential::Password(password));

        backend
            .send_email(&email, "", "TermMail Test", "Automated test from TermMail test suite.")
            .await
            .expect("Failed to send email");

        println!("test_gmail_send_email: sent test email to {}", email);
    }
}
