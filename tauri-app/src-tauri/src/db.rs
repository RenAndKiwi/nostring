//! SQLite persistence layer.
//!
//! Stores all durable state so the app survives restarts.
//! Uses a simple key-value `config` table for singleton values
//! and a structured `heirs` table for the heir registry.

use rusqlite::{params, Connection, Result as SqlResult};
use std::path::Path;

/// Open (or create) the database at `path` and run migrations.
pub fn open_db(path: &Path) -> SqlResult<Connection> {
    let conn = Connection::open(path)?;

    // WAL mode for better concurrent read performance
    conn.pragma_update(None, "journal_mode", "WAL")?;

    // Run migrations
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS config (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS heirs (
            fingerprint    TEXT PRIMARY KEY,
            label          TEXT NOT NULL,
            xpub           TEXT NOT NULL,
            derivation_path TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS checkin_log (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp  INTEGER NOT NULL,
            txid       TEXT NOT NULL,
            spend_type TEXT NOT NULL DEFAULT 'owner_checkin'
        );
        ",
    )?;

    // v0.2 migrations — add heir contact info + delivery log
    migrate_v02(&conn)?;

    Ok(conn)
}

/// v0.2 migration: heir contact fields + descriptor delivery log.
fn migrate_v02(conn: &Connection) -> SqlResult<()> {
    // Add npub and email columns to heirs (idempotent via column check)
    let has_npub = conn.prepare("SELECT npub FROM heirs LIMIT 0").is_ok();
    if !has_npub {
        conn.execute_batch(
            "ALTER TABLE heirs ADD COLUMN npub TEXT;
             ALTER TABLE heirs ADD COLUMN email TEXT;",
        )?;
    }

    // Delivery log: tracks when descriptor backups were sent to heirs
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS delivery_log (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            heir_fingerprint TEXT NOT NULL,
            channel         TEXT NOT NULL,
            timestamp       INTEGER NOT NULL,
            success         INTEGER NOT NULL DEFAULT 1,
            error_msg       TEXT
        );",
    )?;

    // Spend events: tracks detected spend types (owner check-in vs heir claim)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS spend_events (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp   INTEGER NOT NULL,
            txid        TEXT NOT NULL,
            spend_type  TEXT NOT NULL,
            confidence  REAL NOT NULL DEFAULT 0.0,
            method      TEXT NOT NULL,
            policy_id   TEXT,
            outpoint    TEXT
        );",
    )?;

    Ok(())
}

// ============================================================================
// Config helpers (key-value)
// ============================================================================

/// Get a config value by key.
pub fn config_get(conn: &Connection, key: &str) -> SqlResult<Option<String>> {
    let mut stmt = conn.prepare_cached("SELECT value FROM config WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// Set a config value (upsert).
pub fn config_set(conn: &Connection, key: &str, value: &str) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Delete a config value.
#[allow(dead_code)]
pub fn config_delete(conn: &Connection, key: &str) -> SqlResult<()> {
    conn.execute("DELETE FROM config WHERE key = ?1", params![key])?;
    Ok(())
}

// ============================================================================
// Heir helpers
// ============================================================================

/// Serialisable heir row.
#[derive(Debug, Clone)]
pub struct HeirRow {
    pub fingerprint: String,
    pub label: String,
    pub xpub: String,
    pub derivation_path: String,
    /// Nostr npub for descriptor delivery (optional, v0.2)
    pub npub: Option<String>,
    /// Email address for descriptor delivery (optional, v0.2)
    pub email: Option<String>,
}

/// Insert or replace an heir.
pub fn heir_upsert(conn: &Connection, heir: &HeirRow) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO heirs (fingerprint, label, xpub, derivation_path, npub, email)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(fingerprint) DO UPDATE SET
            label = excluded.label,
            xpub = excluded.xpub,
            derivation_path = excluded.derivation_path,
            npub = excluded.npub,
            email = excluded.email",
        params![
            heir.fingerprint,
            heir.label,
            heir.xpub,
            heir.derivation_path,
            heir.npub,
            heir.email
        ],
    )?;
    Ok(())
}

