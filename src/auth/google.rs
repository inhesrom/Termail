use std::path::Path;

use anyhow::{Context, Result};

/// Google OAuth2 scopes needed for Gmail IMAP + SMTP access.
const GMAIL_SCOPES: &[&str] = &["https://mail.google.com/"];

/// Manages Google OAuth2 authentication.
/// Tokens are persisted to disk so the user only authenticates once.
pub struct GoogleAuth {
    authenticator: yup_oauth2::authenticator::DefaultAuthenticator,
}

impl GoogleAuth {
    /// Build a Google OAuth2 authenticator using installed application flow.
    ///
    /// The caller must provide valid OAuth2 client credentials.
    pub async fn new(
        data_dir: &Path,
        client_id: &str,
        client_secret: &str,
    ) -> Result<Self> {
        tracing::debug!("Building Google OAuth authenticator");
        let secret = yup_oauth2::ApplicationSecret {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            auth_uri: "https://accounts.google.com/o/oauth2/auth".into(),
            token_uri: "https://oauth2.googleapis.com/token".into(),
            redirect_uris: vec!["urn:ietf:wg:oauth:2.0:oob".into()],
            ..Default::default()
        };

        let token_path = data_dir.join("google_token.json");

        let authenticator = yup_oauth2::InstalledFlowAuthenticator::builder(
            secret,
            yup_oauth2::InstalledFlowReturnMethod::Interactive,
        )
        .persist_tokens_to_disk(&token_path)
        .build()
        .await
        .context("Failed to build Google OAuth2 authenticator")?;

        Ok(Self { authenticator })
    }

    /// Get a fresh access token for IMAP/SMTP XOAUTH2.
    pub async fn get_access_token(&self) -> Result<String> {
        tracing::debug!("Requesting Google access token");
        let token = self
            .authenticator
            .token(GMAIL_SCOPES)
            .await
            .context("Failed to get Google access token")?;

        Ok(token
            .token()
            .context("Access token was empty")?
            .to_string())
    }
}

/// Build the XOAUTH2 SASL string for IMAP/SMTP authentication.
/// Format: base64("user=<email>\x01auth=Bearer <token>\x01\x01")
pub fn build_xoauth2_string(email: &str, access_token: &str) -> String {
    use base64::Engine;
    let auth_string = format!("user={}\x01auth=Bearer {}\x01\x01", email, access_token);
    base64::engine::general_purpose::STANDARD.encode(auth_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_xoauth2_string() {
        use base64::Engine;

        let result = build_xoauth2_string("user@example.com", "token123");
        let expected_raw = "user=user@example.com\x01auth=Bearer token123\x01\x01";
        let expected = base64::engine::general_purpose::STANDARD.encode(expected_raw);
        assert_eq!(result, expected);

        // Verify round-trip: decode and check raw content
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&result)
            .expect("should be valid base64");
        assert_eq!(String::from_utf8(decoded).unwrap(), expected_raw);
    }
}
