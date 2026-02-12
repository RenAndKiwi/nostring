//! Taproot key-path check-in for inheritable vaults.
//!
//! A check-in spends the vault UTXO via MuSig2 key-path and recreates it
//! at the same address, resetting the CSV timelock. On-chain, this looks
//! like a normal single-sig Taproot transaction — the script tree is never
//! revealed.
//!
//! # Flow
//!
//! 1. Build unsigned PSBT via [`build_taproot_checkin_psbt`]
//! 2. Caller runs CCD MuSig2 signing ceremony (outside this module)
//! 3. Finalize and broadcast
//!
//! The caller may combine a check-in with a payment by adding extra outputs.

use bitcoin::psbt::Psbt;
use bitcoin::transaction::Version;
use bitcoin::{
    absolute::LockTime, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness,
};

use crate::taproot::InheritableVault;
use thiserror::Error;

/// Maximum fee rate we'll accept (sat/vB). Protects against malicious fee data.
const MAX_FEE_RATE: f64 = 500.0;

/// Errors from Taproot check-in operations.
#[derive(Error, Debug)]
pub enum TaprootCheckinError {
    #[error("No UTXOs to check in")]
    NoUtxos,

    #[error("Insufficient funds: need {needed} sat, have {available} sat")]
    InsufficientFunds { needed: Amount, available: Amount },

    #[error("Fee rate {0} sat/vB exceeds maximum ({MAX_FEE_RATE} sat/vB)")]
    FeeRateTooHigh(f64),

    #[error("Check-in amount {0} is below dust limit (546 sat)")]
    DustOutput(Amount),

    #[error("PSBT creation failed: {0}")]
    PsbtError(String),
}

/// Configuration for a Taproot check-in transaction.
pub struct TaprootCheckinConfig {
    /// The inheritable vault to check in.
    pub vault: InheritableVault,
    /// UTXOs to spend. Typically one (the vault UTXO), but can be multiple
    /// for consolidation.
    pub utxos: Vec<(OutPoint, TxOut)>,
    /// Fee rate in sat/vB.
    pub fee_rate: f64,
    /// Optional extra outputs (e.g., combine check-in with a payment).
    pub extra_outputs: Vec<TxOut>,
}

/// Result of building a check-in PSBT.
pub struct TaprootCheckinPsbt {
    /// The unsigned PSBT. Must be signed via CCD MuSig2 ceremony.
    pub psbt: Psbt,
    /// The vault address (same as before — the recreated output).
    pub vault_address: bitcoin::Address,
    /// The check-in amount (value going back to the vault).
    pub checkin_amount: Amount,
    /// The total fee.
    pub fee: Amount,
}

/// Estimate vbytes for a Taproot key-path check-in transaction.
///
/// Key-path spend: 57.5 vbytes per input (1 output key, 64-byte Schnorr sig).
/// P2TR output: 43 vbytes each.
/// Overhead: 10.5 vbytes (version, locktime, marker, flag, counts).
fn estimate_checkin_vbytes(num_inputs: usize, num_outputs: usize) -> f64 {
    let input_vbytes = 57.5 * num_inputs as f64;
    let output_vbytes = 43.0 * num_outputs as f64;
    let overhead = 10.5;
    input_vbytes + output_vbytes + overhead
}