/// Update only the contact fields (npub/email) for an existing heir.
pub fn heir_update_contact(
    conn: &Connection,
    fingerprint: &str,
    npub: Option<&str>,
    email: Option<&str>,
) -> SqlResult<bool> {
    let affected = conn.execute(
        "UPDATE heirs SET npub = ?2, email = ?3 WHERE fingerprint = ?1",
        params![fingerprint, npub, email],
    )?;
    Ok(affected > 0)
}

/// List all heirs.
pub fn heir_list(conn: &Connection) -> SqlResult<Vec<HeirRow>> {
    let mut stmt = conn.prepare(
        "SELECT fingerprint, label, xpub, derivation_path, npub, email FROM heirs",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(HeirRow {
            fingerprint: row.get(0)?,
            label: row.get(1)?,
            xpub: row.get(2)?,
            derivation_path: row.get(3)?,
            npub: row.get(4)?,
            email: row.get(5)?,
        })
    })?;
    rows.collect()
}

/// Remove an heir by fingerprint. Returns true if a row was deleted.
pub fn heir_remove(conn: &Connection, fingerprint: &str) -> SqlResult<bool> {
    let affected = conn.execute(
        "DELETE FROM heirs WHERE fingerprint = ?1",
        params![fingerprint],
    )?;
    Ok(affected > 0)
}

/// Get a single heir by fingerprint.
pub fn heir_get(conn: &Connection, fingerprint: &str) -> SqlResult<Option<HeirRow>> {
    let mut stmt = conn.prepare(
        "SELECT fingerprint, label, xpub, derivation_path, npub, email
         FROM heirs WHERE fingerprint = ?1",
    )?;
    let mut rows = stmt.query(params![fingerprint])?;
    match rows.next()? {
        Some(row) => Ok(Some(HeirRow {
            fingerprint: row.get(0)?,
            label: row.get(1)?,
            xpub: row.get(2)?,
            derivation_path: row.get(3)?,
            npub: row.get(4)?,
            email: row.get(5)?,
        })),
        None => Ok(None),
    }
}

// ============================================================================
// Spend events (owner check-in vs heir claim detection)
// ============================================================================

/// Serialisable spend event row.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SpendEventRow {
    pub id: i64,
    pub timestamp: u64,
    pub txid: String,
    pub spend_type: String,
    pub confidence: f64,
    pub method: String,
    pub policy_id: Option<String>,
    pub outpoint: Option<String>,
}

/// Insert a spend event.
#[allow(dead_code)]
pub fn spend_event_insert(
    conn: &Connection,
    timestamp: u64,
    txid: &str,
    spend_type: &str,
    confidence: f64,
    method: &str,
    policy_id: Option<&str>,
    outpoint: Option<&str>,
) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO spend_events (timestamp, txid, spend_type, confidence, method, policy_id, outpoint)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![timestamp, txid, spend_type, confidence, method, policy_id, outpoint],
    )?;
    Ok(())
}

/// List all spend events (most recent first).
pub fn spend_event_list(conn: &Connection) -> SqlResult<Vec<SpendEventRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, txid, spend_type, confidence, method, policy_id, outpoint
         FROM spend_events ORDER BY id DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SpendEventRow {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            txid: row.get(2)?,
            spend_type: row.get(3)?,
            confidence: row.get(4)?,
            method: row.get(5)?,
            policy_id: row.get(6)?,
            outpoint: row.get(7)?,
        })
    })?;
    rows.collect()
}

