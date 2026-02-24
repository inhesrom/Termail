/// Represents a mail folder/mailbox (e.g. INBOX, Sent, Drafts).
#[derive(Debug, Clone)]
pub struct Mailbox {
    pub name: String,
    pub path: String,
    pub total: u32,
    pub unread: u32,
}
