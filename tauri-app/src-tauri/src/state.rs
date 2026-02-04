//! Application state management
//!
//! In-memory cache backed by SQLite persistence.
//! All mutations write through to the database.

use crate::db::{self, HeirRow};
use bitcoin::bip32::{DerivationPath, Xpub};
use bitcoin::Network;
use nostring_inherit::heir::{HeirKey, HeirRegistry};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

/// Policy status for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyStatus {
    pub current_block: u64,
    pub expiry_block: u64,
    pub blocks_remaining: i64,
    pub days_remaining: f64,
    pub urgency: String,
    pub last_checkin: Option<u64>,
}

/// Inheritance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceConfig {
    pub descriptor: String,
    pub timelock_blocks: u16,
    pub network: String,
}

/// Application state (thread-safe, SQLite-backed)
pub struct AppState {
    // --- Persistent (backed by SQLite) ---
    /// SQLite connection
    pub db: Mutex<Connection>,

    /// Encrypted seed bytes (serialized EncryptedSeed)
    pub encrypted_seed: Mutex<Option<Vec<u8>>>,
    /// Owner xpub for watch-only mode
    pub owner_xpub: Mutex<Option<String>>,
    /// Whether running in watch-only mode
    pub watch_only: Mutex<bool>,
    /// Inheritance configuration
    pub inheritance_config: Mutex<Option<InheritanceConfig>>,
    /// Registry of designated heirs
    pub heir_registry: Mutex<HeirRegistry>,
    /// Service key secret (hex-encoded)
    pub service_key: Mutex<Option<String>>,
    /// Service key npub (bech32)
    pub service_npub: Mutex<Option<String>>,
    /// Electrum server URL
    pub electrum_url: Mutex<String>,
    /// Bitcoin network
    pub network: Mutex<Network>,

    // --- Ephemeral (not persisted) ---
    /// Whether user is "unlocked" (seed decrypted in session)
    pub unlocked: Mutex<bool>,
    /// Cached policy status (recomputed from blockchain)
    pub policy_status: Mutex<Option<PolicyStatus>>,
    /// Cached UTXOs
    #[allow(dead_code)]
    pub cached_utxos: Mutex<Vec<()>>,
}

impl AppState {
    /// Create state from a database path, loading any persisted data.
    pub fn from_db_path(db_path: PathBuf) -> Self {
        let conn = db::open_db(&db_path).expect("Failed to open database");

        // Load persisted values
        let owner_xpub = db::config_get(&conn, "owner_xpub").ok().flatten();
        let watch_only = db::config_get(&conn, "watch_only")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false);
        let encrypted_seed = db::config_get(&conn, "encrypted_seed")
            .ok()
            .flatten()
            .and_then(|hex_str| hex::decode(&hex_str).ok());
        let electrum_url = db::config_get(&conn, "electrum_url")
            .ok()
            .flatten()
            .unwrap_or_else(|| "ssl://electrum.blockstream.info:60002".to_string());
        let network_str = db::config_get(&conn, "network")
            .ok()
            .flatten()
            .unwrap_or_else(|| "bitcoin".to_string());
        let network = match network_str.as_str() {
            "testnet" | "testnet3" => Network::Testnet,
            "signet" => Network::Signet,
            "regtest" => Network::Regtest,
            _ => Network::Bitcoin,
        };
        let service_key = db::config_get(&conn, "service_key").ok().flatten();
        let service_npub = db::config_get(&conn, "service_npub").ok().flatten();

        // Load inheritance config
        let inheritance_config = db::config_get(&conn, "inheritance_descriptor")
            .ok()
            .flatten()
            .map(|descriptor| {
                let timelock_blocks = db::config_get(&conn, "inheritance_timelock")
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse::<u16>().ok())
                    .unwrap_or(26280);
                let ic_network = db::config_get(&conn, "inheritance_network")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| network_str.clone());
                InheritanceConfig {
                    descriptor,
                    timelock_blocks,
                    network: ic_network,
                }
            });

        // Load heirs
        let mut registry = HeirRegistry::new();
        if let Ok(rows) = db::heir_list(&conn) {
            for row in rows {
                if let Ok(xpub) = Xpub::from_str(&row.xpub) {
                    let fp = xpub.fingerprint();
                    let derivation_path = DerivationPath::from_str(&row.derivation_path)
                        .unwrap_or_else(|_| DerivationPath::from_str("m/84'/0'/0'").unwrap());
                    let heir = HeirKey::new(&row.label, fp, xpub, Some(derivation_path));
                    registry.add(heir);
                }
            }
        }

        // Load last check-in time
        let last_checkin = db::checkin_last(&conn).ok().flatten();
        let policy_status = last_checkin.map(|ts| PolicyStatus {
            current_block: 0,
            expiry_block: 0,
            blocks_remaining: 0,
            days_remaining: 0.0,
            urgency: "unknown".to_string(),
            last_checkin: Some(ts),
        });

        // Determine if auto-unlock makes sense (watch-only doesn't need password)
        let unlocked = watch_only && owner_xpub.is_some();

        Self {
            db: Mutex::new(conn),
            encrypted_seed: Mutex::new(encrypted_seed),
            owner_xpub: Mutex::new(owner_xpub),
            watch_only: Mutex::new(watch_only),
            inheritance_config: Mutex::new(inheritance_config),
            heir_registry: Mutex::new(registry),
            service_key: Mutex::new(service_key),
            service_npub: Mutex::new(service_npub),
            electrum_url: Mutex::new(electrum_url),
            network: Mutex::new(network),
            unlocked: Mutex::new(unlocked),
            policy_status: Mutex::new(policy_status),
            cached_utxos: Mutex::new(Vec::new()),
        }
    }
}

