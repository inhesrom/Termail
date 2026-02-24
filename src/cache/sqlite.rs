use anyhow::{Context, Result};
use chrono::{Local, TimeZone};
use rusqlite::{Connection, params};

use crate::models::email::Email;
use crate::models::envelope::Envelope;

/// Initialize the SQLite database schema.
pub fn init_db(conn: &Connection) -> Result<()> {
    tracing::debug!("Initializing SQLite cache schema");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS envelopes (
            uid          INTEGER PRIMARY KEY,
            account      TEXT NOT NULL,
            mailbox      TEXT NOT NULL,
            from_name    TEXT NOT NULL,
            from_address TEXT NOT NULL,
            subject      TEXT NOT NULL,
            date         INTEGER NOT NULL,
            snippet      TEXT NOT NULL DEFAULT '',
            is_read      INTEGER NOT NULL DEFAULT 0,
            is_starred   INTEGER NOT NULL DEFAULT 0,
            has_attachments INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS email_bodies (
            uid          INTEGER PRIMARY KEY,
            account      TEXT NOT NULL,
            message_id   TEXT NOT NULL DEFAULT '',
            to_addrs     TEXT NOT NULL DEFAULT '',
            cc_addrs     TEXT NOT NULL DEFAULT '',
            body_text    TEXT NOT NULL DEFAULT '',
            body_html    TEXT,
            fetched_at   INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sync_state (
            account      TEXT NOT NULL,
            mailbox      TEXT NOT NULL,
            last_uid     INTEGER NOT NULL DEFAULT 0,
            uid_validity INTEGER NOT NULL DEFAULT 0,
            last_sync    INTEGER NOT NULL,
            PRIMARY KEY (account, mailbox)
        );

        CREATE INDEX IF NOT EXISTS idx_envelopes_account_mailbox
            ON envelopes(account, mailbox);
        CREATE INDEX IF NOT EXISTS idx_envelopes_date
            ON envelopes(date DESC);
        ",
    )
    .context("Failed to initialize database schema")?;
    Ok(())
}

/// Insert or replace an envelope in the cache.
pub fn upsert_envelope(conn: &Connection, account: &str, mailbox: &str, env: &Envelope) -> Result<()> {
    tracing::debug!("Caching envelope uid={}", env.uid);
    conn.execute(
        "INSERT OR REPLACE INTO envelopes
            (uid, account, mailbox, from_name, from_address, subject, date, snippet, is_read, is_starred, has_attachments)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            env.uid,
            account,
            mailbox,
            env.from_name,
            env.from_address,
            env.subject,
            env.date.timestamp(),
            env.snippet,
            env.is_read as i32,
            env.is_starred as i32,
            env.has_attachments as i32,
        ],
    )?;
    Ok(())
}

