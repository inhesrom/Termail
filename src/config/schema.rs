use serde::Deserialize;

use crate::models::account::Account;

/// Top-level configuration loaded from ~/.config/termail/config.toml
#[derive(Debug, Deserialize)]
pub struct Config {
    pub accounts: Vec<Account>,
    #[serde(default = "default_tick_rate")]
    pub tick_rate_ms: u64,
    #[serde(default = "default_inbox_width")]
    pub inbox_width_percent: u16,
}

fn default_tick_rate() -> u64 {
    16
}

fn default_inbox_width() -> u16 {
    30
}
