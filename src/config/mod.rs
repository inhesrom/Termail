pub mod schema;

use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

use self::schema::Config;

/// Returns the config directory path (~/.config/termail/)
pub fn config_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "termail")
        .context("Could not determine config directory")?;
    Ok(dirs.config_dir().to_path_buf())
}

/// Returns the data directory path (~/.local/share/termail/)
pub fn data_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "termail")
        .context("Could not determine data directory")?;
    Ok(dirs.data_dir().to_path_buf())
}

/// Load config from ~/.config/termail/config.toml
/// Creates a default config file if it doesn't exist.
pub fn load_config() -> Result<Config> {
    let config_path = config_dir()?.join("config.toml");
    load_config_from(&config_path)
}

/// Load config from a specific path. Creates a default config file if it doesn't exist.
pub fn load_config_from(config_path: &std::path::Path) -> Result<Config> {
    if !config_path.exists() {
        // Create default config
        if let Some(dir) = config_path.parent() {
            std::fs::create_dir_all(dir)
                .context("Failed to create config directory")?;
        }

        tracing::debug!("No config found, creating default at {}", config_path.display());
        let default_config = default_config_template();
        std::fs::write(config_path, default_config)
            .context("Failed to write default config")?;

        // Return empty config (no accounts yet)
        return Ok(Config {
            accounts: vec![],
            tick_rate_ms: 16,
            inbox_width_percent: 30,
        });
    }

    let content = std::fs::read_to_string(config_path)
        .context("Failed to read config file")?;
    let config: Config = toml::from_str(&content)
        .context("Failed to parse config file")?;
    tracing::debug!("Config loaded: {} accounts", config.accounts.len());
    Ok(config)
}

/// Returns the default config file template string.
pub fn default_config_template() -> &'static str {
    r#"# Termail Configuration
# Add your email accounts below.

# tick_rate_ms = 16        # UI refresh rate (default: 16ms = ~60fps)
# inbox_width_percent = 30 # Width of inbox list pane (default: 30%)

# [[accounts]]
# name = "Personal Gmail"
# email = "you@gmail.com"
# provider = "gmail"

# [[accounts]]
# name = "Work Outlook"
# email = "you@company.com"
# provider = "outlook"
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_config_creates_default() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        assert!(!config_path.exists());

        let config = load_config_from(&config_path).unwrap();
        assert!(config_path.exists());
        assert!(config.accounts.is_empty());
        assert_eq!(config.tick_rate_ms, 16);
        assert_eq!(config.inbox_width_percent, 30);
    }

    #[test]
    fn test_load_config_parses_accounts() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let toml_content = r#"
[[accounts]]
name = "Test"
email = "test@example.com"
provider = "gmail"
"#;
        std::fs::write(&config_path, toml_content).unwrap();

        let config = load_config_from(&config_path).unwrap();
        assert_eq!(config.accounts.len(), 1);
        assert_eq!(config.accounts[0].email, "test@example.com");
    }

    #[test]
    fn test_load_config_invalid_toml() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "this is {{not valid toml!!!").unwrap();

        assert!(load_config_from(&config_path).is_err());
    }
}