/// Load all envelopes for an account+mailbox, ordered by date descending.
pub fn load_envelopes(conn: &Connection, account: &str, mailbox: &str) -> Result<Vec<Envelope>> {
    let mut stmt = conn.prepare(
        "SELECT uid, from_name, from_address, subject, date, snippet, is_read, is_starred, has_attachments
         FROM envelopes
         WHERE account = ?1 AND mailbox = ?2
         ORDER BY date DESC",
    )?;

    let envelopes = stmt
        .query_map(params![account, mailbox], |row| {
            let timestamp: i64 = row.get(4)?;
            let date = Local
                .timestamp_opt(timestamp, 0)
                .single()
                .unwrap_or_else(Local::now);

            Ok(Envelope {
                uid: row.get(0)?,
                from_name: row.get(1)?,
                from_address: row.get(2)?,
                subject: row.get(3)?,
                date,
                snippet: row.get(5)?,
                is_read: row.get::<_, i32>(6)? != 0,
                is_starred: row.get::<_, i32>(7)? != 0,
                has_attachments: row.get::<_, i32>(8)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    tracing::debug!("Loaded {} cached envelopes for {}/{}", envelopes.len(), account, mailbox);
    Ok(envelopes)
}

/// Cache a full email body.
pub fn cache_email_body(conn: &Connection, account: &str, email: &Email) -> Result<()> {
    tracing::debug!("Caching email body uid={}", email.uid);
    conn.execute(
        "INSERT OR REPLACE INTO email_bodies
            (uid, account, message_id, to_addrs, cc_addrs, body_text, body_html, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            email.uid,
            account,
            email.message_id,
            email.to.join(", "),
            email.cc.join(", "),
            email.body_text,
            email.body_html,
            Local::now().timestamp(),
        ],
    )?;
    Ok(())
}

/// Load a cached email body. Returns None if not cached.
pub fn load_email_body(conn: &Connection, account: &str, uid: u32) -> Result<Option<Email>> {
    let mut stmt = conn.prepare(
        "SELECT e.uid, e.from_name, e.from_address, e.subject, e.date, e.is_read, e.is_starred,
                b.message_id, b.to_addrs, b.cc_addrs, b.body_text, b.body_html
         FROM envelopes e
         JOIN email_bodies b ON e.uid = b.uid AND e.account = b.account
         WHERE e.account = ?1 AND e.uid = ?2",
    )?;

    let email = stmt
        .query_row(params![account, uid], |row| {
            let timestamp: i64 = row.get(4)?;
            let date = Local
                .timestamp_opt(timestamp, 0)
                .single()
                .unwrap_or_else(Local::now);

            let to_str: String = row.get(8)?;
            let cc_str: String = row.get(9)?;

            Ok(Email {
                uid: row.get(0)?,
                from_name: row.get(1)?,
                from_address: row.get(2)?,
                subject: row.get(3)?,
                date,
                message_id: row.get(7)?,
                to: to_str.split(", ").map(String::from).collect(),
                cc: if cc_str.is_empty() {
                    vec![]
                } else {
                    cc_str.split(", ").map(String::from).collect()
                },
                body_text: row.get(10)?,
                body_html: row.get(11)?,
                attachments: vec![], // Attachments stored separately if needed
                is_read: row.get::<_, i32>(5)? != 0,
                is_starred: row.get::<_, i32>(6)? != 0,
            })
        })
        .optional()?;

    if email.is_some() {
        tracing::debug!("Cache hit for email body uid={}", uid);
    } else {
        tracing::debug!("Cache miss for email body uid={}", uid);
    }
    Ok(email)
}

/// Update sync state for an account+mailbox.
pub fn update_sync_state(
    conn: &Connection,
    account: &str,
    mailbox: &str,
    last_uid: u32,
    uid_validity: u32,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO sync_state (account, mailbox, last_uid, uid_validity, last_sync)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![account, mailbox, last_uid, uid_validity, Local::now().timestamp()],
    )?;
    Ok(())
}

/// Get sync state for an account+mailbox.
pub fn get_sync_state(conn: &Connection, account: &str, mailbox: &str) -> Result<Option<(u32, u32)>> {
    let mut stmt = conn.prepare(
        "SELECT last_uid, uid_validity FROM sync_state WHERE account = ?1 AND mailbox = ?2",
    )?;

    let result = stmt
        .query_row(params![account, mailbox], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?))
        })
        .optional()?;

    Ok(result)
}

/// Delete an envelope from the cache.
pub fn delete_envelope(conn: &Connection, uid: u32) -> Result<()> {
    tracing::debug!("Deleting cached envelope uid={}", uid);
    conn.execute("DELETE FROM envelopes WHERE uid = ?1", params![uid])?;
    conn.execute("DELETE FROM email_bodies WHERE uid = ?1", params![uid])?;
    Ok(())
}

/// Update a flag on a cached envelope.
pub fn update_envelope_flag(conn: &Connection, uid: u32, flag: &str, value: bool) -> Result<()> {
    tracing::debug!("Updating cached flag {}={} for uid={}", flag, value, uid);
    let column = match flag {
        "seen" => "is_read",
        "starred" => "is_starred",
        _ => return Ok(()),
    };
    let sql = format!("UPDATE envelopes SET {} = ?1 WHERE uid = ?2", column);
    conn.execute(&sql, params![value as i32, uid])?;
    Ok(())
}

