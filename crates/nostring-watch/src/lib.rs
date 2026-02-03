//! NoString Watch Service
//!
//! Monitors Bitcoin blockchain for inheritance UTXO state changes.
//!
//! # Features
//!
//! - Periodic polling of inheritance addresses via Electrum
//! - Detects UTXO appearances (new funding) and spends (check-in or claim)
//! - Persistent state tracking across restarts
//! - Event-based notifications for UI integration
//!
//! # Example
//!
//! ```ignore
//! use nostring_watch::{WatchService, WatchConfig};
//! use nostring_electrum::ElectrumClient;
//! use std::path::PathBuf;
//!
//! let client = ElectrumClient::new("ssl://blockstream.info:700", Network::Bitcoin)?;
//! let config = WatchConfig {
//!     state_path: PathBuf::from("~/.nostring/watch_state.json"),
//!     poll_interval_secs: 600, // 10 minutes
//!     min_poll_interval_secs: 60, // 1 minute minimum
//! };
//!
//! let mut service = WatchService::new(client, config)?;
//! service.add_policy("inheritance", descriptor, timelock_blocks)?;
//!
//! // Poll once and get events
//! let events = service.poll()?;
//! for event in events {
//!     println!("Event: {:?}", event);
//! }
//! ```

pub mod events;
pub mod spend_analysis;
pub mod state;

pub use events::{SpendType, WatchEvent};
pub use spend_analysis::{analyze_spend, analyze_witness, SpendAnalysis, DetectionMethod};
pub use state::{PolicyState, TrackedUtxo, WatchState};

use bitcoin::hashes::Hash;
use bitcoin::{Network, OutPoint, ScriptBuf, Txid};
use miniscript::descriptor::DescriptorPublicKey;
use miniscript::Descriptor;
use nostring_electrum::{ElectrumClient, Utxo};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Errors from the watch service
#[derive(Error, Debug)]
pub enum WatchError {
    #[error("Electrum error: {0}")]
    Electrum(#[from] nostring_electrum::Error),

    #[error("State error: {0}")]
    State(#[from] state::StateError),

    #[error("Invalid descriptor: {0}")]
    InvalidDescriptor(String),

    #[error("Policy not found: {0}")]
    PolicyNotFound(String),

    #[error("Poll interval too short (minimum {min} seconds)")]
    PollTooFrequent { min: u64 },
}

/// Configuration for the watch service
#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// Path to state file
    pub state_path: PathBuf,
    /// Default poll interval in seconds
    pub poll_interval_secs: u64,
    /// Minimum allowed poll interval (rate limiting)
    pub min_poll_interval_secs: u64,
    /// Warning threshold in blocks (emit TimelockWarning when below)
    pub warning_threshold_blocks: i64,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            state_path: PathBuf::from("watch_state.json"),
            poll_interval_secs: 600,        // 10 minutes
            min_poll_interval_secs: 60,     // 1 minute minimum
            warning_threshold_blocks: 4320, // ~30 days
        }
    }
}

/// UTXO monitoring service
pub struct WatchService {
    client: ElectrumClient,
    config: WatchConfig,
    state: WatchState,
    _network: Network,
}

impl WatchService {
    /// Create a new watch service
    pub fn new(client: ElectrumClient, config: WatchConfig) -> Result<Self, WatchError> {
        let network = client.network();
        let state = WatchState::load(&config.state_path).unwrap_or_default();

        Ok(Self {
            client,
            config,
            state,
            _network: network,
        })
    }

    /// Add a policy to watch
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this policy
    /// * `descriptor` - WSH descriptor string
    /// * `timelock_blocks` - Timelock duration in blocks
    pub fn add_policy(
        &mut self,
        id: impl Into<String>,
        descriptor: impl Into<String>,
        timelock_blocks: u32,
    ) -> Result<(), WatchError> {
        let id = id.into();
        let descriptor = descriptor.into();

        // Validate descriptor parses
        let _: Descriptor<DescriptorPublicKey> = Descriptor::from_str(&descriptor)
            .map_err(|e| WatchError::InvalidDescriptor(e.to_string()))?;

        let policy = PolicyState::new(&id, &descriptor, timelock_blocks);
        self.state.add_policy(policy);
        self.save_state()?;

        log::info!("Added policy to watch: {}", id);
        Ok(())
    }

    /// Remove a policy from watching
    pub fn remove_policy(&mut self, id: &str) -> Result<(), WatchError> {
        self.state
            .remove_policy(id)
            .ok_or_else(|| WatchError::PolicyNotFound(id.to_string()))?;
        self.save_state()?;
        log::info!("Removed policy from watch: {}", id);
        Ok(())
    }