// ============================================================================
// Write-through helpers (call these instead of raw Mutex writes)
// ============================================================================

impl AppState {
    /// Persist a config key-value pair.
    pub fn persist_config(&self, key: &str, value: &str) {
        let conn = self.db.lock().unwrap();
        let _ = db::config_set(&conn, key, value);
    }

    /// Delete a config key.
    #[allow(dead_code)]
    pub fn delete_config(&self, key: &str) {
        let conn = self.db.lock().unwrap();
        let _ = db::config_delete(&conn, key);
    }

    /// Persist an heir to the database.
    pub fn persist_heir(&self, heir: &HeirKey, timelock_months: Option<u32>) {
        let conn = self.db.lock().unwrap();
        let row = HeirRow {
            fingerprint: heir.fingerprint.to_string(),
            label: heir.label.clone(),
            xpub: heir.xpub.to_string(),
            derivation_path: heir.derivation_path.to_string(),
            npub: None,
            email: None,
            timelock_months,
        };
        let _ = db::heir_upsert(&conn, &row);
    }

    /// Update heir contact info (npub/email) in the database.
    pub fn update_heir_contact(
        &self,
        fingerprint: &str,
        npub: Option<&str>,
        email: Option<&str>,
    ) -> bool {
        let conn = self.db.lock().unwrap();
        db::heir_update_contact(&conn, fingerprint, npub, email).unwrap_or(false)
    }

    /// Log a descriptor delivery attempt.
    pub fn log_delivery(
        &self,
        heir_fingerprint: &str,
        channel: &str,
        success: bool,
        error_msg: Option<&str>,
    ) {
        let conn = self.db.lock().unwrap();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = db::delivery_log_insert(
            &conn,
            heir_fingerprint,
            channel,
            timestamp,
            success,
            error_msg,
        );
    }

    /// Check if we already delivered to this heir on this channel recently
    /// (within the cooldown period). Returns true if delivery is allowed.
    pub fn can_deliver_to_heir(
        &self,
        heir_fingerprint: &str,
        channel: &str,
        cooldown_secs: u64,
    ) -> bool {
        let conn = self.db.lock().unwrap();
        let last = db::delivery_last_success(&conn, heir_fingerprint, channel)
            .ok()
            .flatten();
        match last {
            None => true, // Never delivered
            Some(ts) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                now.saturating_sub(ts) >= cooldown_secs
            }
        }
    }

    /// Remove an heir from the database.
    pub fn remove_heir_db(&self, fingerprint: &str) {
        let conn = self.db.lock().unwrap();
        let _ = db::heir_remove(&conn, fingerprint);
    }

    /// Log a successful check-in.
    pub fn log_checkin(&self, txid: &str) {
        let conn = self.db.lock().unwrap();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = db::checkin_log_insert(&conn, timestamp, txid);
    }

    /// Set owner xpub and persist.
    pub fn set_owner_xpub(&self, xpub: &str) {
        {
            let mut lock = self.owner_xpub.lock().unwrap();
            *lock = Some(xpub.to_string());
        }
        self.persist_config("owner_xpub", xpub);
    }

    /// Set watch-only mode and persist.
    pub fn set_watch_only(&self, val: bool) {
        {
            let mut lock = self.watch_only.lock().unwrap();
            *lock = val;
        }
        self.persist_config("watch_only", if val { "true" } else { "false" });
    }

    /// Set encrypted seed and persist (hex-encoded).
    pub fn set_encrypted_seed(&self, bytes: Vec<u8>) {
        let hex_str = hex::encode(&bytes);
        {
            let mut lock = self.encrypted_seed.lock().unwrap();
            *lock = Some(bytes);
        }
        self.persist_config("encrypted_seed", &hex_str);
    }

    /// Set Bitcoin network and persist.
    pub fn set_network(&self, network: Network) {
        let network_str = match network {
            Network::Bitcoin => "bitcoin",
            Network::Testnet => "testnet",
            Network::Signet => "signet",
            Network::Regtest => "regtest",
            _ => "bitcoin",
        };
        {
            let mut lock = self.network.lock().unwrap();
            *lock = network;
        }
        self.persist_config("network", network_str);
    }

    /// Set electrum URL and persist.
    pub fn set_electrum_url(&self, url: &str) {
        {
            let mut lock = self.electrum_url.lock().unwrap();
            *lock = url.to_string();
        }
        self.persist_config("electrum_url", url);
    }

    /// Set service key and persist.
    pub fn set_service_key(&self, secret_hex: &str, npub: &str) {
        {
            let mut sk = self.service_key.lock().unwrap();
            *sk = Some(secret_hex.to_string());
        }
        {
            let mut np = self.service_npub.lock().unwrap();
            *np = Some(npub.to_string());
        }
        self.persist_config("service_key", secret_hex);
        self.persist_config("service_npub", npub);
    }

    /// Set inheritance config and persist.
    #[allow(dead_code)]
    pub fn set_inheritance_config(&self, config: InheritanceConfig) {
        self.persist_config("inheritance_descriptor", &config.descriptor);
        self.persist_config("inheritance_timelock", &config.timelock_blocks.to_string());
        self.persist_config("inheritance_network", &config.network);
        {
            let mut lock = self.inheritance_config.lock().unwrap();
            *lock = Some(config);
        }
    }
}
