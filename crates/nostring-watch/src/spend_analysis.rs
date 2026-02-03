//! Spend type detection via witness analysis
//!
//! For the NoString inheritance descriptor `wsh(or_d(pk(owner), and_v(v:pk(heir), older(N))))`:
//!
//! The compiled script is:
//! ```text
//! <OWNER> CHECKSIG IFDUP NOTIF <HEIR> CHECKSIGVERIFY <N> CSV ENDIF
//! ```
//!
//! **Owner path** (left branch of `or_d`):
//!   - Witness: `[<sig_owner>, <witness_script>]` — 2 items
//!   - CHECKSIG succeeds → IFDUP duplicates → NOTIF skips heir block
//!
//! **Heir path** (right branch of `or_d`):
//!   - Witness: `[<sig_heir>, <empty>, <witness_script>]` — 3 items
//!   - CHECKSIG with empty sig → 0 → IFDUP no-op → NOTIF enters block
//!   - CHECKSIGVERIFY with heir sig → passes → CSV checks timelock
//!
//! For cascade policies with nested `or_d`, additional heir paths add more
//! witness items (one empty dummy per skipped branch). So:
//!   - 2 items → owner
//!   - 3+ items → heir claim
//!
//! This module also supports a timing-based fallback: if the spend occurred
//! before the timelock expired, it MUST be the owner (heir can't spend yet).

use crate::events::SpendType;
use bitcoin::{Transaction, Witness};
use serde::{Deserialize, Serialize};

/// Result of analyzing a spending transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendAnalysis {
    /// The detected spend type
    pub spend_type: SpendType,
    /// How the spend type was determined
    pub method: DetectionMethod,
    /// Number of witness items (excluding witness script)
    pub witness_stack_size: usize,
    /// Confidence level (0.0 - 1.0)
    pub confidence: f64,
}

/// How the spend type was determined
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DetectionMethod {
    /// Analyzed the witness stack structure
    WitnessAnalysis,
    /// Inferred from timelock timing (spend before expiry = must be owner)
    TimelockTiming,
    /// Could not determine
    Indeterminate,
}

/// Analyze a spending transaction's input witness to determine spend type.
///
/// For P2WSH, the witness structure is: `[stack_items..., witness_script]`
///
/// # Arguments
/// * `witness` - The witness data from the spending input
///
/// # Returns
/// A `SpendAnalysis` with the detected type, method, and confidence.
pub fn analyze_witness(witness: &Witness) -> SpendAnalysis {
    let items: Vec<&[u8]> = witness.iter().collect();

    if items.is_empty() {
        return SpendAnalysis {
            spend_type: SpendType::Unknown,
            method: DetectionMethod::Indeterminate,
            witness_stack_size: 0,
            confidence: 0.0,
        };
    }

    // For P2WSH, last item is the witness script
    // Stack items = everything except the last element
    let stack_size = items.len().saturating_sub(1);

    match stack_size {
        // 1 stack item (signature only) → owner path
        1 => {
            // Verify it looks like a DER signature (starts with 0x30, 71-73 bytes typical)
            let sig = items[0];
            let looks_like_sig = sig.len() >= 64 && sig.len() <= 73;

            SpendAnalysis {
                spend_type: SpendType::OwnerCheckin,
                method: DetectionMethod::WitnessAnalysis,
                witness_stack_size: stack_size,
                confidence: if looks_like_sig { 0.95 } else { 0.7 },
            }
        }
        // 2+ stack items → heir path (signature + empty dummy for owner branch)
        n if n >= 2 => {
            // In the heir path, the second-to-last stack item (index n-2 in original items,
            // but considering items[0..stack_size]) should be empty (dummy for owner CHECKSIG)
            //
            // For simple policy: items = [sig_heir, empty, witness_script]
            // items[1] = empty
            let has_empty_dummy = items.iter().take(stack_size).any(|item| item.is_empty());

            if has_empty_dummy {
                SpendAnalysis {
                    spend_type: SpendType::HeirClaim,
                    method: DetectionMethod::WitnessAnalysis,
                    witness_stack_size: stack_size,
                    confidence: 0.9,
                }
            } else {
                // Multiple items but no empty dummy — unusual, could be
                // a different script type or unexpected structure
                SpendAnalysis {
                    spend_type: SpendType::Unknown,
                    method: DetectionMethod::Indeterminate,
                    witness_stack_size: stack_size,
                    confidence: 0.3,
                }
            }
        }
        // 0 stack items (only witness script) — shouldn't happen for valid spend
        _ => SpendAnalysis {
            spend_type: SpendType::Unknown,
            method: DetectionMethod::Indeterminate,
            witness_stack_size: stack_size,
            confidence: 0.0,
        },
    }
}

