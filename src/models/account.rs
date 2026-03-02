/// Supported email provider types.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Gmail,
    Outlook,
}

impl Provider {
    /// Returns the IMAP/SMTP configuration for this provider.
    pub fn config(&self) -> ProviderConfig {
        match self {
            Provider::Gmail => ProviderConfig {
                imap_host: "imap.gmail.com".into(),
                imap_port: 993,
                smtp_host: "smtp.gmail.com".into(),
                smtp_port: 465,
                smtp_tls_mode: SmtpTlsMode::Implicit,
                archive_folder: "[Gmail]/All Mail".into(),
            },
            Provider::Outlook => ProviderConfig {
                imap_host: "outlook.office365.com".into(),
                imap_port: 993,
                smtp_host: "smtp.office365.com".into(),
                smtp_port: 587,
                smtp_tls_mode: SmtpTlsMode::Starttls,
                archive_folder: "Archive".into(),
            },
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Provider::Gmail => "Gmail",
            Provider::Outlook => "Outlook 365",
        }
    }
}

/// How SMTP TLS should be established.
#[derive(Debug, Clone)]
pub enum SmtpTlsMode {
    /// Port 465 — TLS from the start.
    Implicit,
    /// Port 587 — plaintext then STARTTLS upgrade.
    Starttls,
}

/// IMAP/SMTP server configuration for a provider.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_tls_mode: SmtpTlsMode,
    pub archive_folder: String,
}

/// Account configuration loaded from config file.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Account {
    pub name: String,
    pub email: String,
    pub provider: Provider,
    /// Optional per-account OAuth client ID override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Optional per-account OAuth client secret override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}
