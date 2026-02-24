/// Supported email provider types.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Gmail,
    // Future: Exchange, Imap, Icloud, Protonmail, Jmap
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
