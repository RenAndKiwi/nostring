//! Persistent state for the watch service
//!
//! Tracks known UTXOs and last poll times to detect changes.

use bitcoin::{Amount, OutPoint};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use thiserror::Error;

/// Errors from state operations
#[derive(Error, Debug)]
pub enum StateError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Policy not found: {0}")]
    PolicyNotFound(String),
}

/// A tracked UTXO
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackedUtxo {
    /// The outpoint (txid:vout)
    #[serde(with = "outpoint_serde")]
    pub outpoint: OutPoint,
    /// Value in satoshis
    #[serde(with = "amount_serde")]
    pub value: Amount,
    /// Block height where confirmed
    pub height: u32,
    /// When we first saw this UTXO (unix timestamp)
    pub first_seen: u64,
}

/// Serde helper for OutPoint
mod outpoint_serde {
    use bitcoin::OutPoint;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(outpoint: &OutPoint, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        outpoint.to_string().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<OutPoint, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Serde helper for Amount
mod amount_serde {
    use bitcoin::Amount;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(amount: &Amount, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(amount.to_sat())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Amount, D::Error>
    where
        D: Deserializer<'de>,
    {
        let sats = u64::deserialize(deserializer)?;
        Ok(Amount::from_sat(sats))
    }
}

/// State for a single watched policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyState {
    /// User-provided label or generated ID
    pub id: String,
    /// The descriptor string (for regenerating script)
    pub descriptor: String,
    /// Currently known UTXOs
    pub utxos: Vec<TrackedUtxo>,
    /// Block height when UTXO was first funded (for timelock calculation)
    pub funding_height: Option<u32>,
    /// Timelock in blocks (from policy)
    pub timelock_blocks: u32,
}

impl PolicyState {
    /// Create a new policy state
    pub fn new(id: impl Into<String>, descriptor: impl Into<String>, timelock_blocks: u32) -> Self {
        Self {
            id: id.into(),
            descriptor: descriptor.into(),
            utxos: Vec::new(),
            funding_height: None,
            timelock_blocks,
        }
    }

    /// Check if a UTXO is already tracked
    pub fn has_utxo(&self, outpoint: &OutPoint) -> bool {
        self.utxos.iter().any(|u| &u.outpoint == outpoint)
    }

    /// Add a new UTXO
    pub fn add_utxo(&mut self, utxo: TrackedUtxo) {
        if !self.has_utxo(&utxo.outpoint) {
            // Update funding height if this is the first/earliest UTXO
            if self.funding_height.is_none()
                || utxo.height < self.funding_height.unwrap_or(u32::MAX)
            {
                self.funding_height = Some(utxo.height);
            }
            self.utxos.push(utxo);
        }
    }

    /// Remove a UTXO (when spent)
    pub fn remove_utxo(&mut self, outpoint: &OutPoint) -> Option<TrackedUtxo> {
        if let Some(idx) = self.utxos.iter().position(|u| &u.outpoint == outpoint) {
            Some(self.utxos.remove(idx))
        } else {
            None
        }
    }

    /// Get all tracked outpoints
    pub fn outpoints(&self) -> Vec<OutPoint> {
        self.utxos.iter().map(|u| u.outpoint).collect()
    }

    /// Calculate blocks remaining until timelock expires
    pub fn blocks_until_expiry(&self, current_height: u32) -> Option<i64> {
        self.funding_height.map(|funding| {
            let expiry = funding as i64 + self.timelock_blocks as i64;
            expiry - current_height as i64
        })
    }
}

/// Full watch state (all policies)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchState {
    /// Watched policies by ID
    pub policies: HashMap<String, PolicyState>,
    /// Last successful poll (unix timestamp)
    pub last_poll: Option<u64>,
    /// Last known block height
    pub last_height: Option<u32>,
}

impl WatchState {
    /// Create empty state
    pub fn new() -> Self {
        Self::default()
    }

    /// Load state from file, or create empty if not exists
    pub fn load(path: &Path) -> Result<Self, StateError> {
        if path.exists() {
            let contents = fs::read_to_string(path)?;
            let state: WatchState = serde_json::from_str(&contents)?;
            Ok(state)
        } else {
            Ok(Self::new())
        }
    }