/// Trait to add `.optional()` to rusqlite query results.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::envelope::Envelope;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    fn test_envelope(uid: u32) -> Envelope {
        Envelope {
            uid,
            from_name: "Test User".into(),
            from_address: "test@example.com".into(),
            subject: format!("Test subject {}", uid),
            date: Local::now(),
            snippet: "Test snippet".into(),
            is_read: false,
            is_starred: false,
            has_attachments: false,
        }
    }

    #[test]
    fn test_init_db() {
        let conn = test_conn();
        // Should be able to init again without error (IF NOT EXISTS)
        init_db(&conn).unwrap();
    }

    #[test]
    fn test_upsert_and_load_envelope() {
        let conn = test_conn();
        let env = test_envelope(1);
        upsert_envelope(&conn, "test@gmail.com", "INBOX", &env).unwrap();

        let loaded = load_envelopes(&conn, "test@gmail.com", "INBOX").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].uid, 1);
        assert_eq!(loaded[0].from_name, "Test User");
        assert_eq!(loaded[0].subject, "Test subject 1");
    }

    #[test]
    fn test_upsert_replaces() {
        let conn = test_conn();
        let mut env = test_envelope(1);
        upsert_envelope(&conn, "test@gmail.com", "INBOX", &env).unwrap();

        env.subject = "Updated subject".into();
        upsert_envelope(&conn, "test@gmail.com", "INBOX", &env).unwrap();

        let loaded = load_envelopes(&conn, "test@gmail.com", "INBOX").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].subject, "Updated subject");
    }

    #[test]
    fn test_delete_envelope() {
        let conn = test_conn();
        upsert_envelope(&conn, "test@gmail.com", "INBOX", &test_envelope(1)).unwrap();
        upsert_envelope(&conn, "test@gmail.com", "INBOX", &test_envelope(2)).unwrap();

        delete_envelope(&conn, 1).unwrap();

        let loaded = load_envelopes(&conn, "test@gmail.com", "INBOX").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].uid, 2);
    }

    #[test]
    fn test_update_flag() {
        let conn = test_conn();
        upsert_envelope(&conn, "test@gmail.com", "INBOX", &test_envelope(1)).unwrap();

        update_envelope_flag(&conn, 1, "seen", true).unwrap();
        let loaded = load_envelopes(&conn, "test@gmail.com", "INBOX").unwrap();
        assert!(loaded[0].is_read);

        update_envelope_flag(&conn, 1, "starred", true).unwrap();
        let loaded = load_envelopes(&conn, "test@gmail.com", "INBOX").unwrap();
        assert!(loaded[0].is_starred);
    }

    #[test]
    fn test_sync_state() {
        let conn = test_conn();

        // No state initially
        let state = get_sync_state(&conn, "test@gmail.com", "INBOX").unwrap();
        assert!(state.is_none());

        // Set state
        update_sync_state(&conn, "test@gmail.com", "INBOX", 42, 12345).unwrap();
        let state = get_sync_state(&conn, "test@gmail.com", "INBOX").unwrap();
        assert_eq!(state, Some((42, 12345)));

        // Update state
        update_sync_state(&conn, "test@gmail.com", "INBOX", 99, 12345).unwrap();
        let state = get_sync_state(&conn, "test@gmail.com", "INBOX").unwrap();
        assert_eq!(state, Some((99, 12345)));
    }

    #[test]
    fn test_load_envelopes_ordered_by_date() {
        let conn = test_conn();
        let now = Local::now();

        let mut env1 = test_envelope(1);
        env1.date = now - chrono::Duration::hours(2);
        let mut env2 = test_envelope(2);
        env2.date = now;
        let mut env3 = test_envelope(3);
        env3.date = now - chrono::Duration::hours(1);

        upsert_envelope(&conn, "test@gmail.com", "INBOX", &env1).unwrap();
        upsert_envelope(&conn, "test@gmail.com", "INBOX", &env2).unwrap();
        upsert_envelope(&conn, "test@gmail.com", "INBOX", &env3).unwrap();

        let loaded = load_envelopes(&conn, "test@gmail.com", "INBOX").unwrap();
        assert_eq!(loaded.len(), 3);
        // Should be ordered by date DESC
        assert_eq!(loaded[0].uid, 2);
        assert_eq!(loaded[1].uid, 3);
        assert_eq!(loaded[2].uid, 1);
    }

    #[test]
    fn test_cache_and_load_email_body() {
        let conn = test_conn();

        // Need an envelope first
        upsert_envelope(&conn, "test@gmail.com", "INBOX", &test_envelope(1)).unwrap();

        let email = crate::models::email::Email {
            uid: 1,
            message_id: "<test@example.com>".into(),
            from_name: "Test User".into(),
            from_address: "test@example.com".into(),
            to: vec!["me@gmail.com".into()],
            cc: vec![],
            subject: "Test subject 1".into(),
            date: Local::now(),
            body_text: "Hello, this is a test email body.".into(),
            body_html: Some("<p>Hello</p>".into()),
            attachments: vec![],
            is_read: false,
            is_starred: false,
        };

        cache_email_body(&conn, "test@gmail.com", &email).unwrap();

        let loaded = load_email_body(&conn, "test@gmail.com", 1).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.uid, 1);
        assert_eq!(loaded.body_text, "Hello, this is a test email body.");
        assert_eq!(loaded.body_html, Some("<p>Hello</p>".into()));
    }

    #[test]
    fn test_load_email_body_not_found() {
        let conn = test_conn();
        let loaded = load_email_body(&conn, "test@gmail.com", 999).unwrap();
        assert!(loaded.is_none());
    }
}
