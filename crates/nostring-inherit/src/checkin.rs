//! Check-in mechanism for resetting inheritance timelocks
//!
//! The check-in process works by spending the inheritance UTXO and recreating
//! it with a fresh timelock. This proves the owner is still in control.
//!
//! # How it works
//!
//! 1. Owner has a UTXO locked with the inheritance policy
//! 2. Owner spends that UTXO using the primary (immediate) path
//! 3. Owner creates a new UTXO with the same policy
//! 4. The heir's timelock resets (since CSV is relative to UTXO age)
//!
//! # Optimization: Batching
//!
//! In practice, any transaction that spends the inheritance UTXO resets the
//! timelock. So if the owner makes a regular payment from this wallet,
//! the check-in happens automatically.

use bitcoin::absolute::LockTime;
use bitcoin::transaction::Version;
use bitcoin::{Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness};
use miniscript::descriptor::DescriptorPublicKey;
use miniscript::Descriptor;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CheckinError {
    #[error("No UTXO to check in")]
    NoUtxo,

    #[error("Insufficient funds for check-in (need {needed}, have {available})")]
    InsufficientFunds { needed: Amount, available: Amount },

    #[error("PSBT creation failed: {0}")]
    PsbtError(String),

    #[error("Policy error: {0}")]
    PolicyError(#[from] crate::policy::PolicyError),
}

/// Status of the inheritance timelock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelockStatus {
    /// Current block height
    pub current_height: u32,
    /// Block height when UTXO was created
    pub utxo_height: u32,
    /// Block height when heir can spend
    pub unlock_height: u32,
    /// Blocks remaining until heir can spend
    pub blocks_remaining: i32,
    /// Approximate time remaining (in seconds)
    pub seconds_remaining: i64,
    /// Whether the timelock has expired (heir can spend)
    pub expired: bool,
}

impl TimelockStatus {
    /// Calculate status from UTXO age and timelock
    pub fn calculate(current_height: u32, utxo_height: u32, timelock_blocks: u16) -> Self {
        let unlock_height = utxo_height.saturating_add(timelock_blocks as u32);
        let blocks_remaining = unlock_height as i32 - current_height as i32;
        let seconds_remaining = blocks_remaining as i64 * 600; // ~10 min per block

        Self {
            current_height,
            utxo_height,
            unlock_height,
            blocks_remaining,
            seconds_remaining,
            expired: blocks_remaining <= 0,
        }
    }

    /// Human-readable time remaining
    pub fn time_remaining_display(&self) -> String {
        if self.expired {
            return "EXPIRED - Heir can spend!".to_string();
        }

        let days = self.blocks_remaining / 144;
        let hours = (self.blocks_remaining % 144) / 6;

        if days > 365 {
            format!("~{:.1} years", days as f32 / 365.0)
        } else if days > 30 {
            format!("~{:.1} months", days as f32 / 30.0)
        } else if days > 0 {
            format!("~{} days, {} hours", days, hours)
        } else {
            format!("~{} hours", hours)
        }
    }

    /// Urgency level for check-in reminders
    pub fn urgency(&self) -> CheckinUrgency {
        let days = self.blocks_remaining / 144;

        if self.expired {
            CheckinUrgency::Expired
        } else if days <= 7 {
            CheckinUrgency::Critical
        } else if days <= 30 {
            CheckinUrgency::Warning
        } else if days <= 90 {
            CheckinUrgency::Normal
        } else {
            CheckinUrgency::None
        }
    }
}

/// Urgency level for check-in
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckinUrgency {
    /// No action needed
    None,
    /// Routine reminder
    Normal,
    /// Should check in soon
    Warning,
    /// Must check in immediately
    Critical,
    /// Timelock expired, heir can spend
    Expired,
}

/// An inheritance UTXO being tracked
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceUtxo {
    /// The outpoint txid (hex)
    pub txid: String,
    /// The outpoint vout
    pub vout: u32,
    /// Value in satoshis
    pub value_sats: u64,
    /// Block height when confirmed
    pub confirmation_height: u32,
    /// The script pubkey (hex)
    pub script_pubkey_hex: String,
    /// Timestamp when last checked
    pub last_checked: u64,
}

impl InheritanceUtxo {
    /// Create a new inheritance UTXO
    pub fn new(
        outpoint: OutPoint,
        value: Amount,
        confirmation_height: u32,
        script_pubkey: ScriptBuf,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            txid: outpoint.txid.to_string(),
            vout: outpoint.vout,
            value_sats: value.to_sat(),
            confirmation_height,
            script_pubkey_hex: script_pubkey.to_hex_string(),
            last_checked: now,
        }
    }

    /// Get the outpoint
    pub fn outpoint(&self) -> OutPoint {
        OutPoint {
            txid: bitcoin::Txid::from_str(&self.txid).expect("valid txid"),
            vout: self.vout,
        }
    }

    /// Get the value
    pub fn value(&self) -> Amount {
        Amount::from_sat(self.value_sats)
    }

    /// Get the script pubkey
    pub fn script_pubkey(&self) -> ScriptBuf {
        ScriptBuf::from_hex(&self.script_pubkey_hex).expect("valid script")
    }

    /// Get timelock status
    pub fn status(&self, current_height: u32, timelock_blocks: u16) -> TimelockStatus {
        TimelockStatus::calculate(current_height, self.confirmation_height, timelock_blocks)
    }
}

