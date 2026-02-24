pub mod imap;

use anyhow::Result;
use async_trait::async_trait;

use crate::models::email::Email;
use crate::models::envelope::Envelope;
use crate::models::mailbox::Mailbox;

/// Flag operations supported on emails.
#[derive(Debug, Clone)]
pub enum EmailFlag {
    Seen,
    Starred,
    Deleted,
}

/// Core email backend trait. Implement this for each provider.
#[async_trait]
pub trait EmailBackend: Send + Sync {
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>>;
    async fn fetch_envelopes(
        &self,
        mailbox: &str,
        since_uid: Option<u32>,
        limit: Option<u32>,
    ) -> Result<Vec<Envelope>>;
    async fn fetch_email(&self, mailbox: &str, uid: u32) -> Result<Email>;
    async fn send_email(&self, to: &str, cc: &str, subject: &str, body: &str) -> Result<()>;
    async fn delete_email(&self, mailbox: &str, uid: u32) -> Result<()>;
    async fn archive_email(&self, mailbox: &str, uid: u32) -> Result<()>;
    async fn set_flag(
        &self,
        mailbox: &str,
        uid: u32,
        flag: EmailFlag,
        value: bool,
    ) -> Result<()>;
    fn provider_name(&self) -> &str;
}
