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
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            txid      TEXT NOT NULL
        );
        ",
    )?;

    Ok(conn)
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
}

/// Insert or replace an heir.
pub fn heir_upsert(conn: &Connection, heir: &HeirRow) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO heirs (fingerprint, label, xpub, derivation_path)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(fingerprint) DO UPDATE SET
            label = excluded.label,
            xpub = excluded.xpub,
            derivation_path = excluded.derivation_path",
        params![heir.fingerprint, heir.label, heir.xpub, heir.derivation_path],
    )?;
    Ok(())
}

/// List all heirs.
pub fn heir_list(conn: &Connection) -> SqlResult<Vec<HeirRow>> {
    let mut stmt = conn.prepare("SELECT fingerprint, label, xpub, derivation_path FROM heirs")?;
    let rows = stmt.query_map([], |row| {
        Ok(HeirRow {
            fingerprint: row.get(0)?,
            label: row.get(1)?,
            xpub: row.get(2)?,
            derivation_path: row.get(3)?,
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
        "SELECT fingerprint, label, xpub, derivation_path FROM heirs WHERE fingerprint = ?1",
    )?;
    let mut rows = stmt.query(params![fingerprint])?;
    match rows.next()? {
        Some(row) => Ok(Some(HeirRow {
            fingerprint: row.get(0)?,
            label: row.get(1)?,
            xpub: row.get(2)?,
            derivation_path: row.get(3)?,
        })),
        None => Ok(None),
    }
}

// ============================================================================
// Check-in log
// ============================================================================

/// Record a successful check-in.
pub fn checkin_log_insert(conn: &Connection, timestamp: u64, txid: &str) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO checkin_log (timestamp, txid) VALUES (?1, ?2)",
        params![timestamp, txid],
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
    use std::path::PathBuf;
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
        };

        // Insert
        heir_upsert(&conn, &heir).unwrap();
        let list = heir_list(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].label, "Spouse");

        // Get by fingerprint
        let found = heir_get(&conn, "a1b2c3d4").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().xpub, "xpub6ABC...");

        // Not found
        assert!(heir_get(&conn, "deadbeef").unwrap().is_none());

        // Update via upsert
        let updated = HeirRow {
            fingerprint: "a1b2c3d4".into(),
            label: "Wife".into(),
            xpub: "xpub6DEF...".into(),
            derivation_path: "m/84'/0'/0'".into(),
        };
        heir_upsert(&conn, &updated).unwrap();
        let list = heir_list(&conn).unwrap();
        assert_eq!(list.len(), 1, "Should still be 1 heir after upsert");
        assert_eq!(list[0].label, "Wife");

        // Remove
        assert!(heir_remove(&conn, "a1b2c3d4").unwrap());
        assert!(!heir_remove(&conn, "a1b2c3d4").unwrap()); // Already gone
        assert_eq!(heir_list(&conn).unwrap().len(), 0);
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
    fn test_persistence_across_connections() {
        // This is the critical test: write data, close connection,
        // reopen from same file, verify data survived.
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
                },
            )
            .unwrap();

            checkin_log_insert(&conn, 1706745600, "abc123txid").unwrap();
            // Connection drops here
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

            assert_eq!(checkin_last(&conn).unwrap(), Some(1706745600));
        }
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
                },
            )
            .unwrap();
        }

        let list = heir_list(&conn).unwrap();
        assert_eq!(list.len(), 5);

        // Remove middle one
        heir_remove(&conn, "fp00000002").unwrap();
        assert_eq!(heir_list(&conn).unwrap().len(), 4);
    }
}
