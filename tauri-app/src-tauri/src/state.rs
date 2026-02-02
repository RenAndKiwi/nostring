//! Application state management
//!
//! Holds encrypted seed data and wallet state.

use std::sync::Mutex;
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
    pub address: String,
    pub confirmed: bool,
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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            encrypted_seed: Mutex::new(None),
            unlocked: Mutex::new(false),
            policy_status: Mutex::new(None),
            electrum_url: Mutex::new("ssl://electrum.blockstream.info:60002".to_string()),
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }
}