    /// List watched policy IDs
    pub fn list_policies(&self) -> Vec<String> {
        self.state.policy_ids()
    }

    /// Get a policy's current state
    pub fn get_policy(&self, id: &str) -> Option<&PolicyState> {
        self.state.get_policy(id)
    }

    /// Poll all watched policies and return events
    ///
    /// This is the main entry point for checking UTXO state changes.
    pub fn poll(&mut self) -> Result<Vec<WatchEvent>, WatchError> {
        // Rate limiting
        let now = current_timestamp();
        if let Some(last) = self.state.last_poll {
            let elapsed = now.saturating_sub(last);
            if elapsed < self.config.min_poll_interval_secs {
                return Err(WatchError::PollTooFrequent {
                    min: self.config.min_poll_interval_secs,
                });
            }
        }

        let mut events = Vec::new();

        // Get current block height
        let current_height = match self.client.get_height() {
            Ok(h) => h,
            Err(e) => {
                events.push(WatchEvent::PollError {
                    message: format!("Failed to get block height: {}", e),
                });
                return Ok(events);
            }
        };

        // Poll each policy
        let policy_ids: Vec<String> = self.state.policy_ids();
        for policy_id in policy_ids {
            match self.poll_policy(&policy_id, current_height) {
                Ok(mut policy_events) => events.append(&mut policy_events),
                Err(e) => {
                    events.push(WatchEvent::PollError {
                        message: format!("Error polling {}: {}", policy_id, e),
                    });
                }
            }
        }

        // Update poll timestamp
        self.state.update_poll(now, current_height);
        self.save_state()?;

        Ok(events)
    }

    /// Poll a single policy
    fn poll_policy(
        &mut self,
        policy_id: &str,
        current_height: u32,
    ) -> Result<Vec<WatchEvent>, WatchError> {
        let mut events = Vec::new();

        // Get policy state — extract needed values upfront to avoid borrow issues
        let (descriptor_str, known_outpoints, utxo_heights, timelock_blocks) = {
            let policy = self
                .state
                .get_policy(policy_id)
                .ok_or_else(|| WatchError::PolicyNotFound(policy_id.to_string()))?;

            let descriptor_str = policy.descriptor.clone();
            let known_outpoints = policy.outpoints();
            // Pre-compute utxo heights for timing analysis
            let utxo_heights: Vec<(OutPoint, u32)> = policy
                .utxos
                .iter()
                .map(|u| (u.outpoint, u.height))
                .collect();
            let timelock_blocks = policy.timelock_blocks;

            (descriptor_str, known_outpoints, utxo_heights, timelock_blocks)
        };

        // Parse descriptor and get script
        let descriptor: Descriptor<DescriptorPublicKey> = Descriptor::from_str(&descriptor_str)
            .map_err(|e| WatchError::InvalidDescriptor(e.to_string()))?;

        // Derive address at index 0
        let script = derive_script(&descriptor, 0)?;

        // Get current UTXOs from blockchain
        let current_utxos: Vec<Utxo> = self.client.get_utxos_for_script(&script)?;

        // Detect new UTXOs (appeared)
        let now = current_timestamp();
        for utxo in &current_utxos {
            if !known_outpoints.contains(&utxo.outpoint) {
                events.push(WatchEvent::UtxoAppeared {
                    policy_id: policy_id.to_string(),
                    outpoint: utxo.outpoint,
                    value: utxo.value,
                    height: utxo.height,
                });

                // Add to state
                if let Some(policy_mut) = self.state.get_policy_mut(policy_id) {
                    policy_mut.add_utxo(TrackedUtxo {
                        outpoint: utxo.outpoint,
                        value: utxo.value,
                        height: utxo.height,
                        first_seen: now,
                    });
                }
            }
        }

        // Detect spent UTXOs
        let current_outpoints: Vec<OutPoint> = current_utxos.iter().map(|u| u.outpoint).collect();
        for known in &known_outpoints {
            if !current_outpoints.contains(known) {
                // Get UTXO height for timing analysis
                let utxo_height = utxo_heights
                    .iter()
                    .find(|(op, _)| op == known)
                    .map(|(_, h)| *h)
                    .unwrap_or(0);

                // UTXO was spent - determine how via witness + timing analysis
                let (spend_type, spending_txid) = self.detect_spend_type_for_utxo(
                    known,
                    &script,
                    utxo_height,
                    timelock_blocks,
                );

                events.push(WatchEvent::UtxoSpent {
                    policy_id: policy_id.to_string(),
                    outpoint: *known,
                    spending_txid,
                    spend_type,
                });

                // Remove from state
                if let Some(policy_mut) = self.state.get_policy_mut(policy_id) {
                    policy_mut.remove_utxo(known);
                }
            }
        }

        // Check timelock warning
        if let Some(policy) = self.state.get_policy(policy_id) {
            if let Some(blocks_remaining) = policy.blocks_until_expiry(current_height) {
                if blocks_remaining <= self.config.warning_threshold_blocks && blocks_remaining > 0
                {
                    let days_remaining = blocks_remaining as f64 * 10.0 / 60.0 / 24.0;
                    events.push(WatchEvent::TimelockWarning {
                        policy_id: policy_id.to_string(),
                        blocks_remaining,
                        days_remaining,
                    });
                }
            }
        }

        Ok(events)
    }