/// Analyze spend type using timelock timing as a heuristic.
///
/// If the UTXO was spent before the timelock expired, it MUST be the owner
/// (the heir's spending path isn't available yet). If after expiry, it could
/// be either — we can't determine from timing alone.
///
/// # Arguments
/// * `spend_height` - Block height where the spend was confirmed
/// * `utxo_height` - Block height where the UTXO was created
/// * `timelock_blocks` - Number of blocks for the timelock (CSV value)
///
/// # Returns
/// `Some(SpendType::OwnerCheckin)` if definitively owner,
/// `None` if timing alone can't determine (post-expiry).
pub fn analyze_timing(
    spend_height: u32,
    utxo_height: u32,
    timelock_blocks: u32,
) -> Option<SpendType> {
    let blocks_elapsed = spend_height.saturating_sub(utxo_height);

    if blocks_elapsed < timelock_blocks {
        // Spent before timelock expired → must be owner
        Some(SpendType::OwnerCheckin)
    } else {
        // Post-expiry: could be either owner or heir
        None
    }
}

/// Combined analysis: try witness first, fall back to timing.
///
/// # Arguments
/// * `witness` - The witness data from the spending input
/// * `spend_height` - Block height of the spending transaction (0 if unknown)
/// * `utxo_height` - Block height of the original UTXO (0 if unknown)
/// * `timelock_blocks` - Timelock duration in blocks
pub fn analyze_spend(
    witness: &Witness,
    spend_height: u32,
    utxo_height: u32,
    timelock_blocks: u32,
) -> SpendAnalysis {
    // Primary: witness analysis
    let mut analysis = analyze_witness(witness);

    // If witness analysis is inconclusive, try timing
    if analysis.spend_type == SpendType::Unknown && spend_height > 0 && utxo_height > 0 {
        if let Some(timing_type) = analyze_timing(spend_height, utxo_height, timelock_blocks) {
            analysis = SpendAnalysis {
                spend_type: timing_type,
                method: DetectionMethod::TimelockTiming,
                witness_stack_size: analysis.witness_stack_size,
                confidence: 0.99, // Timing before expiry is definitive
            };
        }
    }

    // If witness says owner but timing could tell us more, boost confidence
    if analysis.spend_type == SpendType::OwnerCheckin
        && analysis.method == DetectionMethod::WitnessAnalysis
        && spend_height > 0
        && utxo_height > 0
    {
        if let Some(SpendType::OwnerCheckin) =
            analyze_timing(spend_height, utxo_height, timelock_blocks)
        {
            // Both witness and timing agree → very high confidence
            analysis.confidence = 0.99;
        }
    }

    analysis
}

