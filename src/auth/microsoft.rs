use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;
use yup_oauth2::authenticator_delegate::{DeviceAuthResponse, DeviceFlowDelegate};

use crate::backend::imap::OAuthTokenSource;

/// Microsoft OAuth2 scopes for Outlook IMAP + SMTP.
const OUTLOOK_SCOPES: &[&str] = &[
    "https://outlook.office365.com/IMAP.AccessAsUser.All",
    "https://outlook.office365.com/SMTP.Send",
    "offline_access",
];

/// Default Azure AD multi-tenant public native app client ID.
/// Users can override per-account via `Account.client_id`.
/// You must register your own app in Azure portal with:
///   - "Allow public client flows" enabled (device code grant)
///   - API permissions: IMAP.AccessAsUser.All, SMTP.Send
pub const DEFAULT_CLIENT_ID: &str = "YOUR_AZURE_CLIENT_ID_HERE";

/// Device code information to display to the user during auth.
#[derive(Debug, Clone)]
pub struct DeviceCodeInfo {
    pub verification_uri: String,
    pub user_code: String,
}

/// Custom delegate that relays device code info to the TUI via an mpsc channel.
struct TuiDeviceFlowDelegate {
    code_tx: mpsc::UnboundedSender<DeviceCodeInfo>,
}

impl DeviceFlowDelegate for TuiDeviceFlowDelegate {
    fn present_user_code<'a>(
        &'a self,
        device_auth_resp: &'a DeviceAuthResponse,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let info = DeviceCodeInfo {
            verification_uri: device_auth_resp.verification_uri.clone(),
            user_code: device_auth_resp.user_code.clone(),
        };
        let _ = self.code_tx.send(info);
        Box::pin(async {})
    }
}

/// Manages Microsoft OAuth2 authentication via device code flow.
/// Tokens are persisted to disk for automatic refresh on restart.
pub struct MicrosoftAuth {
    authenticator: yup_oauth2::authenticator::DefaultAuthenticator,
}

impl MicrosoftAuth {
    /// Build token file path for a given email.
    fn token_path(data_dir: &Path, email: &str) -> std::path::PathBuf {
        let sanitized = email.replace('@', "_at_").replace('.', "_");
        data_dir.join(format!("microsoft_token_{}.json", sanitized))
    }

    fn app_secret(client_id: &str) -> yup_oauth2::ApplicationSecret {
        yup_oauth2::ApplicationSecret {
            client_id: client_id.to_string(),
            client_secret: String::new(),
            auth_uri: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
            token_uri: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
            redirect_uris: vec![],
            ..Default::default()
        }
    }

    /// Create a new Microsoft OAuth2 authenticator for initial setup.
    /// The device code info is sent via the returned receiver so the TUI
    /// can display it to the user.
    pub async fn new_with_device_code(
        data_dir: &Path,
        email: &str,
        client_id: &str,
    ) -> Result<(Self, mpsc::UnboundedReceiver<DeviceCodeInfo>)> {
        let (code_tx, code_rx) = mpsc::unbounded_channel();

        let secret = Self::app_secret(client_id);
        let token_path = Self::token_path(data_dir, email);

        let authenticator = yup_oauth2::DeviceFlowAuthenticator::builder(secret)
            .device_code_url("https://login.microsoftonline.com/common/oauth2/v2.0/devicecode")
            .flow_delegate(Box::new(TuiDeviceFlowDelegate { code_tx }))
            .persist_tokens_to_disk(&token_path)
            .build()
            .await
            .context("Failed to build Microsoft device code authenticator")?;

        Ok((Self { authenticator }, code_rx))
    }

    /// Load an existing Microsoft OAuth2 authenticator from cached tokens.
    /// Used at app startup when the user has already authenticated.
    pub async fn load(data_dir: &Path, email: &str, client_id: &str) -> Result<Self> {
        let secret = Self::app_secret(client_id);
        let token_path = Self::token_path(data_dir, email);

        if !token_path.exists() {
            anyhow::bail!("No cached Microsoft token found for {}", email);
        }

        let authenticator = yup_oauth2::DeviceFlowAuthenticator::builder(secret)
            .device_code_url("https://login.microsoftonline.com/common/oauth2/v2.0/devicecode")
            .persist_tokens_to_disk(&token_path)
            .build()
            .await
            .context("Failed to load Microsoft authenticator from cached tokens")?;

        Ok(Self { authenticator })
    }

    /// Synchronous wrapper for `load()` — used in `create_backend()` which
    /// runs outside of an async context at startup.
    pub fn load_blocking(data_dir: &Path, email: &str, client_id: &str) -> Result<Self> {
        // We're already inside a tokio runtime (main is #[tokio::main]),
        // so use block_in_place + a nested runtime to avoid panic.
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(Self::load(data_dir, email, client_id))
        })
    }

    /// Get a fresh access token, refreshing automatically if needed.
    pub async fn get_access_token(&self) -> Result<String> {
        tracing::debug!("Requesting Microsoft access token");
        let token = self
            .authenticator
            .token(OUTLOOK_SCOPES)
            .await
            .context("Failed to get Microsoft access token")?;

        Ok(token
            .token()
            .context("Microsoft access token was empty")?
            .to_string())
    }

    /// Delete the cached token file for an account.
    pub fn delete_token_file(data_dir: &Path, email: &str) -> Result<()> {
        let path = Self::token_path(data_dir, email);
        if path.exists() {
            std::fs::remove_file(&path)
                .context("Failed to delete Microsoft token file")?;
        }
        Ok(())
    }
}

#[async_trait]
impl OAuthTokenSource for MicrosoftAuth {
    async fn get_access_token(&self) -> Result<String> {
        self.get_access_token().await
    }
}
