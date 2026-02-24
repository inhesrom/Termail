use anyhow::{Context, Result};

const SERVICE_NAME: &str = "termail";

/// Store a refresh token securely in the OS keyring.
pub fn store_token(account: &str, token: &str) -> Result<()> {
    tracing::debug!("Storing credentials in OS keyring for {}", account);
    let entry = keyring::Entry::new(SERVICE_NAME, account)
        .context("Failed to create keyring entry")?;
    entry
        .set_password(token)
        .context("Failed to store token in keyring")?;
    Ok(())
}

/// Retrieve a refresh token from the OS keyring.
pub fn get_token(account: &str) -> Result<Option<String>> {
    tracing::debug!("Looking up credentials in OS keyring for {}", account);
    let entry = keyring::Entry::new(SERVICE_NAME, account)
        .context("Failed to create keyring entry")?;
    match entry.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => {
            tracing::debug!("No credentials found in keyring for {}", account);
            Ok(None)
        }
        Err(e) => Err(anyhow::anyhow!("Failed to get token from keyring: {}", e)),
    }
}

/// Delete a token from the OS keyring.
pub fn delete_token(account: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE_NAME, account)
        .context("Failed to create keyring entry")?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("Failed to delete token from keyring: {}", e)),
    }
}