    /// Detect how a UTXO was spent by analyzing the spending transaction's witness.
    ///
    /// Fetches the script history to find the spending transaction, then
    /// analyzes the witness data to determine owner vs heir path.
    fn detect_spend_type_for_utxo(
        &self,
        outpoint: &OutPoint,
        script: &ScriptBuf,
        utxo_height: u32,
        timelock_blocks: u32,
    ) -> (SpendType, Txid) {
        // Find the spending transaction by looking at script history
        match self.find_spending_tx(outpoint, script) {
            Some((spending_tx, spend_height)) => {
                // Analyze the witness of the input that spent our UTXO
                if let Some(analysis) = spend_analysis::analyze_transaction_for_outpoint(
                    &spending_tx,
                    &outpoint.txid,
                    outpoint.vout,
                ) {
                    // If witness analysis is inconclusive, try timing
                    if analysis.spend_type == SpendType::Unknown
                        && spend_height > 0
                        && utxo_height > 0
                    {
                        if let Some(timing_type) = spend_analysis::analyze_timing(
                            spend_height,
                            utxo_height,
                            timelock_blocks,
                        ) {
                            return (timing_type, spending_tx.compute_txid());
                        }
                    }
                    (analysis.spend_type, spending_tx.compute_txid())
                } else {
                    (SpendType::Unknown, spending_tx.compute_txid())
                }
            }
            None => (SpendType::Unknown, Txid::all_zeros()),
        }
    }

    /// Find the transaction that spent a given outpoint by scanning script history.
    fn find_spending_tx(
        &self,
        outpoint: &OutPoint,
        script: &ScriptBuf,
    ) -> Option<(bitcoin::Transaction, u32)> {
        // Get all transactions for this script
        let history = self.client.get_script_history(script).ok()?;

        for hist_item in &history {
            // Skip the funding transaction itself
            if hist_item.txid == outpoint.txid {
                continue;
            }

            // Fetch the full transaction
            if let Ok(tx) = self.client.get_transaction(&hist_item.txid) {
                // Check if any input spends our outpoint
                for input in &tx.input {
                    if input.previous_output == *outpoint {
                        return Some((tx, hist_item.height));
                    }
                }
            }
        }
        None
    }

    /// Save state to disk
    fn save_state(&self) -> Result<(), WatchError> {
        self.state.save(&self.config.state_path)?;
        Ok(())
    }

    /// Force a state save (for testing)
    pub fn flush(&self) -> Result<(), WatchError> {
        self.save_state()
    }

    /// Get the current state (for inspection)
    pub fn state(&self) -> &WatchState {
        &self.state
    }
}

/// Derive a script from a descriptor at a given index
fn derive_script(
    descriptor: &Descriptor<DescriptorPublicKey>,
    index: u32,
) -> Result<ScriptBuf, WatchError> {
    use miniscript::descriptor::DefiniteDescriptorKey;

    let derived: Descriptor<DefiniteDescriptorKey> = descriptor
        .at_derivation_index(index)
        .map_err(|e| WatchError::InvalidDescriptor(e.to_string()))?;

    Ok(derived.script_pubkey())
}

