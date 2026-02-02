//! Application state management
//!
//! Holds encrypted seed data and wallet state.

use std::sync::Mutex;
use bitcoin::Network;
use nostring_inherit::heir::HeirRegistry;
use serde::{Deserialize, Serialize};

/// Policy status for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyStatus {
    /// Current block height
    pub current_block: u64,
    /// Block when timelock expires
    pub expiry_block: u64,
    /// Blocks remaining until expiry
    pub blocks_remaining: i64,
    /// Approximate days remaining
    pub days_remaining: f64,
    /// Urgency level: "ok", "warning", "critical"
    pub urgency: String,
    /// Last check-in timestamp
    pub last_checkin: Option<u64>,
}

/// Wallet/inheritance UTXO info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceUtxo {
    pub txid: String,
    pub vout: u32,
    pub amount_sats: u64,
    pub script_pubkey_hex: String,
    pub height: u32,
}

/// Inheritance configuration (set during policy creation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceConfig {
    /// The WSH descriptor string (for address generation)
    pub descriptor: String,
    /// Timelock in blocks (e.g., 26280 for ~6 months)
    pub timelock_blocks: u16,
    /// Bitcoin network
    pub network: String,
}

/// Application state (thread-safe)
pub struct AppState {
    /// Encrypted seed bytes (serialized EncryptedSeed)
    pub encrypted_seed: Mutex<Option<Vec<u8>>>,
    /// Whether user is "unlocked" (seed decrypted in session)
    pub unlocked: Mutex<bool>,
    /// Cached policy status
    pub policy_status: Mutex<Option<PolicyStatus>>,
    /// Electrum server URL
    pub electrum_url: Mutex<String>,
    /// Bitcoin network
    pub network: Mutex<Network>,
    /// Inheritance configuration
    pub inheritance_config: Mutex<Option<InheritanceConfig>>,
    /// Cached UTXOs for inheritance address (TODO: use for caching)
    #[allow(dead_code)]
    pub cached_utxos: Mutex<Vec<InheritanceUtxo>>,
    /// Registry of designated heirs
    pub heir_registry: Mutex<HeirRegistry>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            encrypted_seed: Mutex::new(None),
            unlocked: Mutex::new(false),
            policy_status: Mutex::new(None),
            electrum_url: Mutex::new("ssl://electrum.blockstream.info:60002".to_string()),
            network: Mutex::new(Network::Bitcoin),
            inheritance_config: Mutex::new(None),
            cached_utxos: Mutex::new(Vec::new()),
            heir_registry: Mutex::new(HeirRegistry::new()),
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }
}
