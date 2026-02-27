use std::io::{self, Write};

use anyhow::{Context, Result};

use crate::auth::token_store;
use crate::config;
use crate::models::account::{Account, Provider};

/// Run the interactive account setup wizard.
pub async fn run_setup() -> Result<()> {
    println!("Termail Account Setup");
    println!("=====================\n");

    let name = prompt("Display name (e.g. \"Personal Gmail\"): ")?;
    let email = prompt("Email address: ")?;

    let provider = detect_provider(&email);
    match &provider {
        Provider::Gmail => println!("\nDetected provider: Gmail"),
    }

    println!("\nTo create an App Password:");
    println!("  1. Go to myaccount.google.com");
    println!("  2. Security → 2-Step Verification");
    println!("  3. App passwords → Create one");
    println!();

    let password = prompt("App Password: ")?;

    // Store password in OS keyring
    token_store::store_token(&email, &password)?;
    println!("\nPassword saved to local credentials file.");

    // Save account to config (no secrets in config file)
    let account = Account {
        name,
        email,
        provider,
        client_id: None,
        client_secret: None,
    };
    append_account_to_config(&account)?;

    println!("\nAccount added! Run `termail` to start.");
    Ok(())
}

/// Prompt the user for input on stdout/stdin.
fn prompt(message: &str) -> Result<String> {
    print!("{}", message);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Auto-detect the email provider from the domain.
/// TODO: detect provider from domain once multiple providers are supported.
fn detect_provider(_email: &str) -> Provider {
    Provider::Gmail
}

/// Remove all accounts from the config file, resetting it to the default template.
pub fn remove_account_from_config() -> Result<()> {
    tracing::debug!("Removing all accounts from config");
    let config_path = config::config_dir()?.join("config.toml");
    remove_account_from_config_at(&config_path)
}

/// Remove all accounts from a specific config file path.
pub fn remove_account_from_config_at(config_path: &std::path::Path) -> Result<()> {
    std::fs::write(config_path, config::default_config_template())
        .context("Failed to write config file")?;
    Ok(())
}

/// Append an [[accounts]] entry to the config file.
pub fn append_account_to_config(account: &Account) -> Result<()> {
    tracing::debug!("Appending account {} to config", account.email);
    let config_dir = config::config_dir()?;
    append_account_to_config_at(account, &config_dir)
}

/// Append an [[accounts]] entry to a config file in the given directory.
pub fn append_account_to_config_at(account: &Account, config_dir: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let config_path = config_dir.join("config.toml");

    // Ensure the config file exists
    if !config_path.exists() {
        let _ = config::load_config_from(&config_path);
    }

    // Serialize using serde/toml for correct escaping
    #[derive(serde::Serialize)]
    struct Wrapper<'a> {
        accounts: [&'a Account; 1],
    }
    let entry = toml::to_string(&Wrapper { accounts: [account] })
        .context("Failed to serialize account entry")?;

    let mut existing = std::fs::read_to_string(&config_path).unwrap_or_default();
    existing.push('\n');
    existing.push_str(&entry);
    std::fs::write(&config_path, existing).context("Failed to write config file")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_account(name: &str, email: &str) -> Account {
        Account {
            name: name.into(),
            email: email.into(),
            provider: Provider::Gmail,
            client_id: None,
            client_secret: None,
        }
    }

    #[test]
    fn test_remove_account_resets_config() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        // Write a config with an account
        std::fs::write(&config_path, "[[accounts]]\nname = \"X\"\nemail = \"x@x.com\"\nprovider = \"gmail\"\n").unwrap();

        remove_account_from_config_at(&config_path).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(!content.contains("[[accounts]]") || content.contains("# [[accounts]]"));
        assert!(content.contains("# Termail Configuration"));
    }

    #[test]
    fn test_append_account_to_config() {
        let tmp = TempDir::new().unwrap();
        let account = test_account("Personal", "me@gmail.com");

        append_account_to_config_at(&account, tmp.path()).unwrap();

        let content = std::fs::read_to_string(tmp.path().join("config.toml")).unwrap();
        assert!(content.contains("[[accounts]]"));
        assert!(content.contains("me@gmail.com"));
        assert!(content.contains("Personal"));
    }

    #[test]
    fn test_append_preserves_existing() {
        let tmp = TempDir::new().unwrap();
        let acct1 = test_account("First", "first@example.com");
        let acct2 = test_account("Second", "second@example.com");

        append_account_to_config_at(&acct1, tmp.path()).unwrap();
        append_account_to_config_at(&acct2, tmp.path()).unwrap();

        let content = std::fs::read_to_string(tmp.path().join("config.toml")).unwrap();
        assert!(content.contains("first@example.com"));
        assert!(content.contains("second@example.com"));

        // Verify both are parseable
        let config: config::schema::Config = toml::from_str(&content).unwrap();
        assert_eq!(config.accounts.len(), 2);
    }
}