/// Get current unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_config(dir: &std::path::Path) -> WatchConfig {
        WatchConfig {
            state_path: dir.join("watch_state.json"),
            poll_interval_secs: 600,
            min_poll_interval_secs: 0, // Disable rate limiting for tests
            warning_threshold_blocks: 4320,
        }
    }

    // Note: Full integration tests require network access
    // These tests verify the non-network logic

    #[test]
    fn test_config_defaults() {
        let config = WatchConfig::default();
        assert_eq!(config.poll_interval_secs, 600);
        assert_eq!(config.min_poll_interval_secs, 60);
        assert_eq!(config.warning_threshold_blocks, 4320);
    }

    #[test]
    fn test_derive_script() {
        // Test with a simple pk descriptor
        let desc_str = "wsh(pk(xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8/0/*))";
        let descriptor: Descriptor<DescriptorPublicKey> = Descriptor::from_str(desc_str).unwrap();

        let script = derive_script(&descriptor, 0).unwrap();
        assert!(!script.is_empty());
    }

    #[test]
    fn test_watch_state_roundtrip() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());

        // Create state with a policy
        let mut state = WatchState::new();
        state.add_policy(PolicyState::new("test-policy", "wsh(pk(...))", 26280));
        state.save(&config.state_path).unwrap();

        // Load it back
        let loaded = WatchState::load(&config.state_path).unwrap();
        assert!(loaded.get_policy("test-policy").is_some());
    }

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        // Should be after 2024
        assert!(ts > 1700000000);
    }

    #[test]
    fn test_add_remove_policy() {
        let dir = tempdir().unwrap();
        let _config = test_config(dir.path());

        // Create a mock-friendly service by loading state directly
        let mut state = WatchState::new();

        // Add policy
        let descriptor = "wsh(pk(xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8/0/*))";
        state.add_policy(PolicyState::new("test-inheritance", descriptor, 26280));

        assert_eq!(state.policy_ids().len(), 1);
        assert!(state.get_policy("test-inheritance").is_some());

        // Remove policy
        let removed = state.remove_policy("test-inheritance");
        assert!(removed.is_some());
        assert!(state.policy_ids().is_empty());
    }

    #[test]
    fn test_rate_limiting() {
        // Test that rate limiting config is respected
        let config = WatchConfig {
            state_path: std::path::PathBuf::from("/tmp/test"),
            poll_interval_secs: 600,
            min_poll_interval_secs: 60,
            warning_threshold_blocks: 4320,
        };

        assert_eq!(config.min_poll_interval_secs, 60);
        // Actual rate limiting is tested in integration test below
    }

    // =========================================================================
    // Integration Tests (require network access)
    // Run with: cargo test --package nostring-watch -- --ignored
    // =========================================================================

    #[test]
    #[ignore = "requires network access"]
    fn test_poll_mainnet() {
        use nostring_electrum::ElectrumClient;

        let dir = tempdir().unwrap();
        let config = WatchConfig {
            state_path: dir.path().join("watch_state.json"),
            poll_interval_secs: 600,
            min_poll_interval_secs: 0, // Disable for test
            warning_threshold_blocks: 4320,
        };

        // Connect to mainnet
        let client = ElectrumClient::new("ssl://blockstream.info:700", Network::Bitcoin)
            .expect("Failed to connect to Electrum");

        let mut service = WatchService::new(client, config).expect("Failed to create WatchService");

        // Add a test policy (this xpub won't have real UTXOs)
        let descriptor = "wsh(pk(xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8/0/*))";
        service
            .add_policy("test-policy", descriptor, 26280)
            .expect("Failed to add policy");

        // Poll should succeed (even if no UTXOs found)
        let events = service.poll().expect("Poll failed");

        // Should have polled successfully
        assert!(service.state().last_poll.is_some());
        assert!(service.state().last_height.is_some());

        // Height should be reasonable (mainnet ~935k as of Feb 2026)
        let height = service.state().last_height.unwrap();
        assert!(height > 930000, "Height {} is too low", height);
        assert!(height < 960000, "Height {} is too high", height);

        // No UTXOs for test xpub, so no UtxoAppeared events
        // But we should not have errors
        let errors: Vec<_> = events.iter().filter(|e| e.is_error()).collect();
        assert!(errors.is_empty(), "Poll returned errors: {:?}", errors);

        println!("✓ Poll successful at height {}", height);
        println!("✓ Events: {:?}", events);
    }

    #[test]
    #[ignore = "requires network access"]
    fn test_poll_rate_limiting() {
        use nostring_electrum::ElectrumClient;

        let dir = tempdir().unwrap();
        let config = WatchConfig {
            state_path: dir.path().join("watch_state.json"),
            poll_interval_secs: 600,
            min_poll_interval_secs: 60, // Enable rate limiting
            warning_threshold_blocks: 4320,
        };

        let client = ElectrumClient::new("ssl://blockstream.info:700", Network::Bitcoin)
            .expect("Failed to connect to Electrum");

        let mut service = WatchService::new(client, config).expect("Failed to create WatchService");

        // First poll should succeed
        let result1 = service.poll();
        assert!(result1.is_ok(), "First poll should succeed");

        // Immediate second poll should fail (rate limited)
        let result2 = service.poll();
        assert!(result2.is_err(), "Second poll should be rate limited");

        match result2 {
            Err(WatchError::PollTooFrequent { min }) => {
                assert_eq!(min, 60);
                println!("✓ Rate limiting enforced (min {} seconds)", min);
            }
            other => panic!("Expected PollTooFrequent, got {:?}", other),
        }
    }
}
