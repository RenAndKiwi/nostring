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
use bitcoin::bip32::{ChildNumber, DerivationPath, Fingerprint};
use bitcoin::psbt::Psbt;
use bitcoin::secp256k1;
use bitcoin::transaction::Version;
use bitcoin::{Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness};
use miniscript::descriptor::DescriptorPublicKey;
use miniscript::{Descriptor, ForEachKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// Derivation index for the UTXO address (which child key was used)
    derivation_index: u32,
    /// Optional additional outputs (e.g., if sending funds elsewhere)
    extra_outputs: Vec<TxOut>,
}

impl CheckinTxBuilder {
    /// Create a new check-in transaction builder
    ///
    /// `derivation_index` is the BIP-32 child index at which the UTXO's
    /// address was derived from the descriptor (e.g., 0 for the first
    /// receive address).
    pub fn new(
        utxo: InheritanceUtxo,
        descriptor: Descriptor<DescriptorPublicKey>,
        fee_rate: u64,
        derivation_index: u32,
    ) -> Self {
        Self {
            utxo,
            descriptor,
            fee_rate,
            derivation_index,
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

    /// Build an unsigned PSBT for the check-in
    ///
    /// The PSBT can be exported to SeedSigner or other hardware wallets for signing.
    /// Populates BIP-174 `witness_utxo` and `witness_script` fields so hardware
    /// wallets can validate input amounts (prevents fee-manipulation attacks).
    pub fn build_psbt(&self) -> Result<Psbt, CheckinError> {
        let tx = self.build_unsigned_tx()?;

        let mut psbt =
            Psbt::from_unsigned_tx(tx).map_err(|e| CheckinError::PsbtError(e.to_string()))?;

        // Populate witness_utxo: the TxOut being spent (amount + scriptPubKey).
        // Without this, hardware wallets cannot verify the input amount and
        // are vulnerable to fee-manipulation attacks (BIP-174 §input.witness_utxo).
        psbt.inputs[0].witness_utxo = Some(TxOut {
            value: self.utxo.value(),
            script_pubkey: self.utxo.script_pubkey(),
        });

        // Populate witness_script: the redeemScript for P2WSH inputs.
        // For P2WSH, the scriptPubKey is OP_0 <32-byte-hash>, and the
        // witness_script is the actual script that hashes to that value.
        // Hardware wallets need this to construct the correct sighash.
        //
        // Derive the descriptor at the UTXO's derivation index to resolve
        // wildcard keys (<0;1>/*) into concrete public keys, then extract
        // the inner witness script.
        let secp = bitcoin::secp256k1::Secp256k1::verification_only();

        // For multi-path descriptors (<0;1>/*), split into single-path
        // descriptors and use the receive path (index 0).
        let single_descs = self
            .descriptor
            .clone()
            .into_single_descriptors()
            .map_err(|e| CheckinError::PsbtError(format!("descriptor split failed: {}", e)))?;
        let receive_desc = single_descs
            .into_iter()
            .next()
            .ok_or_else(|| CheckinError::PsbtError("empty descriptor list".to_string()))?;

        let derived = receive_desc
            .derived_descriptor(&secp, self.derivation_index)
            .map_err(|e| CheckinError::PsbtError(format!("descriptor derivation failed: {}", e)))?;

        let witness_script = derived.explicit_script().map_err(|e| {
            CheckinError::PsbtError(format!("witness script extraction failed: {}", e))
        })?;

        psbt.inputs[0].witness_script = Some(witness_script);

        // Populate BIP-32 derivation paths (BIP-174 PSBT_IN_BIP32_DERIVATION).
        // This tells hardware wallets which HD key path to use for signing.
        // For each key in the descriptor, we map the derived public key to
        // its (master_fingerprint, full_derivation_path).
        let mut bip32_derivation: BTreeMap<secp256k1::PublicKey, (Fingerprint, DerivationPath)> =
            BTreeMap::new();

        receive_desc.for_each_key(|key| {
            if let DescriptorPublicKey::XPub(ref xkey) = key {
                if let Some((fingerprint, base_path)) = &xkey.origin {
                    // Derive the child pubkey at our derivation index
                    if let Ok(child_xpub) = xkey.xkey.derive_pub(
                        &secp,
                        &[ChildNumber::Normal {
                            index: self.derivation_index,
                        }],
                    ) {
                        let pubkey = child_xpub.public_key;

                        // Full path = origin path + xpub derivation path + child index
                        // e.g., [fingerprint/84'/0'/0']xpub/0/* at index 5 →
                        //        m/84'/0'/0'/0/5
                        let mut full_path: Vec<ChildNumber> = base_path.as_ref().to_vec();
                        for step in xkey.derivation_path.as_ref() {
                            full_path.push(*step);
                        }
                        full_path.push(ChildNumber::Normal {
                            index: self.derivation_index,
                        });

                        bip32_derivation
                            .insert(pubkey, (*fingerprint, DerivationPath::from(full_path)));
                    }
                }
            } else if let DescriptorPublicKey::MultiXPub(ref xkey) = key {
                if let Some((fingerprint, base_path)) = &xkey.origin {
                    // For multi-path xpubs (<0;1>/*), use path index 0 (receive)
                    if let Some(first_path) = xkey.derivation_paths.paths().first() {
                        if let Ok(child_xpub) = xkey.xkey.derive_pub(&secp, first_path) {
                            if let Ok(final_xpub) = child_xpub.derive_pub(
                                &secp,
                                &[ChildNumber::Normal {
                                    index: self.derivation_index,
                                }],
                            ) {
                                let pubkey = final_xpub.public_key;

                                let mut full_path: Vec<ChildNumber> = base_path.as_ref().to_vec();
                                for step in first_path.as_ref() {
                                    full_path.push(*step);
                                }
                                full_path.push(ChildNumber::Normal {
                                    index: self.derivation_index,
                                });

                                bip32_derivation.insert(
                                    pubkey,
                                    (*fingerprint, DerivationPath::from(full_path)),
                                );
                            }
                        }
                    }
                }
            }
            true // continue iterating
        });

        psbt.inputs[0].bip32_derivation = bip32_derivation;

        Ok(psbt)
    }

    /// Build PSBT and serialize to base64
    pub fn build_psbt_base64(&self) -> Result<String, CheckinError> {
        use base64::prelude::*;
        let psbt = self.build_psbt()?;
        Ok(BASE64_STANDARD.encode(psbt.serialize()))
    }

    /// Build PSBT and serialize to bytes (for QR encoding)
    pub fn build_psbt_bytes(&self) -> Result<Vec<u8>, CheckinError> {
        let psbt = self.build_psbt()?;
        Ok(psbt.serialize())
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

    /// Helper: derive the script_pubkey for a descriptor at a given index
    fn derive_script_pubkey(descriptor: &Descriptor<DescriptorPublicKey>, index: u32) -> ScriptBuf {
        let secp = bitcoin::secp256k1::Secp256k1::verification_only();
        let single_descs = descriptor.clone().into_single_descriptors().unwrap();
        let receive_desc = &single_descs[0];
        let derived = receive_desc.derived_descriptor(&secp, index).unwrap();
        derived.script_pubkey()
    }

    #[test]
    fn test_checkin_psbt_generation() {
        use crate::policy::{InheritancePolicy, Timelock};
        use bitcoin::bip32::Xpub;
        use miniscript::descriptor::DescriptorPublicKey;
        use std::str::FromStr;

        // Create test keys
        let test_xpub = Xpub::from_str("xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8").unwrap();
        let owner_key =
            DescriptorPublicKey::from_str(&format!("[00000001/84'/0'/0']{}/<0;1>/*", test_xpub))
                .unwrap();
        let heir_key =
            DescriptorPublicKey::from_str(&format!("[00000002/84'/0'/1']{}/<0;1>/*", test_xpub))
                .unwrap();

        // Create a simple inheritance policy
        let policy =
            InheritancePolicy::simple(owner_key, heir_key, Timelock::six_months()).unwrap();

        let descriptor = policy.to_wsh_descriptor().unwrap();

        // Derive the correct script_pubkey for derivation index 0
        let spk = derive_script_pubkey(&descriptor, 0);

        // Create a test UTXO with the correct script_pubkey
        let outpoint = OutPoint {
            txid: Txid::all_zeros(),
            vout: 0,
        };
        let utxo = InheritanceUtxo::new(outpoint, Amount::from_sat(100_000), 800_000, spk);

        // Build the PSBT (derivation_index = 0)
        let builder = CheckinTxBuilder::new(utxo, descriptor, 10, 0);
        let psbt = builder.build_psbt().expect("PSBT creation should succeed");

        // --- Verify witness_utxo is populated ---
        let witness_utxo = psbt.inputs[0]
            .witness_utxo
            .as_ref()
            .expect("witness_utxo must be populated");
        assert_eq!(witness_utxo.value, Amount::from_sat(100_000));
        assert!(
            witness_utxo.script_pubkey.is_p2wsh(),
            "script_pubkey must be P2WSH"
        );

        // --- Verify witness_script is populated ---
        let witness_script = psbt.inputs[0]
            .witness_script
            .as_ref()
            .expect("witness_script must be populated");
        assert!(
            !witness_script.is_empty(),
            "witness_script must not be empty"
        );

        // --- Verify witness_script hashes to the P2WSH script_pubkey ---
        let expected_wsh =
            ScriptBuf::new_p2wsh(&bitcoin::WScriptHash::hash(witness_script.as_bytes()));
        assert_eq!(
            witness_utxo.script_pubkey, expected_wsh,
            "witness_script must hash to the P2WSH script_pubkey"
        );

        // --- Verify BIP-32 derivation paths are populated ---
        let bip32 = &psbt.inputs[0].bip32_derivation;
        assert!(
            !bip32.is_empty(),
            "bip32_derivation must be populated for hardware wallet signing"
        );
        // Note: test uses same xpub for owner+heir (different origin paths),
        // so derived concrete pubkeys collide in the BTreeMap. In production,
        // different hardware wallets would have distinct xpubs → 2 entries.
        // Here we verify at least 1 valid entry exists.
        assert!(
            bip32.len() >= 1,
            "should have at least one derivation path entry"
        );

        // Verify each entry has the correct fingerprint and path structure
        for (pubkey, (fingerprint, path)) in bip32 {
            // Fingerprint should be one of our test fingerprints
            let fp_bytes = fingerprint.to_bytes();
            assert!(
                fp_bytes == [0, 0, 0, 1] || fp_bytes == [0, 0, 0, 2],
                "unexpected fingerprint: {:?}",
                fingerprint
            );

            // Path should have 5 components: 84'/0'/N'/0/0
            // (origin 84'/0'/N' + receive chain 0 + derivation index 0)
            let path_vec: Vec<_> = path.into_iter().collect();
            assert_eq!(
                path_vec.len(),
                5,
                "derivation path should have 5 components, got {}: {:?}",
                path_vec.len(),
                path
            );

            // Verify the pubkey is a valid secp256k1 point
            assert_eq!(pubkey.serialize().len(), 33, "pubkey should be compressed");
        }

        // Test base64 encoding
        let base64_result = builder.build_psbt_base64();
        assert!(base64_result.is_ok());
        let base64_str = base64_result.unwrap();
        assert!(base64_str.starts_with("cHNidP8")); // PSBT magic in base64

        // Test bytes encoding
        let bytes_result = builder.build_psbt_bytes();
        assert!(bytes_result.is_ok());
        let bytes = bytes_result.unwrap();
        assert_eq!(&bytes[0..5], b"psbt\xff"); // PSBT magic bytes
    }

    #[test]
    fn test_psbt_witness_fields_at_different_derivation_indices() {
        use crate::policy::{InheritancePolicy, Timelock};
        use bitcoin::bip32::Xpub;
        use miniscript::descriptor::DescriptorPublicKey;
        use std::str::FromStr;

        let test_xpub = Xpub::from_str("xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8").unwrap();
        let owner_key =
            DescriptorPublicKey::from_str(&format!("[00000001/84'/0'/0']{}/<0;1>/*", test_xpub))
                .unwrap();
        let heir_key =
            DescriptorPublicKey::from_str(&format!("[00000002/84'/0'/1']{}/<0;1>/*", test_xpub))
                .unwrap();

        let policy =
            InheritancePolicy::simple(owner_key, heir_key, Timelock::six_months()).unwrap();
        let descriptor = policy.to_wsh_descriptor().unwrap();

        // Test indices 0, 1, 2 — each should produce different witness scripts
        // that correctly hash to their respective P2WSH script_pubkeys
        let mut witness_scripts = Vec::new();
        for idx in 0..3u32 {
            let spk = derive_script_pubkey(&descriptor, idx);

            let outpoint = OutPoint {
                txid: Txid::all_zeros(),
                vout: 0,
            };
            let utxo = InheritanceUtxo::new(outpoint, Amount::from_sat(50_000), 800_000, spk);

            let builder = CheckinTxBuilder::new(utxo, descriptor.clone(), 5, idx);
            let psbt = builder
                .build_psbt()
                .unwrap_or_else(|e| panic!("PSBT at index {} failed: {:?}", idx, e));

            let ws = psbt.inputs[0]
                .witness_script
                .as_ref()
                .expect("witness_script must be set");
            let wu = psbt.inputs[0]
                .witness_utxo
                .as_ref()
                .expect("witness_utxo must be set");

            // Verify witness_script hashes to script_pubkey
            let expected_wsh = ScriptBuf::new_p2wsh(&bitcoin::WScriptHash::hash(ws.as_bytes()));
            assert_eq!(
                wu.script_pubkey, expected_wsh,
                "witness_script/script_pubkey mismatch at index {}",
                idx
            );

            witness_scripts.push(ws.clone());
        }

        // Different derivation indices must yield different witness scripts
        // (because each child key is different)
        assert_ne!(
            witness_scripts[0], witness_scripts[1],
            "index 0 and 1 must differ"
        );
        assert_ne!(
            witness_scripts[1], witness_scripts[2],
            "index 1 and 2 must differ"
        );
    }

    #[test]
    fn test_psbt_bip32_derivation_with_distinct_keys() {
        use crate::policy::{InheritancePolicy, Timelock};
        use bitcoin::bip32::Xpub;
        use miniscript::descriptor::DescriptorPublicKey;
        use std::str::FromStr;

        // Use two DIFFERENT xpubs (derived from same root at different paths, but distinct keys)
        // These are the BIP-32 test vector xpubs
        let owner_xpub = Xpub::from_str(
            "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8"
        ).unwrap();
        // Second xpub: derive child from first to get a genuinely different key
        let secp = bitcoin::secp256k1::Secp256k1::verification_only();
        let heir_xpub = owner_xpub
            .derive_pub(&secp, &[bitcoin::bip32::ChildNumber::Normal { index: 1 }])
            .unwrap();

        let owner_key =
            DescriptorPublicKey::from_str(&format!("[00000001/84'/0'/0']{}/<0;1>/*", owner_xpub))
                .unwrap();
        let heir_key =
            DescriptorPublicKey::from_str(&format!("[00000002/84'/0'/1']{}/<0;1>/*", heir_xpub))
                .unwrap();

        let policy =
            InheritancePolicy::simple(owner_key, heir_key, Timelock::six_months()).unwrap();
        let descriptor = policy.to_wsh_descriptor().unwrap();
        let spk = derive_script_pubkey(&descriptor, 0);

        let outpoint = OutPoint {
            txid: Txid::all_zeros(),
            vout: 0,
        };
        let utxo = InheritanceUtxo::new(outpoint, Amount::from_sat(100_000), 800_000, spk);
        let builder = CheckinTxBuilder::new(utxo, descriptor, 10, 0);
        let psbt = builder.build_psbt().expect("PSBT creation should succeed");

        let bip32 = &psbt.inputs[0].bip32_derivation;

        // With distinct xpubs, we should have 2 entries (owner + heir)
        assert_eq!(
            bip32.len(),
            2,
            "distinct xpubs should produce 2 bip32_derivation entries, got {}",
            bip32.len()
        );

        // Collect fingerprints
        let fingerprints: Vec<_> = bip32.values().map(|(fp, _)| fp.to_bytes()).collect();
        assert!(
            fingerprints.contains(&[0, 0, 0, 1]),
            "owner fingerprint 00000001 missing"
        );
        assert!(
            fingerprints.contains(&[0, 0, 0, 2]),
            "heir fingerprint 00000002 missing"
        );

        // Verify paths end with derivation index 0
        for (_pubkey, (_fp, path)) in bip32 {
            let steps: Vec<_> = path.into_iter().collect();
            let last = steps.last().expect("path must not be empty");
            assert_eq!(
                *last,
                &bitcoin::bip32::ChildNumber::Normal { index: 0 },
                "derivation path must end with child index 0"
            );
        }
    }
}
