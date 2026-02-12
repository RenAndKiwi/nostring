//! Application state management
//!
//! In-memory cache backed by SQLite persistence.
//! All mutations write through to the database.

use crate::db::{self, HeirRow};
use bitcoin::bip32::{DerivationPath, Xpub};
use bitcoin::Network;
use miniscript::descriptor::DescriptorPublicKey;
use nostring_ccd::types::DelegatedKey;
use nostring_inherit::heir::{HeirKey, HeirRegistry};
use nostring_inherit::policy::{PathInfo, Timelock};
use nostring_inherit::taproot::{create_inheritable_vault, InheritableVault};
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

/// CCD (Chain Code Delegation) state.
///
/// Encapsulates all collaborative custody state: co-signer registration,
/// vault creation, and vault reconstruction from persisted data.
#[derive(Default)]
pub struct CcdState {
    /// Registered co-signer's delegated key info
    pub cosigner: Option<DelegatedKey>,
    /// Active inheritable vault (reconstructed from DB on startup)
    pub vault: Option<InheritableVault>,
    /// If vault reconstruction failed, the error message.
    /// The UI should surface this as a warning on startup.
    pub load_error: Option<String>,
}

impl CcdState {
    /// Reconstruct CCD state from database.
    ///
    /// Loads cosigner registration, then attempts to reconstruct the vault
    /// from stored parameters + the current heir registry.
    pub fn from_db(
        conn: &Connection,
        owner_xpub: Option<&str>,
        registry: &HeirRegistry,
        network: Network,
    ) -> Self {
        let cosigner = Self::load_cosigner(conn);
        let (vault, load_error) = match (&cosigner, owner_xpub) {
            (Some(delegated), Some(xpub_str)) => {
                match Self::reconstruct_vault(conn, delegated, xpub_str, registry, network) {
                    Ok(v) => (v, None),
                    Err(e) => {
                        log::error!("CCD vault reconstruction failed: {}", e);
                        (None, Some(e))
                    }
                }
            }
            _ => (None, None),
        };
        Self {
            cosigner,
            vault,
            load_error,
        }
    }

    fn load_cosigner(conn: &Connection) -> Option<DelegatedKey> {
        let pk_str = match db::config_get(conn, "cosigner_pubkey") {
            Ok(Some(s)) => s,
            _ => return None, // No cosigner registered — not an error
        };
        let cc_str = match db::config_get(conn, "cosigner_chain_code") {
            Ok(Some(s)) => s,
            Ok(None) => {
                log::warn!("CCD: cosigner pubkey found but chain code missing");
                return None;
            }
            Err(e) => {
                log::error!("CCD: failed to read cosigner_chain_code: {}", e);
                return None;
            }
        };
        let label = db::config_get(conn, "cosigner_label")
            .ok()
            .flatten()
            .unwrap_or_else(|| "cosigner".to_string());

        let pubkey = match bitcoin::secp256k1::PublicKey::from_str(&pk_str) {
            Ok(pk) => pk,
            Err(e) => {
                log::error!("CCD: invalid stored cosigner pubkey '{}': {}", pk_str, e);
                return None;
            }
        };
        let cc_bytes = match hex::decode(&cc_str) {
            Ok(b) if b.len() == 32 => b,
            Ok(b) => {
                log::error!(
                    "CCD: stored chain code wrong length ({} bytes, expected 32)",
                    b.len()
                );
                return None;
            }
            Err(e) => {
                log::error!("CCD: invalid stored chain code hex: {}", e);
                return None;
            }
        };
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&cc_bytes);
        let chain_code = nostring_ccd::types::ChainCode(arr);