/// Builder for check-in transactions
pub struct CheckinTxBuilder {
    /// The UTXO to spend
    utxo: InheritanceUtxo,
    /// The descriptor for this UTXO
    descriptor: Descriptor<DescriptorPublicKey>,
    /// Fee rate in sat/vbyte
    fee_rate: u64,
    /// Optional additional outputs (e.g., if sending funds elsewhere)
    extra_outputs: Vec<TxOut>,
}

impl CheckinTxBuilder {
    /// Create a new check-in transaction builder
    pub fn new(
        utxo: InheritanceUtxo,
        descriptor: Descriptor<DescriptorPublicKey>,
        fee_rate: u64,
    ) -> Self {
        Self {
            utxo,
            descriptor,
            fee_rate,
            extra_outputs: Vec::new(),
        }
    }

    /// Add an extra output (for payments during check-in)
    pub fn with_output(mut self, output: TxOut) -> Self {
        self.extra_outputs.push(output);
        self
    }

    /// Calculate the fee for this transaction
    fn estimate_fee(&self) -> Amount {
        // Estimate vbytes based on P2WSH spend
        // Input: ~138 vbytes for P2WSH multisig
        // Output: ~43 vbytes for P2WSH
        let input_vbytes = 138u64;
        let output_vbytes = 43u64 * (1 + self.extra_outputs.len() as u64);
        let overhead = 11u64; // version, locktime, counts

        let total_vbytes = input_vbytes + output_vbytes + overhead;
        Amount::from_sat(total_vbytes * self.fee_rate)
    }

    /// Build an unsigned transaction for the check-in
    pub fn build_unsigned_tx(&self) -> Result<Transaction, CheckinError> {
        let fee = self.estimate_fee();
        let utxo_value = self.utxo.value();

        // Calculate change
        let extra_output_total: Amount = self.extra_outputs.iter().map(|o| o.value).sum();
        let change = utxo_value
            .checked_sub(fee)
            .and_then(|v| v.checked_sub(extra_output_total))
            .ok_or(CheckinError::InsufficientFunds {
                needed: fee + extra_output_total,
                available: utxo_value,
            })?;

        // Build transaction
        let mut outputs = self.extra_outputs.clone();
        outputs.push(TxOut {
            value: change,
            script_pubkey: self.utxo.script_pubkey(), // Same address for check-in
        });

        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: self.utxo.outpoint(),
                script_sig: ScriptBuf::new(), // Empty for SegWit
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::default(),
            }],
            output: outputs,
        };

        Ok(tx)
    }

    /// Get the descriptor
    #[allow(dead_code)]
    pub fn descriptor(&self) -> &Descriptor<DescriptorPublicKey> {
        &self.descriptor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::hashes::Hash;
    use bitcoin::Txid;

    #[test]
    fn test_timelock_status_calculation() {
        // UTXO at height 800,000, current height 810,000
        // Timelock: 26,280 blocks (~6 months)
        let status = TimelockStatus::calculate(810_000, 800_000, 26_280);

        assert_eq!(status.utxo_height, 800_000);
        assert_eq!(status.unlock_height, 826_280);
        assert_eq!(status.blocks_remaining, 16_280); // 826,280 - 810,000
        assert!(!status.expired);
    }

    #[test]
    fn test_timelock_expired() {
        // UTXO at height 800,000, current height 830,000
        // Timelock: 26,280 blocks (~6 months)
        let status = TimelockStatus::calculate(830_000, 800_000, 26_280);

        assert!(status.expired);
        assert!(status.blocks_remaining < 0);
        assert_eq!(status.urgency(), CheckinUrgency::Expired);
    }

    #[test]
    fn test_urgency_levels() {
        // Test different urgency levels based on blocks remaining

        // ~100 days remaining (14,400 blocks) - more than 90 days = None
        let status = TimelockStatus::calculate(100_000, 100_000, 14_400);
        assert_eq!(status.urgency(), CheckinUrgency::None);

        // ~20 days remaining
        let status = TimelockStatus::calculate(100_000, 100_000, 2_880);
        assert_eq!(status.urgency(), CheckinUrgency::Warning);

        // ~5 days remaining
        let status = TimelockStatus::calculate(100_000, 100_000, 720);
        assert_eq!(status.urgency(), CheckinUrgency::Critical);
    }

    #[test]
    fn test_time_remaining_display() {
        let status = TimelockStatus::calculate(0, 0, 26_280);
        let display = status.time_remaining_display();
        assert!(display.contains("months"));
    }

    #[test]
    fn test_inheritance_utxo() {
        let outpoint = OutPoint {
            txid: Txid::all_zeros(),
            vout: 0,
        };
        let utxo = InheritanceUtxo::new(
            outpoint,
            Amount::from_sat(100_000),
            800_000,
            ScriptBuf::new(),
        );

        let status = utxo.status(810_000, 26_280);
        assert!(!status.expired);
    }

    #[test]
    fn test_utxo_serde_roundtrip() {
        let outpoint = OutPoint {
            txid: Txid::all_zeros(),
            vout: 0,
        };
        let utxo = InheritanceUtxo::new(
            outpoint,
            Amount::from_sat(100_000),
            800_000,
            ScriptBuf::new(),
        );

        let json = serde_json::to_string(&utxo).unwrap();
        let restored: InheritanceUtxo = serde_json::from_str(&json).unwrap();

        assert_eq!(utxo.txid, restored.txid);
        assert_eq!(utxo.vout, restored.vout);
        assert_eq!(utxo.value_sats, restored.value_sats);
    }
}
