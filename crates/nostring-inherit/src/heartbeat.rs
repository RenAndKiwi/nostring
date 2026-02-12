//! Deadman heartbeat evaluation for inheritable vaults.
//!
//! Pure logic — no I/O, no network, no async. Takes blockchain state,
//! returns a recommendation. The caller (Tauri app, daemon) decides
//! whether to act on it.
//!
//! # How It Works
//!
//! CSV timelocks are relative to UTXO confirmation height. Every key-path
//! spend resets the clock. The heartbeat module evaluates how much of the
//! timelock has elapsed and recommends action:
//!
//! ```text
//! |--- Healthy ---|--- CheckinRecommended ---|--- CheckinRequired ---|--- Expired
//! 0%             50%                        90%                    100%
//! ```
//!
//! Thresholds are configurable.

use crate::checkin::{CheckinUrgency, TimelockStatus};
use crate::taproot::InheritableVault;
use serde::{Deserialize, Serialize};

/// Heartbeat configuration — when to recommend check-in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    /// Fraction of timelock elapsed before recommending check-in (0.0–1.0).
    /// Default: 0.5 (halfway point).
    pub checkin_threshold: f64,

    /// Fraction of timelock elapsed before check-in is critical (0.0–1.0).
    /// Default: 0.9.
    pub critical_threshold: f64,

    /// How often the caller should poll blockchain height (seconds).
    /// This is advisory — the heartbeat module doesn't poll itself.
    /// Default: 3600 (1 hour).
    pub poll_interval_secs: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            checkin_threshold: 0.5,
            critical_threshold: 0.9,
            poll_interval_secs: 3600,
        }
    }
}

impl HeartbeatConfig {
    /// Validate that thresholds are sensible.
    pub fn validate(&self) -> Result<(), HeartbeatError> {
        if self.checkin_threshold <= 0.0 || self.checkin_threshold >= 1.0 {
            return Err(HeartbeatError::InvalidThreshold(
                "checkin_threshold must be between 0.0 and 1.0 exclusive".into(),
            ));
        }
        if self.critical_threshold <= self.checkin_threshold || self.critical_threshold >= 1.0 {
            return Err(HeartbeatError::InvalidThreshold(
                "critical_threshold must be between checkin_threshold and 1.0 exclusive".into(),
            ));
        }
        Ok(())
    }
}

/// What the heartbeat recommends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeartbeatAction {
    /// Timelock is far from expiry. No action needed.
    Healthy,
    /// Passed the check-in threshold. Should check in soon.
    CheckinRecommended,
    /// Passed the critical threshold. Must check in now.
    CheckinRequired,
    /// Timelock expired. Heir can claim. Too late for check-in.
    Expired,
}

impl HeartbeatAction {
    /// Map to the existing `CheckinUrgency` for compatibility.
    pub fn to_urgency(self) -> CheckinUrgency {
        match self {
            HeartbeatAction::Healthy => CheckinUrgency::None,
            HeartbeatAction::CheckinRecommended => CheckinUrgency::Warning,
            HeartbeatAction::CheckinRequired => CheckinUrgency::Critical,
            HeartbeatAction::Expired => CheckinUrgency::Expired,
        }
    }
}

/// Full heartbeat status for a vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatStatus {
    /// The vault address (for display).
    pub vault_address: String,
    /// Timelock status (blocks remaining, expired, etc.).
    pub timelock_status: TimelockStatus,
    /// Fraction of timelock elapsed (0.0–1.0+).
    pub elapsed_fraction: f64,
    /// Recommended action.
    pub action: HeartbeatAction,
}

/// Errors from heartbeat evaluation.
#[derive(Debug, thiserror::Error)]
pub enum HeartbeatError {
    #[error("Invalid threshold: {0}")]
    InvalidThreshold(String),
}