/// Build an unsigned PSBT that spends the vault UTXO(s) via key-path
/// and recreates the vault at the same inheritable address.
///
/// The PSBT must then be signed via the CCD MuSig2 ceremony.
/// This module does NOT handle signing — it just builds the transaction.
///
/// # Key-path spend details
///
/// - Input sequence: `ENABLE_RBF_NO_LOCKTIME` (allows RBF, no relative locktime)
/// - The key-path spend uses the MuSig2 aggregate key tweaked with the Taproot
///   commitment (which includes the script tree). The signer needs the
///   `tap_internal_key` and `tap_merkle_root` from the vault's `TaprootSpendInfo`.
/// - On-chain, this is indistinguishable from a normal single-sig P2TR spend.
pub fn build_taproot_checkin_psbt(
    config: &TaprootCheckinConfig,
) -> Result<TaprootCheckinPsbt, TaprootCheckinError> {
    if config.utxos.is_empty() {
        return Err(TaprootCheckinError::NoUtxos);
    }
    if config.fee_rate > MAX_FEE_RATE {
        return Err(TaprootCheckinError::FeeRateTooHigh(config.fee_rate));
    }
    if config.fee_rate <= 0.0 {
        return Err(TaprootCheckinError::FeeRateTooHigh(config.fee_rate));
    }

    // Total input value
    let total_input: Amount = config.utxos.iter().map(|(_, txout)| txout.value).sum();

    // Total extra output value
    let total_extra: Amount = config.extra_outputs.iter().map(|o| o.value).sum();

    // Estimate fee
    let num_outputs = 1 + config.extra_outputs.len(); // vault recreate + extras
    let vbytes = estimate_checkin_vbytes(config.utxos.len(), num_outputs);
    let fee = Amount::from_sat((vbytes * config.fee_rate).ceil() as u64);

    // Check-in amount = total input - fee - extra outputs
    let checkin_amount = total_input
        .checked_sub(fee)
        .and_then(|v| v.checked_sub(total_extra))
        .ok_or(TaprootCheckinError::InsufficientFunds {
            needed: fee + total_extra,
            available: total_input,
        })?;

    // Reject dust
    if checkin_amount.to_sat() < 546 {
        return Err(TaprootCheckinError::DustOutput(checkin_amount));
    }

    // Build outputs: vault recreate first, then extras
    let mut outputs = vec![TxOut {
        value: checkin_amount,
        script_pubkey: config.vault.address.script_pubkey(),
    }];
    outputs.extend(config.extra_outputs.clone());

    // Build inputs
    let inputs: Vec<TxIn> = config
        .utxos
        .iter()
        .map(|(outpoint, _)| TxIn {
            previous_output: *outpoint,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(),
        })
        .collect();

    let tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: outputs,
    };

    let mut psbt =
        Psbt::from_unsigned_tx(tx).map_err(|e| TaprootCheckinError::PsbtError(e.to_string()))?;

    // Populate PSBT input fields for each UTXO
    for (i, (_, txout)) in config.utxos.iter().enumerate() {
        // witness_utxo: the TxOut being spent (for fee verification by signers)
        psbt.inputs[i].witness_utxo = Some(txout.clone());

        // tap_internal_key: the untweaked MuSig2 aggregate key
        psbt.inputs[i].tap_internal_key =
            Some(config.vault.taproot_spend_info.internal_key());

        // tap_merkle_root: so the signer can compute the correct taptweak
        psbt.inputs[i].tap_merkle_root = config.vault.taproot_spend_info.merkle_root();
    }

    Ok(TaprootCheckinPsbt {
        psbt,
        vault_address: config.vault.address.clone(),
        checkin_amount,
        fee,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{make_default_test_vault, test_keypair};
    use bitcoin::hashes::Hash as _;
    use bitcoin::secp256k1::Secp256k1;
    use bitcoin::Network;

    fn make_utxo(vault: &InheritableVault, value: u64) -> (OutPoint, TxOut) {
        let outpoint = OutPoint {
            txid: bitcoin::Txid::from_byte_array([0xAA; 32]),
            vout: 0,
        };
        let txout = TxOut {
            value: Amount::from_sat(value),
            script_pubkey: vault.address.script_pubkey(),
        };
        (outpoint, txout)
    }

    #[test]
    fn test_basic_checkin() {
        let vault = make_default_test_vault();
        let utxo = make_utxo(&vault, 100_000);

        let config = TaprootCheckinConfig {
            vault: vault.clone(),
            utxos: vec![utxo],
            fee_rate: 2.0,
            extra_outputs: vec![],
        };

        let result = build_taproot_checkin_psbt(&config).unwrap();

        // Output should recreate the vault at the same address
        assert_eq!(
            result.psbt.unsigned_tx.output[0].script_pubkey,
            vault.address.script_pubkey()
        );
        assert_eq!(result.vault_address, vault.address);
        assert!(result.checkin_amount.to_sat() > 0);
        assert!(result.fee.to_sat() > 0);
        assert_eq!(
            result.checkin_amount + result.fee,
            Amount::from_sat(100_000)
        );
    }

    #[test]
    fn test_checkin_with_extra_output() {
        let vault = make_default_test_vault();
        let utxo = make_utxo(&vault, 100_000);

        let secp = Secp256k1::new();
        let (_, payment_pk) = test_keypair(10);
        let payment_addr = bitcoin::Address::p2tr(
            &secp,
            payment_pk.x_only_public_key().0,
            None,
            Network::Testnet,
        );

        let payment_output = TxOut {
            value: Amount::from_sat(20_000),
            script_pubkey: payment_addr.script_pubkey(),
        };

        let config = TaprootCheckinConfig {
            vault: vault.clone(),
            utxos: vec![utxo],
            fee_rate: 2.0,
            extra_outputs: vec![payment_output],
        };

        let result = build_taproot_checkin_psbt(&config).unwrap();

        // Two outputs: vault recreate + payment
        assert_eq!(result.psbt.unsigned_tx.output.len(), 2);
        assert_eq!(
            result.psbt.unsigned_tx.output[0].script_pubkey,
            vault.address.script_pubkey()
        );
        assert_eq!(
            result.psbt.unsigned_tx.output[1].value,
            Amount::from_sat(20_000)
        );
        assert_eq!(
            result.checkin_amount + result.fee + Amount::from_sat(20_000),
            Amount::from_sat(100_000)
        );
    }

    #[test]
    fn test_checkin_multiple_utxos() {
        let vault = make_default_test_vault();
        let utxo1 = make_utxo(&vault, 30_000);
        let mut utxo2 = make_utxo(&vault, 40_000);
        utxo2.0 = OutPoint {
            txid: bitcoin::Txid::from_byte_array([0xBB; 32]),
            vout: 1,
        };

        let config = TaprootCheckinConfig {
            vault: vault.clone(),
            utxos: vec![utxo1, utxo2],
            fee_rate: 1.0,
            extra_outputs: vec![],
        };

        let result = build_taproot_checkin_psbt(&config).unwrap();

        assert_eq!(result.psbt.unsigned_tx.input.len(), 2);
        assert_eq!(
            result.checkin_amount + result.fee,
            Amount::from_sat(70_000)
        );
    }

    #[test]
    fn test_no_utxos_rejected() {
        let vault = make_default_test_vault();

        let config = TaprootCheckinConfig {
            vault,
            utxos: vec![],
            fee_rate: 2.0,
            extra_outputs: vec![],
        };

        assert!(matches!(
            build_taproot_checkin_psbt(&config),
            Err(TaprootCheckinError::NoUtxos)
        ));
    }

    #[test]
    fn test_fee_rate_too_high_rejected() {
        let vault = make_default_test_vault();
        let utxo = make_utxo(&vault, 100_000);

        let config = TaprootCheckinConfig {
            vault,
            utxos: vec![utxo],
            fee_rate: 501.0,
            extra_outputs: vec![],
        };

        assert!(matches!(
            build_taproot_checkin_psbt(&config),
            Err(TaprootCheckinError::FeeRateTooHigh(_))
        ));
    }

    #[test]
    fn test_zero_fee_rate_rejected() {
        let vault = make_default_test_vault();
        let utxo = make_utxo(&vault, 100_000);

        let config = TaprootCheckinConfig {
            vault,
            utxos: vec![utxo],
            fee_rate: 0.0,
            extra_outputs: vec![],
        };

        assert!(matches!(
            build_taproot_checkin_psbt(&config),
            Err(TaprootCheckinError::FeeRateTooHigh(_))
        ));
    }

    #[test]
    fn test_insufficient_funds_rejected() {
        let vault = make_default_test_vault();
        let utxo = make_utxo(&vault, 200); // too small to cover fee

        let config = TaprootCheckinConfig {
            vault,
            utxos: vec![utxo],
            fee_rate: 10.0,
            extra_outputs: vec![],
        };

        assert!(matches!(
            build_taproot_checkin_psbt(&config),
            Err(TaprootCheckinError::InsufficientFunds { .. })
        ));
    }

    #[test]
    fn test_dust_checkin_rejected() {
        let vault = make_default_test_vault();
        // 655 sat at 1 sat/vB: fee ~111, leaving ~544 < 546 dust threshold
        let utxo = make_utxo(&vault, 655);

        let config = TaprootCheckinConfig {
            vault,
            utxos: vec![utxo],
            fee_rate: 1.0,
            extra_outputs: vec![],
        };

        let result = build_taproot_checkin_psbt(&config);
        assert!(result.is_err(), "should reject dust check-in amount");
    }

    #[test]
    fn test_psbt_has_tap_internal_key() {
        let vault = make_default_test_vault();
        let utxo = make_utxo(&vault, 100_000);

        let config = TaprootCheckinConfig {
            vault: vault.clone(),
            utxos: vec![utxo],
            fee_rate: 2.0,
            extra_outputs: vec![],
        };

        let result = build_taproot_checkin_psbt(&config).unwrap();

        // PSBT input should have tap_internal_key set
        assert_eq!(
            result.psbt.inputs[0].tap_internal_key,
            Some(vault.taproot_spend_info.internal_key())
        );
        // And merkle root
        assert_eq!(
            result.psbt.inputs[0].tap_merkle_root,
            vault.taproot_spend_info.merkle_root()
        );
    }

    #[test]
    fn test_psbt_has_witness_utxo() {
        let vault = make_default_test_vault();
        let utxo = make_utxo(&vault, 100_000);

        let config = TaprootCheckinConfig {
            vault: vault.clone(),
            utxos: vec![utxo.clone()],
            fee_rate: 2.0,
            extra_outputs: vec![],
        };

        let result = build_taproot_checkin_psbt(&config).unwrap();

        let witness_utxo = result.psbt.inputs[0].witness_utxo.as_ref().unwrap();
        assert_eq!(witness_utxo.value, utxo.1.value);
        assert_eq!(witness_utxo.script_pubkey, vault.address.script_pubkey());
    }

    #[test]
    fn test_fee_estimation_reasonable() {
        // 1 input, 1 output key-path spend should be ~111 vbytes
        let vbytes = estimate_checkin_vbytes(1, 1);
        assert!(vbytes > 100.0 && vbytes < 130.0, "got {}", vbytes);

        // 2 inputs, 2 outputs
        let vbytes2 = estimate_checkin_vbytes(2, 2);
        assert!(vbytes2 > 200.0 && vbytes2 < 250.0, "got {}", vbytes2);
    }

    #[test]
    fn test_psbt_merkle_root_matches_vault() {
        let vault = make_default_test_vault();
        let utxo = make_utxo(&vault, 100_000);

        let config = TaprootCheckinConfig {
            vault: vault.clone(),
            utxos: vec![utxo],
            fee_rate: 2.0,
            extra_outputs: vec![],
        };

        let result = build_taproot_checkin_psbt(&config).unwrap();

        // Verify merkle root in PSBT matches the vault's taproot tree
        let expected_root = vault.taproot_spend_info.merkle_root();
        assert_eq!(result.psbt.inputs[0].tap_merkle_root, expected_root);

        // Also verify the output recreates the exact same script_pubkey
        // (proves the address derivation is deterministic)
        let output_spk = &result.psbt.unsigned_tx.output[0].script_pubkey;
        assert_eq!(output_spk, &vault.address.script_pubkey());
    }

    #[test]
    fn test_sequence_is_rbf() {
        let vault = make_default_test_vault();
        let utxo = make_utxo(&vault, 100_000);

        let config = TaprootCheckinConfig {
            vault,
            utxos: vec![utxo],
            fee_rate: 2.0,
            extra_outputs: vec![],
        };

        let result = build_taproot_checkin_psbt(&config).unwrap();
        assert_eq!(
            result.psbt.unsigned_tx.input[0].sequence,
            Sequence::ENABLE_RBF_NO_LOCKTIME
        );
    }
}