/// List spend events filtered by type.
#[allow(dead_code)]
pub fn spend_event_list_by_type(conn: &Connection, spend_type: &str) -> SqlResult<Vec<SpendEventRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, txid, spend_type, confidence, method, policy_id, outpoint
         FROM spend_events WHERE spend_type = ?1 ORDER BY id DESC",
    )?;
    let rows = stmt.query_map(params![spend_type], |row| {
        Ok(SpendEventRow {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            txid: row.get(2)?,
            spend_type: row.get(3)?,
            confidence: row.get(4)?,
            method: row.get(5)?,
            policy_id: row.get(6)?,
            outpoint: row.get(7)?,
        })
    })?;
    rows.collect()
}

/// Check if any heir claims have been detected.
#[allow(dead_code)]
pub fn has_heir_claims(conn: &Connection) -> SqlResult<bool> {
    let mut stmt = conn.prepare_cached(
        "SELECT COUNT(*) FROM spend_events WHERE spend_type = 'heir_claim'",
    )?;
    let count: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(count > 0)
}

// ============================================================================
// Delivery log (descriptor backup sent to heirs)
// ============================================================================

/// Record a descriptor delivery attempt to an heir.
pub fn delivery_log_insert(
    conn: &Connection,
    heir_fingerprint: &str,
    channel: &str,
    timestamp: u64,
    success: bool,
    error_msg: Option<&str>,
) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO delivery_log (heir_fingerprint, channel, timestamp, success, error_msg)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            heir_fingerprint,
            channel,
            timestamp,
            success as i32,
            error_msg
        ],
    )?;
    Ok(())
}

/// Get the last successful delivery timestamp for a given heir + channel.
pub fn delivery_last_success(
    conn: &Connection,
    heir_fingerprint: &str,
    channel: &str,
) -> SqlResult<Option<u64>> {
    let mut stmt = conn.prepare_cached(
        "SELECT timestamp FROM delivery_log
         WHERE heir_fingerprint = ?1 AND channel = ?2 AND success = 1
         ORDER BY id DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![heir_fingerprint, channel])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// Get all delivery log entries (most recent first).
#[allow(dead_code)]
pub fn delivery_log_list(conn: &Connection) -> SqlResult<Vec<DeliveryLogEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, heir_fingerprint, channel, timestamp, success, error_msg
         FROM delivery_log ORDER BY id DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(DeliveryLogEntry {
            id: row.get(0)?,
            heir_fingerprint: row.get(1)?,
            channel: row.get(2)?,
            timestamp: row.get(3)?,
            success: row.get::<_, i32>(4)? != 0,
            error_msg: row.get(5)?,
        })
    })?;
    rows.collect()
}

/// A delivery log entry.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DeliveryLogEntry {
    pub id: i64,
    pub heir_fingerprint: String,
    pub channel: String,
    pub timestamp: u64,
    pub success: bool,
    pub error_msg: Option<String>,
}

// ============================================================================
// Check-in log
// ============================================================================

/// Record a successful check-in (defaults to owner_checkin type).
pub fn checkin_log_insert(conn: &Connection, timestamp: u64, txid: &str) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO checkin_log (timestamp, txid, spend_type) VALUES (?1, ?2, 'owner_checkin')",
        params![timestamp, txid],
    )?;
    Ok(())
}

/// Record a check-in with explicit spend type.
#[allow(dead_code)]
pub fn checkin_log_insert_with_type(
    conn: &Connection,
    timestamp: u64,
    txid: &str,
    spend_type: &str,
) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO checkin_log (timestamp, txid, spend_type) VALUES (?1, ?2, ?3)",
        params![timestamp, txid, spend_type],
    )?;
    Ok(())
}