    /// Save state to file
    pub fn save(&self, path: &Path) -> Result<(), StateError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }

    /// Add or update a policy
    pub fn add_policy(&mut self, policy: PolicyState) {
        self.policies.insert(policy.id.clone(), policy);
    }

    /// Get a policy by ID
    pub fn get_policy(&self, id: &str) -> Option<&PolicyState> {
        self.policies.get(id)
    }

    /// Get a mutable policy by ID
    pub fn get_policy_mut(&mut self, id: &str) -> Option<&mut PolicyState> {
        self.policies.get_mut(id)
    }

    /// Remove a policy
    pub fn remove_policy(&mut self, id: &str) -> Option<PolicyState> {
        self.policies.remove(id)
    }

    /// List all policy IDs
    pub fn policy_ids(&self) -> Vec<String> {
        self.policies.keys().cloned().collect()
    }

    /// Update last poll info
    pub fn update_poll(&mut self, timestamp: u64, height: u32) {
        self.last_poll = Some(timestamp);
        self.last_height = Some(height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tempfile::tempdir;

    fn test_outpoint() -> OutPoint {
        OutPoint::from_str("1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef:0")
            .unwrap()
    }

    #[test]
    fn test_policy_state_utxo_tracking() {
        let mut policy = PolicyState::new("test", "wsh(...)", 26280);
        assert!(policy.utxos.is_empty());

        let utxo = TrackedUtxo {
            outpoint: test_outpoint(),
            value: Amount::from_sat(100000),
            height: 934000,
            first_seen: 1700000000,
        };

        policy.add_utxo(utxo.clone());
        assert_eq!(policy.utxos.len(), 1);
        assert!(policy.has_utxo(&test_outpoint()));
        assert_eq!(policy.funding_height, Some(934000));

        // Adding same UTXO again should be idempotent
        policy.add_utxo(utxo);
        assert_eq!(policy.utxos.len(), 1);

        // Remove UTXO
        let removed = policy.remove_utxo(&test_outpoint());
        assert!(removed.is_some());
        assert!(policy.utxos.is_empty());
    }

    #[test]
    fn test_blocks_until_expiry() {
        let mut policy = PolicyState::new("test", "wsh(...)", 26280); // ~6 months

        // No funding yet
        assert!(policy.blocks_until_expiry(934000).is_none());

        // Add funding
        policy.funding_height = Some(930000);

        // Current at 934000, funding at 930000, timelock 26280
        // Expiry at 930000 + 26280 = 956280
        // Remaining = 956280 - 934000 = 22280
        let remaining = policy.blocks_until_expiry(934000).unwrap();
        assert_eq!(remaining, 22280);

        // After expiry
        let remaining = policy.blocks_until_expiry(960000).unwrap();
        assert!(remaining < 0);
    }

    #[test]
    fn test_watch_state_persistence() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("watch_state.json");

        // Create and save state
        let mut state = WatchState::new();
        state.add_policy(PolicyState::new("policy1", "wsh(pk(...))", 26280));
        state.update_poll(1700000000, 934000);
        state.save(&path).unwrap();

        // Load and verify
        let loaded = WatchState::load(&path).unwrap();
        assert_eq!(loaded.policies.len(), 1);
        assert!(loaded.get_policy("policy1").is_some());
        assert_eq!(loaded.last_poll, Some(1700000000));
        assert_eq!(loaded.last_height, Some(934000));
    }

    #[test]
    fn test_tracked_utxo_serde() {
        let utxo = TrackedUtxo {
            outpoint: test_outpoint(),
            value: Amount::from_sat(100000),
            height: 934000,
            first_seen: 1700000000,
        };

        let json = serde_json::to_string(&utxo).unwrap();
        let restored: TrackedUtxo = serde_json::from_str(&json).unwrap();

        assert_eq!(utxo.outpoint, restored.outpoint);
        assert_eq!(utxo.value, restored.value);
        assert_eq!(utxo.height, restored.height);
    }
}