/// Evaluate the heartbeat status of an inheritable vault.
///
/// Pure function: takes blockchain state, returns a recommendation.
///
/// # Arguments
/// * `vault` — the inheritable vault
/// * `utxo_height` — block height when the vault UTXO was confirmed
/// * `current_height` — current blockchain tip height
/// * `config` — heartbeat thresholds
///
/// # Returns
/// `HeartbeatStatus` with the recommended action.
pub fn evaluate_heartbeat(
    vault: &InheritableVault,
    utxo_height: u32,
    current_height: u32,
    config: &HeartbeatConfig,
) -> HeartbeatStatus {
    let timelock_blocks = vault.timelock.blocks();
    let timelock_status = TimelockStatus::calculate(current_height, utxo_height, timelock_blocks);

    let blocks_elapsed = current_height.saturating_sub(utxo_height);
    let elapsed_fraction = if timelock_blocks == 0 {
        1.0 // Degenerate case: zero timelock is always expired
    } else {
        blocks_elapsed as f64 / timelock_blocks as f64
    };

    let action = if timelock_status.expired {
        HeartbeatAction::Expired
    } else if elapsed_fraction >= config.critical_threshold {
        HeartbeatAction::CheckinRequired
    } else if elapsed_fraction >= config.checkin_threshold {
        HeartbeatAction::CheckinRecommended
    } else {
        HeartbeatAction::Healthy
    };

    HeartbeatStatus {
        vault_address: vault.address.to_string(),
        timelock_status,
        elapsed_fraction,
        action,
    }
}

/// Evaluate heartbeat for a cascade vault (multiple timelocks).
///
/// Returns the status for the **earliest** (most urgent) timelock.
/// If any recovery path is expired, that's the one that matters.
pub fn evaluate_cascade_heartbeat(
    vault: &InheritableVault,
    utxo_height: u32,
    current_height: u32,
    config: &HeartbeatConfig,
) -> HeartbeatStatus {
    // For cascade vaults, the earliest timelock is the most urgent.
    // InheritableVault stores recovery_scripts sorted by timelock (left-leaning tree).
    // The vault's `timelock` field is the primary (earliest) timelock.
    evaluate_heartbeat(vault, utxo_height, current_height, config)
}

