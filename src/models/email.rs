use chrono::{DateTime, Local};

/// A full email with headers and parsed body content.
#[derive(Debug, Clone)]
pub struct Email {
    pub uid: u32,
    pub message_id: String,
    pub from_name: String,
    pub from_address: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub date: DateTime<Local>,
    pub body_text: String,
    pub body_html: Option<String>,
    pub attachments: Vec<Attachment>,
    pub is_read: bool,
    pub is_starred: bool,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,
    pub mime_type: String,
    pub size: usize,
}