/// Get the most recent check-in timestamp.
pub fn checkin_last(conn: &Connection) -> SqlResult<Option<u64>> {
    let mut stmt =
        conn.prepare_cached("SELECT timestamp FROM checkin_log ORDER BY id DESC LIMIT 1")?;
    let mut rows = stmt.query([])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn temp_db() -> (Connection, NamedTempFile) {
        let file = NamedTempFile::new().expect("create temp file");
        let conn = open_db(file.path()).expect("open db");
        (conn, file)
    }

    #[test]
    fn test_config_roundtrip() {
        let (conn, _f) = temp_db();

        // Initially empty
        assert_eq!(config_get(&conn, "foo").unwrap(), None);

        // Set and get
        config_set(&conn, "foo", "bar").unwrap();
        assert_eq!(config_get(&conn, "foo").unwrap(), Some("bar".to_string()));

        // Upsert overwrites
        config_set(&conn, "foo", "baz").unwrap();
        assert_eq!(config_get(&conn, "foo").unwrap(), Some("baz".to_string()));

        // Delete
        config_delete(&conn, "foo").unwrap();
        assert_eq!(config_get(&conn, "foo").unwrap(), None);

        // Delete non-existent is fine
        config_delete(&conn, "nope").unwrap();
    }

    #[test]
    fn test_heir_crud() {
        let (conn, _f) = temp_db();

        let heir = HeirRow {
            fingerprint: "a1b2c3d4".into(),
            label: "Spouse".into(),
            xpub: "xpub6ABC...".into(),
            derivation_path: "m/84'/0'/0'".into(),
            npub: None,
            email: None,
        };

        // Insert
        heir_upsert(&conn, &heir).unwrap();
        let list = heir_list(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].label, "Spouse");
        assert!(list[0].npub.is_none());
        assert!(list[0].email.is_none());

        // Get by fingerprint
        let found = heir_get(&conn, "a1b2c3d4").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().xpub, "xpub6ABC...");

        // Not found
        assert!(heir_get(&conn, "deadbeef").unwrap().is_none());

        // Update via upsert (with contact info)
        let updated = HeirRow {
            fingerprint: "a1b2c3d4".into(),
            label: "Wife".into(),
            xpub: "xpub6DEF...".into(),
            derivation_path: "m/84'/0'/0'".into(),
            npub: Some("npub1test".into()),
            email: Some("wife@example.com".into()),
        };
        heir_upsert(&conn, &updated).unwrap();
        let list = heir_list(&conn).unwrap();
        assert_eq!(list.len(), 1, "Should still be 1 heir after upsert");
        assert_eq!(list[0].label, "Wife");
        assert_eq!(list[0].npub.as_deref(), Some("npub1test"));
        assert_eq!(list[0].email.as_deref(), Some("wife@example.com"));

        // Remove
        assert!(heir_remove(&conn, "a1b2c3d4").unwrap());
        assert!(!heir_remove(&conn, "a1b2c3d4").unwrap()); // Already gone
        assert_eq!(heir_list(&conn).unwrap().len(), 0);
    }

    #[test]
    fn test_heir_contact_update() {
        let (conn, _f) = temp_db();

        let heir = HeirRow {
            fingerprint: "a1b2c3d4".into(),
            label: "Alice".into(),
            xpub: "xpub6ABC...".into(),
            derivation_path: "m/84'/0'/0'".into(),
            npub: None,
            email: None,
        };
        heir_upsert(&conn, &heir).unwrap();

        // Initially no contact info
        let found = heir_get(&conn, "a1b2c3d4").unwrap().unwrap();
        assert!(found.npub.is_none());
        assert!(found.email.is_none());

        // Update contact info
        assert!(heir_update_contact(
            &conn,
            "a1b2c3d4",
            Some("npub1alice"),
            Some("alice@example.com"),
        )
        .unwrap());

        let found = heir_get(&conn, "a1b2c3d4").unwrap().unwrap();
        assert_eq!(found.npub.as_deref(), Some("npub1alice"));
        assert_eq!(found.email.as_deref(), Some("alice@example.com"));

        // Clear contact info
        assert!(heir_update_contact(&conn, "a1b2c3d4", None, None).unwrap());
        let found = heir_get(&conn, "a1b2c3d4").unwrap().unwrap();
        assert!(found.npub.is_none());
        assert!(found.email.is_none());

        // Update non-existent heir returns false
        assert!(!heir_update_contact(&conn, "deadbeef", Some("npub"), None).unwrap());
    }

    #[test]
    fn test_delivery_log() {
        let (conn, _f) = temp_db();

        // No deliveries initially
        assert_eq!(
            delivery_last_success(&conn, "a1b2c3d4", "nostr").unwrap(),
            None
        );

        // Log a successful delivery
        delivery_log_insert(&conn, "a1b2c3d4", "nostr", 1000, true, None).unwrap();
        assert_eq!(
            delivery_last_success(&conn, "a1b2c3d4", "nostr").unwrap(),
            Some(1000)
        );

        // Log a failed delivery — shouldn't affect last success
        delivery_log_insert(
            &conn,
            "a1b2c3d4",
            "nostr",
            2000,
            false,
            Some("relay timeout"),
        )
        .unwrap();
        assert_eq!(
            delivery_last_success(&conn, "a1b2c3d4", "nostr").unwrap(),
            Some(1000)
        );

        // Different channel is separate
        assert_eq!(
            delivery_last_success(&conn, "a1b2c3d4", "email").unwrap(),
            None
        );

        // Log success on email channel
        delivery_log_insert(&conn, "a1b2c3d4", "email", 3000, true, None).unwrap();
        assert_eq!(
            delivery_last_success(&conn, "a1b2c3d4", "email").unwrap(),
            Some(3000)
        );

        // All entries present
        let all = delivery_log_list(&conn).unwrap();
        assert_eq!(all.len(), 3);
        // Most recent first
        assert_eq!(all[0].timestamp, 3000);
    }

    #[test]
    fn test_delivery_log_across_connections() {
        let file = NamedTempFile::new().expect("create temp file");
        let db_path = file.path().to_path_buf();

        // Write delivery log
        {
            let conn = open_db(&db_path).expect("open db 1");
            heir_upsert(
                &conn,
                &HeirRow {
                    fingerprint: "aabb".into(),
                    label: "Test".into(),
                    xpub: "xpub...".into(),
                    derivation_path: "m/84'/0'/0'".into(),
                    npub: Some("npub1test".into()),
                    email: None,
                },
            )
            .unwrap();
            delivery_log_insert(&conn, "aabb", "nostr", 5000, true, None).unwrap();
        }

        // Read from new connection
        {
            let conn = open_db(&db_path).expect("open db 2");
            let last = delivery_last_success(&conn, "aabb", "nostr").unwrap();
            assert_eq!(last, Some(5000));
            let heir = heir_get(&conn, "aabb").unwrap().unwrap();
            assert_eq!(heir.npub.as_deref(), Some("npub1test"));
        }
    }

    #[test]
    fn test_checkin_log() {
        let (conn, _f) = temp_db();

        // No check-ins initially
        assert_eq!(checkin_last(&conn).unwrap(), None);

        // Log some check-ins
        checkin_log_insert(&conn, 1000, "txid_aaa").unwrap();
        assert_eq!(checkin_last(&conn).unwrap(), Some(1000));

        checkin_log_insert(&conn, 2000, "txid_bbb").unwrap();
        assert_eq!(checkin_last(&conn).unwrap(), Some(2000));

        // First one is still in the log (last returns most recent)
        checkin_log_insert(&conn, 3000, "txid_ccc").unwrap();
        assert_eq!(checkin_last(&conn).unwrap(), Some(3000));
    }

    #[test]
    fn test_checkin_log_with_type() {
        let (conn, _f) = temp_db();

        checkin_log_insert_with_type(&conn, 1000, "txid_owner", "owner_checkin").unwrap();
        checkin_log_insert_with_type(&conn, 2000, "txid_heir", "heir_claim").unwrap();

        assert_eq!(checkin_last(&conn).unwrap(), Some(2000));
    }

    #[test]
    fn test_spend_events() {
        let (conn, _f) = temp_db();

        // No events initially
        let events = spend_event_list(&conn).unwrap();
        assert!(events.is_empty());
        assert!(!has_heir_claims(&conn).unwrap());

        // Insert owner checkin
        spend_event_insert(
            &conn, 1000, "txid_owner", "owner_checkin", 0.95,
            "witness_analysis", Some("policy1"), Some("abc:0"),
        ).unwrap();

        let events = spend_event_list(&conn).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].spend_type, "owner_checkin");
        assert_eq!(events[0].confidence, 0.95);
        assert!(!has_heir_claims(&conn).unwrap());

        // Insert heir claim
        spend_event_insert(
            &conn, 2000, "txid_heir", "heir_claim", 0.9,
            "witness_analysis", Some("policy1"), Some("def:0"),
        ).unwrap();

        assert!(has_heir_claims(&conn).unwrap());

        let events = spend_event_list(&conn).unwrap();
        assert_eq!(events.len(), 2);

        // Filter by type
        let heir_events = spend_event_list_by_type(&conn, "heir_claim").unwrap();
        assert_eq!(heir_events.len(), 1);
        assert_eq!(heir_events[0].txid, "txid_heir");

        let owner_events = spend_event_list_by_type(&conn, "owner_checkin").unwrap();
        assert_eq!(owner_events.len(), 1);
        assert_eq!(owner_events[0].txid, "txid_owner");
    }

    #[test]
    fn test_persistence_across_connections() {
        let file = NamedTempFile::new().expect("create temp file");
        let db_path = file.path().to_path_buf();

        // First connection: write data
        {
            let conn = open_db(&db_path).expect("open db 1");
            config_set(&conn, "owner_xpub", "xpub6REAL...").unwrap();
            config_set(&conn, "watch_only", "true").unwrap();
            config_set(&conn, "service_key", "deadbeef01234567").unwrap();

            heir_upsert(
                &conn,
                &HeirRow {
                    fingerprint: "aabbccdd".into(),
                    label: "Child".into(),
                    xpub: "xpub6CHILD...".into(),
                    derivation_path: "m/84'/0'/0'".into(),
                    npub: Some("npub1child".into()),
                    email: Some("child@example.com".into()),
                },
            )
            .unwrap();

            checkin_log_insert(&conn, 1706745600, "abc123txid").unwrap();
        }

        // Second connection: verify everything survived
        {
            let conn = open_db(&db_path).expect("open db 2");

            assert_eq!(
                config_get(&conn, "owner_xpub").unwrap(),
                Some("xpub6REAL...".to_string())
            );
            assert_eq!(
                config_get(&conn, "watch_only").unwrap(),
                Some("true".to_string())
            );
            assert_eq!(
                config_get(&conn, "service_key").unwrap(),
                Some("deadbeef01234567".to_string())
            );

            let heirs = heir_list(&conn).unwrap();
            assert_eq!(heirs.len(), 1);
            assert_eq!(heirs[0].label, "Child");
            assert_eq!(heirs[0].fingerprint, "aabbccdd");
            assert_eq!(heirs[0].npub.as_deref(), Some("npub1child"));
            assert_eq!(heirs[0].email.as_deref(), Some("child@example.com"));

            assert_eq!(checkin_last(&conn).unwrap(), Some(1706745600));
        }
    }

    #[test]
    fn test_nsec_inheritance_revocation() {
        let (conn, _f) = temp_db();

        // Simulate setting up nsec inheritance
        let locked_shares = serde_json::to_string(&vec![
            "ms12nsecbyyy".to_string(),
            "ms12nsecczz".to_string(),
        ])
        .unwrap();
        config_set(&conn, "nsec_locked_shares", &locked_shares).unwrap();
        config_set(&conn, "nsec_owner_npub", "npub1testowner123").unwrap();

        // Verify they exist
        assert_eq!(
            config_get(&conn, "nsec_owner_npub").unwrap(),
            Some("npub1testowner123".to_string())
        );
        let shares: Vec<String> =
            serde_json::from_str(&config_get(&conn, "nsec_locked_shares").unwrap().unwrap())
                .unwrap();
        assert_eq!(shares.len(), 2);

        // Revoke: delete both keys
        config_delete(&conn, "nsec_locked_shares").unwrap();
        config_delete(&conn, "nsec_owner_npub").unwrap();

        // Verify they're gone
        assert_eq!(config_get(&conn, "nsec_owner_npub").unwrap(), None);
        assert_eq!(config_get(&conn, "nsec_locked_shares").unwrap(), None);
    }

    #[test]
    fn test_nsec_inheritance_resplit() {
        let (conn, _f) = temp_db();

        // Initial split
        let locked_v1 = serde_json::to_string(&vec!["share_v1_a".to_string()]).unwrap();
        config_set(&conn, "nsec_locked_shares", &locked_v1).unwrap();
        config_set(&conn, "nsec_owner_npub", "npub1original").unwrap();

        // Re-split: overwrite with new data (upsert)
        let locked_v2 =
            serde_json::to_string(&vec!["share_v2_a".to_string(), "share_v2_b".to_string()])
                .unwrap();
        config_set(&conn, "nsec_locked_shares", &locked_v2).unwrap();
        config_set(&conn, "nsec_owner_npub", "npub1newidentity").unwrap();

        // Verify new data
        assert_eq!(
            config_get(&conn, "nsec_owner_npub").unwrap(),
            Some("npub1newidentity".to_string())
        );
        let shares: Vec<String> =
            serde_json::from_str(&config_get(&conn, "nsec_locked_shares").unwrap().unwrap())
                .unwrap();
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0], "share_v2_a");
        assert_eq!(shares[1], "share_v2_b");
    }

    #[test]
    fn test_nsec_revoke_then_resplit() {
        let (conn, _f) = temp_db();

        // Setup
        config_set(&conn, "nsec_locked_shares", r#"["s1"]"#).unwrap();
        config_set(&conn, "nsec_owner_npub", "npub1first").unwrap();

        // Revoke
        config_delete(&conn, "nsec_locked_shares").unwrap();
        config_delete(&conn, "nsec_owner_npub").unwrap();
        assert_eq!(config_get(&conn, "nsec_owner_npub").unwrap(), None);

        // Re-split after revocation
        config_set(&conn, "nsec_locked_shares", r#"["s2","s3"]"#).unwrap();
        config_set(&conn, "nsec_owner_npub", "npub1second").unwrap();

        assert_eq!(
            config_get(&conn, "nsec_owner_npub").unwrap(),
            Some("npub1second".to_string())
        );
        let shares: Vec<String> =
            serde_json::from_str(&config_get(&conn, "nsec_locked_shares").unwrap().unwrap())
                .unwrap();
        assert_eq!(shares.len(), 2);
    }

    #[test]
    fn test_multiple_heirs() {
        let (conn, _f) = temp_db();

        for i in 0..5 {
            heir_upsert(
                &conn,
                &HeirRow {
                    fingerprint: format!("fp{:08x}", i),
                    label: format!("Heir {}", i),
                    xpub: format!("xpub{}", i),
                    derivation_path: "m/84'/0'/0'".into(),
                    npub: None,
                    email: Some(format!("heir{}@example.com", i)),
                },
            )
            .unwrap();
        }

        let list = heir_list(&conn).unwrap();
        assert_eq!(list.len(), 5);
        assert_eq!(list[0].email.as_deref(), Some("heir0@example.com"));

        // Remove middle one
        heir_remove(&conn, "fp00000002").unwrap();
        assert_eq!(heir_list(&conn).unwrap().len(), 4);
    }
}
