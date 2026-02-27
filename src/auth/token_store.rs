use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// On-disk credential file layout:
///
/// ```toml
/// [credentials]
/// "user@gmail.com" = "app-password-here"
/// ```
#[derive(Debug, Default, Serialize, Deserialize)]
struct CredentialFile {
    #[serde(default)]
    credentials: HashMap<String, String>,
}

/// Path to the credentials file: `~/.local/share/termail/credentials.toml`
fn credentials_path() -> Result<PathBuf> {
    let dir = crate::config::data_dir()?;
    Ok(dir.join("credentials.toml"))
}

/// Read and parse the credential file, returning a default if it doesn't exist.
fn load() -> Result<CredentialFile> {
    let path = credentials_path()?;
    if !path.exists() {
        return Ok(CredentialFile::default());
    }
    let contents = std::fs::read_to_string(&path)
        .context("Failed to read credentials file")?;
    let creds: CredentialFile = toml::from_str(&contents)
        .context("Failed to parse credentials file")?;
    Ok(creds)
}

/// Write the credential file back to disk, creating parent dirs and setting
/// permissions to 0600 on Unix.
fn save(creds: &CredentialFile) -> Result<()> {
    let path = credentials_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create credentials directory")?;
    }
    let contents = toml::to_string_pretty(creds)
        .context("Failed to serialize credentials")?;
    std::fs::write(&path, &contents)
        .context("Failed to write credentials file")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms)
            .context("Failed to set credentials file permissions")?;
    }

    Ok(())
}

/// Store a password in the local credentials file.
pub fn store_token(account: &str, token: &str) -> Result<()> {
    tracing::debug!("Storing credentials for {}", account);
    let mut creds = load()?;
    creds.credentials.insert(account.to_string(), token.to_string());
    save(&creds)?;
    Ok(())
}

/// Retrieve a password from the local credentials file.
pub fn get_token(account: &str) -> Result<Option<String>> {
    tracing::debug!("Looking up credentials for {}", account);
    let creds = load()?;
    Ok(creds.credentials.get(account).cloned())
}

/// Delete a password from the local credentials file.
pub fn delete_token(account: &str) -> Result<()> {
    tracing::debug!("Deleting credentials for {}", account);
    let mut creds = load()?;
    creds.credentials.remove(account);
    save(&creds)?;
    Ok(())
}