        log::info!("CCD: loaded cosigner '{}' from database", label);
        Some(nostring_ccd::register_cosigner_with_chain_code(
            pubkey, chain_code, &label,
        ))
    }

    /// Reconstruct vault from persisted parameters.
    ///
    /// Returns:
    /// - `Ok(None)` — no vault was ever configured (clean state)
    /// - `Ok(Some(vault))` — vault reconstructed successfully
    /// - `Err(msg)` — vault data exists in DB but is corrupt/inconsistent
    fn reconstruct_vault(
        conn: &Connection,
        delegated: &DelegatedKey,
        owner_xpub_str: &str,
        registry: &HeirRegistry,
        network: Network,
    ) -> Result<Option<InheritableVault>, String> {
        // Check if vault was ever configured
        let index_str = match db::config_get(conn, "ccd_vault_index") {
            Ok(Some(s)) => s,
            Ok(None) => return Ok(None), // No vault created yet — clean state
            Err(e) => return Err(format!("Failed to read vault index from DB: {}", e)),
        };

        // From here on, vault data EXISTS — any failure is corrupt state
        let timelock_str = match db::config_get(conn, "ccd_vault_timelock") {
            Ok(Some(s)) => s,
            Ok(None) => return Err("Vault index found but timelock missing from database".into()),
            Err(e) => return Err(format!("Failed to read vault timelock from DB: {}", e)),
        };

        let address_index: u32 = index_str.parse().map_err(|e| {
            format!(
                "Stored vault index \'{}\' is not a valid number: {}",
                index_str, e
            )
        })?;
        let timelock_blocks: u16 = timelock_str.parse().map_err(|e| {
            format!(
                "Stored timelock \'{}\' is not a valid number: {}",
                timelock_str, e
            )
        })?;

        let owner_xpub =
            Xpub::from_str(owner_xpub_str).map_err(|e| format!("Owner xpub is invalid: {}", e))?;
        let owner_pubkey = owner_xpub.public_key;

        let heirs = registry.list();
        if heirs.is_empty() {
            return Err(
                "Vault was configured but all heirs have been removed.                  Add heirs to restore your vault."
                    .into(),
            );
        }

        let timelock = Timelock::from_blocks(timelock_blocks).map_err(|e| {
            format!(
                "Stored timelock ({} blocks) is invalid: {}",
                timelock_blocks, e
            )
        })?;
        let path_info = Self::heirs_to_path_info(heirs)
            .ok_or_else(|| "Failed to convert stored heir keys to descriptor keys".to_string())?;

        let vault = create_inheritable_vault(
            &owner_pubkey,
            delegated,
            address_index,
            path_info,
            timelock,
            0,
            network,
        )
        .map_err(|e| format!("Vault reconstruction failed: {}", e))?;

        log::info!(
            "CCD: reconstructed vault at {} (index {}, timelock {} blocks)",
            vault.address,
            address_index,
            timelock_blocks
        );
        Ok(Some(vault))
    }

    /// Convert heir list to PathInfo for vault creation.
    pub fn heirs_to_path_info(heirs: &[HeirKey]) -> Option<PathInfo> {
        if heirs.is_empty() {
            return None;
        }
        if heirs.len() == 1 {
            let xonly = heirs[0].xpub.public_key.x_only_public_key().0;
            let desc = DescriptorPublicKey::from_str(&format!("{}", xonly)).ok()?;
            Some(PathInfo::Single(desc))
        } else {
            let descs: Option<Vec<DescriptorPublicKey>> = heirs
                .iter()
                .map(|h| {
                    let xonly = h.xpub.public_key.x_only_public_key().0;
                    DescriptorPublicKey::from_str(&format!("{}", xonly)).ok()
                })
                .collect();
            let descs = descs?;
            let threshold = descs.len();
            Some(PathInfo::Multi(threshold, descs))
        }
    }
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

    // --- CCD (Chain Code Delegation) ---
    pub ccd: Mutex<CcdState>,

    // --- Ephemeral (not persisted) ---
    /// Whether user is "unlocked" (seed decrypted in session)
    pub unlocked: Mutex<bool>,
    /// Cached policy status (recomputed from blockchain)
    pub policy_status: Mutex<Option<PolicyStatus>>,
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
        let electrum_url = db::config_get(&conn, "electrum_url")
            .ok()
            .flatten()
            .unwrap_or_else(|| nostring_electrum::default_server(network).to_string());
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

        // Load CCD state (cosigner + vault reconstruction)
        let ccd = CcdState::from_db(&conn, owner_xpub.as_deref(), &registry, network);

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
            ccd: Mutex::new(ccd),
            unlocked: Mutex::new(unlocked),
            policy_status: Mutex::new(policy_status),
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
