pub mod search;
pub mod sqlite;
pub mod sync;

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Cache store managing the SQLite connection.
pub struct CacheStore {
    pub conn: Connection,
}

impl CacheStore {
    /// Open or create the cache database at the given path.
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)
            .context("Failed to create data directory")?;

        let db_path = data_dir.join("cache.db");
        let conn = Connection::open(&db_path)
            .context("Failed to open cache database")?;

        // Enable WAL mode for better concurrent read/write performance
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        sqlite::init_db(&conn)?;

        Ok(Self { conn })
    }
}
