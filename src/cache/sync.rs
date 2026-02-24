use anyhow::Result;
use rusqlite::Connection;

use crate::cache::sqlite;
use crate::models::envelope::Envelope;

/// Sync envelopes to the local cache. Handles incremental updates.
/// Returns the envelopes that were added/updated.
pub fn sync_envelopes(
    conn: &Connection,
    account: &str,
    mailbox: &str,
    new_envelopes: &[Envelope],
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    for env in new_envelopes {
        sqlite::upsert_envelope(&tx, account, mailbox, env)?;
    }

    // Update sync state with the highest UID we've seen
    if let Some(max_uid) = new_envelopes.iter().map(|e| e.uid).max() {
        sqlite::update_sync_state(&tx, account, mailbox, max_uid, 0)?;
    }

    tx.commit()?;
    Ok(())
}

/// Get the last synced UID for incremental fetching.
pub fn get_last_uid(conn: &Connection, account: &str, mailbox: &str) -> Result<Option<u32>> {
    Ok(sqlite::get_sync_state(conn, account, mailbox)?.map(|(uid, _)| uid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use crate::models::envelope::Envelope;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        sqlite::init_db(&conn).unwrap();
        conn
    }

    fn test_envelope(uid: u32) -> Envelope {
        Envelope {
            uid,
            from_name: "Sender".into(),
            from_address: "sender@example.com".into(),
            subject: format!("Subject {}", uid),
            date: Local::now(),
            snippet: "snippet".into(),
            is_read: false,
            is_starred: false,
            has_attachments: false,
        }
    }

    #[test]
    fn test_sync_envelopes_basic() {
        let conn = test_conn();
        let envelopes = vec![test_envelope(1), test_envelope(2)];
        sync_envelopes(&conn, "acct", "INBOX", &envelopes).unwrap();

        let loaded = sqlite::load_envelopes(&conn, "acct", "INBOX").unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn test_sync_updates_last_uid() {
        let conn = test_conn();
        let envelopes = vec![test_envelope(5), test_envelope(10), test_envelope(3)];
        sync_envelopes(&conn, "acct", "INBOX", &envelopes).unwrap();

        let last = get_last_uid(&conn, "acct", "INBOX").unwrap();
        assert_eq!(last, Some(10));
    }

    #[test]
    fn test_sync_empty_envelopes() {
        let conn = test_conn();
        sync_envelopes(&conn, "acct", "INBOX", &[]).unwrap();

        let last = get_last_uid(&conn, "acct", "INBOX").unwrap();
        assert_eq!(last, None);
    }

    #[test]
    fn test_get_last_uid_no_state() {
        let conn = test_conn();
        let last = get_last_uid(&conn, "acct", "INBOX").unwrap();
        assert_eq!(last, None);
    }
}