/// Batch evaluate heartbeat for multiple vaults.
///
/// Returns statuses sorted by urgency (most urgent first).
pub fn evaluate_batch(
    vaults: &[(InheritableVault, u32)], // (vault, utxo_height)
    current_height: u32,
    config: &HeartbeatConfig,
) -> Vec<HeartbeatStatus> {
    let mut statuses: Vec<HeartbeatStatus> = vaults
        .iter()
        .map(|(vault, utxo_height)| evaluate_heartbeat(vault, *utxo_height, current_height, config))
        .collect();

    // Sort: Expired first, then CheckinRequired, then CheckinRecommended, then Healthy
    statuses.sort_by(|a, b| {
        let priority = |action: &HeartbeatAction| -> u8 {
            match action {
                HeartbeatAction::Expired => 0,
                HeartbeatAction::CheckinRequired => 1,
                HeartbeatAction::CheckinRecommended => 2,
                HeartbeatAction::Healthy => 3,
            }
        };
        priority(&a.action).cmp(&priority(&b.action)).then(
            a.elapsed_fraction
                .partial_cmp(&b.elapsed_fraction)
                .unwrap_or(std::cmp::Ordering::Equal)
                .reverse(),
        )
    });

    statuses
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::PathInfo;
    use crate::test_utils::{make_test_vault, test_chain_code, test_keypair};
    use bitcoin::Network;
    use miniscript::descriptor::DescriptorPublicKey;
    use nostring_ccd::register_cosigner_with_chain_code;
    use std::str::FromStr;

    #[test]
    fn test_healthy_status() {
        let vault = make_test_vault(1000);
        let config = HeartbeatConfig::default();
        let status = evaluate_heartbeat(&vault, 100, 200, &config);

        assert_eq!(status.action, HeartbeatAction::Healthy);
        assert!((status.elapsed_fraction - 0.1).abs() < 0.001);
        assert!(!status.timelock_status.expired);
    }

    #[test]
    fn test_checkin_recommended() {
        let vault = make_test_vault(1000);
        let config = HeartbeatConfig::default(); // threshold at 0.5
                                                 // 600 of 1000 blocks elapsed = 0.6
        let status = evaluate_heartbeat(&vault, 100, 700, &config);

        assert_eq!(status.action, HeartbeatAction::CheckinRecommended);
        assert!((status.elapsed_fraction - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_checkin_required() {
        let vault = make_test_vault(1000);
        let config = HeartbeatConfig::default(); // critical at 0.9
                                                 // 950 of 1000 blocks elapsed = 0.95
        let status = evaluate_heartbeat(&vault, 100, 1050, &config);

        assert_eq!(status.action, HeartbeatAction::CheckinRequired);
        assert!((status.elapsed_fraction - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_expired() {
        let vault = make_test_vault(1000);
        let config = HeartbeatConfig::default();
        // 1100 of 1000 blocks elapsed = 1.1 (past expiry)
        let status = evaluate_heartbeat(&vault, 100, 1200, &config);

        assert_eq!(status.action, HeartbeatAction::Expired);
        assert!(status.timelock_status.expired);
    }

    #[test]
    fn test_zero_blocks_elapsed() {
        let vault = make_test_vault(1000);
        let config = HeartbeatConfig::default();
        let status = evaluate_heartbeat(&vault, 100, 100, &config);

        assert_eq!(status.action, HeartbeatAction::Healthy);
        assert!((status.elapsed_fraction).abs() < 0.001);
    }

    #[test]
    fn test_exactly_at_checkin_threshold() {
        let vault = make_test_vault(1000);
        let config = HeartbeatConfig::default(); // threshold at 0.5
                                                 // Exactly 500 of 1000 = 0.5
        let status = evaluate_heartbeat(&vault, 100, 600, &config);

        assert_eq!(status.action, HeartbeatAction::CheckinRecommended);
    }

    #[test]
    fn test_exactly_at_critical_threshold() {
        let vault = make_test_vault(1000);
        let config = HeartbeatConfig::default(); // critical at 0.9
                                                 // Exactly 900 of 1000 = 0.9
        let status = evaluate_heartbeat(&vault, 100, 1000, &config);

        assert_eq!(status.action, HeartbeatAction::CheckinRequired);
    }

    #[test]
    fn test_custom_thresholds() {
        let vault = make_test_vault(1000);
        let config = HeartbeatConfig {
            checkin_threshold: 0.3,
            critical_threshold: 0.7,
            poll_interval_secs: 600,
        };

        // 350 of 1000 = 0.35 (past 0.3 threshold)
        let status = evaluate_heartbeat(&vault, 100, 450, &config);
        assert_eq!(status.action, HeartbeatAction::CheckinRecommended);

        // 750 of 1000 = 0.75 (past 0.7 critical)
        let status = evaluate_heartbeat(&vault, 100, 850, &config);
        assert_eq!(status.action, HeartbeatAction::CheckinRequired);
    }

    #[test]
    fn test_config_validation() {
        let bad1 = HeartbeatConfig {
            checkin_threshold: 0.0,
            critical_threshold: 0.9,
            poll_interval_secs: 3600,
        };
        assert!(bad1.validate().is_err());

        let bad2 = HeartbeatConfig {
            checkin_threshold: 0.5,
            critical_threshold: 0.4, // less than checkin
            poll_interval_secs: 3600,
        };
        assert!(bad2.validate().is_err());

        let bad3 = HeartbeatConfig {
            checkin_threshold: 0.5,
            critical_threshold: 1.0, // not exclusive
            poll_interval_secs: 3600,
        };
        assert!(bad3.validate().is_err());

        let good = HeartbeatConfig::default();
        assert!(good.validate().is_ok());
    }

    #[test]
    fn test_action_to_urgency() {
        assert_eq!(HeartbeatAction::Healthy.to_urgency(), CheckinUrgency::None);
        assert_eq!(
            HeartbeatAction::CheckinRecommended.to_urgency(),
            CheckinUrgency::Warning
        );
        assert_eq!(
            HeartbeatAction::CheckinRequired.to_urgency(),
            CheckinUrgency::Critical
        );
        assert_eq!(
            HeartbeatAction::Expired.to_urgency(),
            CheckinUrgency::Expired
        );
    }

    #[test]
    fn test_batch_evaluation_sorted_by_urgency() {
        let vault1 = make_test_vault(1000);
        let vault2 = make_test_vault(1000);
        let vault3 = make_test_vault(1000);
        let config = HeartbeatConfig::default();

        let vaults = vec![
            (vault1, 500), // healthy (current_height - 500 = 100 blocks elapsed)
            (vault2, 100), // critical (500 blocks elapsed = 0.5, recommended)
            (vault3, 0),   // expired (600 blocks elapsed > 1000? No, 600/1000=0.6 recommended)
        ];

        // current_height = 600
        let statuses = evaluate_batch(&vaults, 600, &config);
        assert_eq!(statuses.len(), 3);

        // vault at height 500: 100/1000 = 0.1 → Healthy
        // vault at height 100: 500/1000 = 0.5 → CheckinRecommended
        // vault at height 0: 600/1000 = 0.6 → CheckinRecommended
        // Sorted: the two recommended first (higher elapsed first), then healthy
        assert_eq!(statuses[0].action, HeartbeatAction::CheckinRecommended);
        assert_eq!(statuses[1].action, HeartbeatAction::CheckinRecommended);
        assert_eq!(statuses[2].action, HeartbeatAction::Healthy);
        // Higher elapsed fraction first
        assert!(statuses[0].elapsed_fraction > statuses[1].elapsed_fraction);
    }

    #[test]
    fn test_cascade_uses_earliest_timelock() {
        // Cascade vault with 3-month and 6-month timelocks.
        // Heartbeat should evaluate against the 3-month (earliest) one.
        use crate::taproot::create_cascade_vault;

        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let (_heir1_sk, heir1_pk) = test_keypair(3);
        let (_heir2_sk, heir2_pk) = test_keypair(4);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let h1_xonly = heir1_pk.x_only_public_key().0;
        let h2_xonly = heir2_pk.x_only_public_key().0;
        let h1_desc = DescriptorPublicKey::from_str(&format!("{}", h1_xonly)).unwrap();
        let h2_desc = DescriptorPublicKey::from_str(&format!("{}", h2_xonly)).unwrap();

        use crate::policy::Timelock;
        let three_months = Timelock::from_blocks(13_140).unwrap(); // ~3 months
        let six_months = Timelock::six_months(); // 26,280 blocks

        let vault = create_cascade_vault(
            &owner_pk,
            &delegated,
            0,
            vec![
                (three_months, PathInfo::Single(h1_desc)),
                (six_months, PathInfo::Single(h2_desc)),
            ],
            0,
            Network::Testnet,
        )
        .unwrap();

        // Vault's primary timelock should be 3 months (earliest)
        assert_eq!(vault.timelock.blocks(), 13_140);

        let config = HeartbeatConfig::default();

        // At 50% of 3-month timelock (6,570 blocks): should be CheckinRecommended
        let status = evaluate_heartbeat(&vault, 800_000, 806_570, &config);
        assert_eq!(status.action, HeartbeatAction::CheckinRecommended);

        // At 50% of 6-month timelock but only 50% of 3-month: same answer
        // because we evaluate against the earliest timelock
        let status2 = evaluate_cascade_heartbeat(&vault, 800_000, 806_570, &config);
        assert_eq!(status2.action, HeartbeatAction::CheckinRecommended);
    }

    #[test]
    fn test_six_month_vault_realistic() {
        // 6 months ≈ 26,280 blocks
        let vault = make_test_vault(26_280);
        let config = HeartbeatConfig::default();

        // Just created
        let status = evaluate_heartbeat(&vault, 800_000, 800_000, &config);
        assert_eq!(status.action, HeartbeatAction::Healthy);

        // 3 months in (halfway)
        let status = evaluate_heartbeat(&vault, 800_000, 813_140, &config);
        assert_eq!(status.action, HeartbeatAction::CheckinRecommended);

        // 5.5 months in (~95%)
        let status = evaluate_heartbeat(&vault, 800_000, 824_966, &config);
        assert_eq!(status.action, HeartbeatAction::CheckinRequired);

        // 6+ months (expired)
        let status = evaluate_heartbeat(&vault, 800_000, 826_281, &config);
        assert_eq!(status.action, HeartbeatAction::Expired);
    }
}