/// Analyze a full transaction to find which input spent a specific outpoint,
/// and determine the spend type from its witness.
///
/// # Arguments
/// * `tx` - The spending transaction
/// * `spent_txid` - The txid of the UTXO that was spent
/// * `spent_vout` - The vout of the UTXO that was spent
///
/// # Returns
/// The witness analysis if the input was found, None otherwise.
pub fn analyze_transaction_for_outpoint(
    tx: &Transaction,
    spent_txid: &bitcoin::Txid,
    spent_vout: u32,
) -> Option<SpendAnalysis> {
    for input in &tx.input {
        if &input.previous_output.txid == spent_txid
            && input.previous_output.vout == spent_vout
        {
            return Some(analyze_witness(&input.witness));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::Witness;

    /// Build a mock "owner" witness: [signature, witness_script]
    fn mock_owner_witness() -> Witness {
        // A plausible DER-encoded signature (71 bytes) + sighash byte
        let sig = vec![
            0x30, 0x44, 0x02, 0x20, // DER sequence header
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // r value (32 bytes)
            0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
            0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
            0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
            0x02, 0x20, // s value header
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, // s value (32 bytes)
            0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30,
            0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38,
            0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40,
            0x01, // SIGHASH_ALL
        ];

        // Mock witness script (simplified)
        let witness_script = vec![0x21, 0x02, 0xAA, 0xBB, 0xCC]; // OP_PUSH33 <pubkey>...

        let mut witness = Witness::new();
        witness.push(&sig);
        witness.push(&witness_script);
        witness
    }

    /// Build a mock "heir" witness: [signature, empty_dummy, witness_script]
    fn mock_heir_witness() -> Witness {
        let sig = vec![
            0x30, 0x44, 0x02, 0x20,
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
            0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
            0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
            0x02, 0x20,
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28,
            0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30,
            0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38,
            0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40,
            0x01,
        ];

        let empty_dummy: Vec<u8> = vec![]; // Empty for owner CHECKSIG
        let witness_script = vec![0x21, 0x02, 0xAA, 0xBB, 0xCC];

        let mut witness = Witness::new();
        witness.push(&sig);
        witness.push(&empty_dummy);
        witness.push(&witness_script);
        witness
    }

    #[test]
    fn test_owner_witness_detection() {
        let witness = mock_owner_witness();
        let analysis = analyze_witness(&witness);

        assert_eq!(analysis.spend_type, SpendType::OwnerCheckin);
        assert_eq!(analysis.method, DetectionMethod::WitnessAnalysis);
        assert_eq!(analysis.witness_stack_size, 1);
        assert!(analysis.confidence >= 0.9);
    }

    #[test]
    fn test_heir_witness_detection() {
        let witness = mock_heir_witness();
        let analysis = analyze_witness(&witness);

        assert_eq!(analysis.spend_type, SpendType::HeirClaim);
        assert_eq!(analysis.method, DetectionMethod::WitnessAnalysis);
        assert_eq!(analysis.witness_stack_size, 2);
        assert!(analysis.confidence >= 0.85);
    }

    #[test]
    fn test_empty_witness() {
        let witness = Witness::new();
        let analysis = analyze_witness(&witness);

        assert_eq!(analysis.spend_type, SpendType::Unknown);
        assert_eq!(analysis.method, DetectionMethod::Indeterminate);
        assert_eq!(analysis.confidence, 0.0);
    }

    #[test]
    fn test_timing_before_expiry() {
        // UTXO at height 800000, spent at 810000, timelock 26280 blocks
        // 810000 - 800000 = 10000 < 26280 → must be owner
        let result = analyze_timing(810_000, 800_000, 26_280);
        assert_eq!(result, Some(SpendType::OwnerCheckin));
    }

    #[test]
    fn test_timing_after_expiry() {
        // UTXO at height 800000, spent at 830000, timelock 26280 blocks
        // 830000 - 800000 = 30000 > 26280 → could be either
        let result = analyze_timing(830_000, 800_000, 26_280);
        assert_eq!(result, None);
    }

    #[test]
    fn test_timing_exactly_at_expiry() {
        // Exactly at timelock boundary
        let result = analyze_timing(826_280, 800_000, 26_280);
        assert_eq!(result, None); // At boundary, heir could technically spend
    }

    #[test]
    fn test_combined_owner_analysis() {
        let witness = mock_owner_witness();
        let analysis = analyze_spend(&witness, 810_000, 800_000, 26_280);

        assert_eq!(analysis.spend_type, SpendType::OwnerCheckin);
        // Both witness and timing agree → high confidence
        assert!(analysis.confidence >= 0.95);
    }

    #[test]
    fn test_combined_heir_analysis() {
        let witness = mock_heir_witness();
        // Spend after timelock expiry
        let analysis = analyze_spend(&witness, 830_000, 800_000, 26_280);

        assert_eq!(analysis.spend_type, SpendType::HeirClaim);
        assert_eq!(analysis.method, DetectionMethod::WitnessAnalysis);
    }

    #[test]
    fn test_combined_unknown_with_timing_fallback() {
        // Unknown witness but timing says pre-expiry → timing fallback kicks in
        let witness = Witness::new();
        let analysis = analyze_spend(&witness, 810_000, 800_000, 26_280);

        // Empty witness can't be analyzed, but timing says pre-expiry → must be owner
        assert_eq!(analysis.spend_type, SpendType::OwnerCheckin);
        assert_eq!(analysis.method, DetectionMethod::TimelockTiming);
        assert!(analysis.confidence >= 0.95);
    }

    #[test]
    fn test_combined_unknown_no_timing_info() {
        // Unknown witness with no timing info → stays Unknown
        let witness = Witness::new();
        let analysis = analyze_spend(&witness, 0, 0, 26_280);

        assert_eq!(analysis.spend_type, SpendType::Unknown);
        assert_eq!(analysis.method, DetectionMethod::Indeterminate);
    }

    #[test]
    fn test_cascade_heir_witness() {
        // For cascade: heir2 path has [sig_heir2, empty_for_heir1, empty_for_owner, script]
        let sig = vec![0x30, 0x44, 0x02, 0x20,
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
            0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
            0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
            0x02, 0x20,
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28,
            0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30,
            0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38,
            0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40,
            0x01];
        let empty1: Vec<u8> = vec![];
        let empty2: Vec<u8> = vec![];
        let witness_script = vec![0x21, 0x02, 0xAA, 0xBB, 0xCC];

        let mut witness = Witness::new();
        witness.push(&sig);
        witness.push(&empty1);
        witness.push(&empty2);
        witness.push(&witness_script);

        let analysis = analyze_witness(&witness);
        assert_eq!(analysis.spend_type, SpendType::HeirClaim);
        assert_eq!(analysis.witness_stack_size, 3);
    }

    #[test]
    fn test_analyze_transaction_for_outpoint() {
        use bitcoin::absolute::LockTime;
        use bitcoin::hashes::Hash;
        use bitcoin::transaction::Version;
        use bitcoin::{OutPoint, ScriptBuf, Sequence, TxIn, TxOut, Txid};

        let target_txid = Txid::all_zeros();
        let target_vout = 0u32;

        let owner_witness = mock_owner_witness();

        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: target_txid,
                    vout: target_vout,
                },
                script_sig: ScriptBuf::new(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: owner_witness,
            }],
            output: vec![TxOut {
                value: bitcoin::Amount::from_sat(50_000),
                script_pubkey: ScriptBuf::new(),
            }],
        };

        let result = analyze_transaction_for_outpoint(&tx, &target_txid, target_vout);
        assert!(result.is_some());
        let analysis = result.unwrap();
        assert_eq!(analysis.spend_type, SpendType::OwnerCheckin);

        // Non-matching outpoint should return None
        let other_txid = Txid::from_byte_array([0x01; 32]);
        let result = analyze_transaction_for_outpoint(&tx, &other_txid, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_witness_with_short_signature() {
        // Non-standard signature length — still detectable as owner
        let short_sig = vec![0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01, 0x01];
        let witness_script = vec![0x21, 0x02, 0xAA];

        let mut witness = Witness::new();
        witness.push(&short_sig);
        witness.push(&witness_script);

        let analysis = analyze_witness(&witness);
        assert_eq!(analysis.spend_type, SpendType::OwnerCheckin);
        // Lower confidence because signature length is unusual
        assert!(analysis.confidence < 0.95);
    }

    #[test]
    fn test_spend_type_display() {
        // Verify SpendType serialization
        let owner = SpendType::OwnerCheckin;
        let heir = SpendType::HeirClaim;
        let unknown = SpendType::Unknown;

        let owner_json = serde_json::to_string(&owner).unwrap();
        let heir_json = serde_json::to_string(&heir).unwrap();
        let unknown_json = serde_json::to_string(&unknown).unwrap();

        assert_eq!(owner_json, "\"OwnerCheckin\"");
        assert_eq!(heir_json, "\"HeirClaim\"");
        assert_eq!(unknown_json, "\"Unknown\"");
    }
}
