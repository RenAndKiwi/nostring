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
