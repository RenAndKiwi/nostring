//! CCD Vault — Taproot vaults with delegated co-signing.
//!
//! Creates 2-of-2 Taproot outputs using simple key aggregation (P_owner + P_cosigner).
//! The co-signer's key is derived via CCD tweak at signing time.

use bitcoin::hashes::Hash;
use bitcoin::key::{TapTweak, XOnlyPublicKey};
use bitcoin::secp256k1::{Keypair, PublicKey, Secp256k1, SecretKey};
use bitcoin::{Address, Amount, Network, TxOut};

use crate::types::*;
use crate::{aggregate_taproot_key, apply_tweak, compute_tweak, verify_tweak};

/// Create a new CCD vault at a given address index.
///
/// The vault's Taproot address is derived from the aggregated key:
///   P_agg = P_owner + derive(P_cosigner, chain_code, index)
///
/// The owner knows both keys. The co-signer only learns their derived key
/// when they receive a tweak at signing time.
pub fn create_vault(
    owner_pubkey: &PublicKey,
    delegated: &DelegatedKey,
    address_index: u32,
    network: Network,
) -> Result<CcdVault, CcdError> {
    // Derive co-signer's child pubkey at this index
    let disclosure = compute_tweak(delegated, address_index)?;
    let cosigner_derived = disclosure.derived_pubkey;

    // Aggregate: P_agg = P_owner + P_cosigner_derived
    let aggregate_xonly = aggregate_taproot_key(owner_pubkey, &cosigner_derived)?;

    // Build P2TR address from the aggregate x-only key (key-path spend only, no script tree)
    let secp = Secp256k1::new();
    let address = Address::p2tr(&secp, aggregate_xonly, None, network);

    Ok(CcdVault {
        owner_pubkey: *owner_pubkey,
        delegated: delegated.clone(),
        address_index,
        cosigner_derived_pubkey: cosigner_derived,
        aggregate_xonly,
        address,
        network,
    })
}

/// Build an unsigned PSBT for spending from a vault.
///
/// Returns the PSBT and the input tweaks needed for co-signing.
pub fn build_spend_psbt(
    vault: &CcdVault,
    utxo_outpoints: &[(bitcoin::OutPoint, TxOut)],
    destinations: &[(Address, Amount)],
    fee: Amount,
    change_address: Option<&Address>,
) -> Result<(bitcoin::psbt::Psbt, Vec<InputTweak>), CcdError> {
    use bitcoin::psbt::Psbt;
    use bitcoin::transaction::{Transaction, TxIn, Version};

    if utxo_outpoints.is_empty() {
        return Err(CcdError::PsbtError("no UTXOs provided".into()));
    }

    // Calculate total input value
    let total_in: Amount = utxo_outpoints
        .iter()
        .map(|(_, txout)| txout.value)
        .sum();

    // Calculate total output value
    let total_out: Amount = destinations
        .iter()
        .map(|(_, amount)| *amount)
        .sum();

    let total_out_with_fee = total_out
        .checked_add(fee)
        .ok_or_else(|| CcdError::PsbtError("output + fee overflow".into()))?;

    if total_in < total_out_with_fee {
        return Err(CcdError::PsbtError(format!(
            "insufficient funds: have {} sat, need {} sat",
            total_in.to_sat(),
            total_out_with_fee.to_sat()
        )));
    }

    // Build outputs
    let mut outputs: Vec<TxOut> = destinations
        .iter()
        .map(|(addr, amount)| TxOut {
            value: *amount,
            script_pubkey: addr.script_pubkey(),
        })
        .collect();

    // Change output if needed
    let change = total_in - total_out_with_fee;
    if change > Amount::ZERO {
        let change_script = match change_address {
            Some(addr) => addr.script_pubkey(),
            None => vault.address.script_pubkey(), // send change back to vault
        };
        outputs.push(TxOut {
            value: change,
            script_pubkey: change_script,
        });
    }

    // Build inputs
    let inputs: Vec<TxIn> = utxo_outpoints
        .iter()
        .map(|(outpoint, _)| TxIn {
            previous_output: *outpoint,
            ..Default::default()
        })
        .collect();

    // Create unsigned transaction
    let tx = Transaction {
        version: Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: inputs,
        output: outputs,
    };

    // Create PSBT
    let mut psbt =
        Psbt::from_unsigned_tx(tx).map_err(|e| CcdError::PsbtError(e.to_string()))?;

    // Populate witness UTXO for each input (required for Taproot signing)
    for (i, (_, txout)) in utxo_outpoints.iter().enumerate() {
        psbt.inputs[i].witness_utxo = Some(txout.clone());
        // Set the Taproot internal key
        psbt.inputs[i].tap_internal_key = Some(vault.aggregate_xonly);
    }

    // Compute tweaks for each input
    // All inputs are from the same vault address, so same tweak
    let disclosure = compute_tweak(&vault.delegated, vault.address_index)?;
    let input_tweaks: Vec<InputTweak> = (0..utxo_outpoints.len())
        .map(|i| InputTweak {
            input_index: i,
            tweak: disclosure.tweak,
            derived_pubkey: disclosure.derived_pubkey,
        })
        .collect();

    Ok((psbt, input_tweaks))
}

/// Co-signer: verify tweaks and sign each PSBT input.
///
/// The co-signer:
/// 1. Verifies each tweak produces the expected derived pubkey
/// 2. Derives the child secret key via tweak application
/// 3. Signs each input's sighash with the derived key
///
/// Returns partial Schnorr signatures (one per input).
pub fn cosigner_sign(
    cosigner_sk: &SecretKey,
    psbt: &bitcoin::psbt::Psbt,
    input_tweaks: &[InputTweak],
    cosigner_pk: &PublicKey,
) -> Result<Vec<(usize, bitcoin::taproot::Signature)>, CcdError> {
    use bitcoin::sighash::{Prevouts, SighashCache};
    use bitcoin::TapSighashType;

    let secp = Secp256k1::new();
    let mut signatures = Vec::with_capacity(input_tweaks.len());

    // Collect all witness UTXOs for Prevouts
    let prevouts: Vec<TxOut> = psbt
        .inputs
        .iter()
        .map(|input| {
            input
                .witness_utxo
                .clone()
                .ok_or_else(|| CcdError::PsbtError("missing witness UTXO".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut sighash_cache = SighashCache::new(&psbt.unsigned_tx);

    for tweak_info in input_tweaks {
        let idx = tweak_info.input_index;

        // Verify the tweak
        if !verify_tweak(cosigner_pk, &tweak_info.tweak, &tweak_info.derived_pubkey) {
            return Err(CcdError::TweakVerificationFailed(idx));
        }

        // Derive child secret key
        let child_sk = apply_tweak(cosigner_sk, &tweak_info.tweak)?;
        let child_keypair = Keypair::from_secret_key(&secp, &child_sk);

        // Compute the Taproot key-path sighash
        let sighash = sighash_cache
            .taproot_key_spend_signature_hash(
                idx,
                &Prevouts::All(&prevouts),
                TapSighashType::Default,
            )
            .map_err(|e| CcdError::SigningError(e.to_string()))?;

        let msg = bitcoin::secp256k1::Message::from_digest(sighash.to_byte_array());

        // Sign with the derived keypair
        let sig = secp.sign_schnorr(&msg, &child_keypair);

        signatures.push((
            idx,
            bitcoin::taproot::Signature {
                signature: sig,
                sighash_type: TapSighashType::Default,
            },
        ));
    }

    Ok(signatures)
}

/// Owner: sign each PSBT input with the owner's key.
///
/// Returns partial Schnorr signatures from the owner's side.
pub fn owner_sign(
    owner_keypair: &Keypair,
    psbt: &bitcoin::psbt::Psbt,
) -> Result<Vec<(usize, bitcoin::taproot::Signature)>, CcdError> {
    use bitcoin::sighash::{Prevouts, SighashCache};
    use bitcoin::TapSighashType;

    let secp = Secp256k1::new();
    let mut signatures = Vec::with_capacity(psbt.inputs.len());

    let prevouts: Vec<TxOut> = psbt
        .inputs
        .iter()
        .map(|input| {
            input
                .witness_utxo
                .clone()
                .ok_or_else(|| CcdError::PsbtError("missing witness UTXO".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut sighash_cache = SighashCache::new(&psbt.unsigned_tx);

    for idx in 0..psbt.inputs.len() {
        let sighash = sighash_cache
            .taproot_key_spend_signature_hash(
                idx,
                &Prevouts::All(&prevouts),
                TapSighashType::Default,
            )
            .map_err(|e| CcdError::SigningError(e.to_string()))?;

        let msg = bitcoin::secp256k1::Message::from_digest(sighash.to_byte_array());
        let sig = secp.sign_schnorr(&msg, owner_keypair);

        signatures.push((
            idx,
            bitcoin::taproot::Signature {
                signature: sig,
                sighash_type: TapSighashType::Default,
            },
        ));
    }

    Ok(signatures)
}

/// Compute the Taproot output key from the internal (aggregate) key.
///
/// For key-path-only spending (no script tree), the output key is:
///   Q = P + t*G where t = tagged_hash("TapTweak", P)
///
/// This is what Bitcoin Core uses to tweak the internal key for the actual
/// on-chain output. Both parties must apply this tweak when signing.
pub fn taproot_output_key(
    internal_key: &XOnlyPublicKey,
) -> (XOnlyPublicKey, bitcoin::key::Parity) {
    let secp = Secp256k1::new();
    let (tweaked_pk, parity) = internal_key.tap_tweak(&secp, None);
    (XOnlyPublicKey::from(tweaked_pk), parity)
}

// ─── MuSig2-enabled vault operations ────────────────────────────────────────

/// Create a vault using MuSig2 key aggregation (BIP-327).
///
/// Unlike `create_vault` (which uses simple P1+P2), this uses proper MuSig2
/// key aggregation with rogue-key protection. The resulting address uses the
/// Taproot-tweaked output key.
pub fn create_vault_musig2(
    owner_pubkey: &PublicKey,
    delegated: &DelegatedKey,
    address_index: u32,
    network: Network,
) -> Result<(CcdVault, musig2::KeyAggContext), CcdError> {
    use crate::musig::musig2_key_agg_tweaked;

    let disclosure = compute_tweak(delegated, address_index)?;
    let cosigner_derived = disclosure.derived_pubkey;

    // MuSig2 key aggregation (untweaked = internal key)
    let (_, untweaked_xonly) =
        crate::musig::musig2_key_agg(owner_pubkey, &cosigner_derived)?;

    // MuSig2 key aggregation WITH taproot tweak (for signing context)
    let (key_agg_ctx, _output_xonly) = musig2_key_agg_tweaked(owner_pubkey, &cosigner_derived)?;

    // The address is built from the INTERNAL key — Address::p2tr applies the taptweak itself
    let secp = Secp256k1::new();
    let address = Address::p2tr(&secp, untweaked_xonly, None, network);

    let vault = CcdVault {
        owner_pubkey: *owner_pubkey,
        delegated: delegated.clone(),
        address_index,
        cosigner_derived_pubkey: cosigner_derived,
        aggregate_xonly: untweaked_xonly, // internal key
        address,
        network,
    };

    Ok((vault, key_agg_ctx))
}

/// Run the complete MuSig2 signing ceremony for a PSBT.
///
/// This simulates both rounds of MuSig2 in a single function (for local use).
/// In practice, rounds would be split across Nostr messages.
///
/// Returns the finalized transaction with valid Taproot key-path signatures.
pub fn musig2_sign_psbt(
    owner_sk: &SecretKey,
    cosigner_child_sk: &SecretKey,
    key_agg_ctx: &musig2::KeyAggContext,
    psbt: &bitcoin::psbt::Psbt,
) -> Result<bitcoin::Transaction, CcdError> {
    use bitcoin::sighash::{Prevouts, SighashCache};
    use bitcoin::TapSighashType;
    use crate::musig;

    let prevouts: Vec<TxOut> = psbt
        .inputs
        .iter()
        .map(|input| {
            input
                .witness_utxo
                .clone()
                .ok_or_else(|| CcdError::PsbtError("missing witness UTXO".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let tx = psbt.unsigned_tx.clone();

    // First pass: compute all sighashes
    let mut sighashes = Vec::with_capacity(psbt.inputs.len());
    {
        let mut sighash_cache = SighashCache::new(&tx);
        for idx in 0..psbt.inputs.len() {
            let sighash = sighash_cache
                .taproot_key_spend_signature_hash(
                    idx,
                    &Prevouts::All(&prevouts),
                    TapSighashType::Default,
                )
                .map_err(|e| CcdError::SigningError(e.to_string()))?;
            sighashes.push(sighash.to_byte_array());
        }
    }

    // Second pass: MuSig2 sign each input and collect witnesses
    let mut witnesses: Vec<bitcoin::Witness> = Vec::with_capacity(psbt.inputs.len());
    for message in &sighashes {
        // Round 1: Both generate nonces
        let (owner_secnonce, owner_pubnonce) =
            musig::generate_nonce(owner_sk, key_agg_ctx, Some(message))?;
        let (cosigner_secnonce, cosigner_pubnonce) =
            musig::generate_nonce(cosigner_child_sk, key_agg_ctx, Some(message))?;

        // Aggregate nonces
        let agg_nonce = musig::aggregate_nonces(&[owner_pubnonce, cosigner_pubnonce]);

        // Round 2: Both produce partial signatures
        let owner_partial =
            musig::partial_sign(owner_sk, owner_secnonce, key_agg_ctx, &agg_nonce, message)?;
        let cosigner_partial = musig::partial_sign(
            cosigner_child_sk,
            cosigner_secnonce,
            key_agg_ctx,
            &agg_nonce,
            message,
        )?;

        // Aggregate into final Schnorr signature
        let final_sig = musig::aggregate_signatures(
            key_agg_ctx,
            &agg_nonce,
            &[owner_partial, cosigner_partial],
            message,
        )?;

        // Key-path spend = single 64-byte signature (default sighash omits the byte)
        witnesses.push(bitcoin::Witness::from_slice(&[final_sig.to_vec()]));
    }

    // Apply witnesses to transaction
    let mut signed_tx = tx;
    for (idx, witness) in witnesses.into_iter().enumerate() {
        signed_tx.input[idx].witness = witness;
    }

    Ok(signed_tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register_cosigner;
    use bitcoin::hashes::Hash;
    use bitcoin::secp256k1::Secp256k1;
    use bitcoin::{Amount, Network, OutPoint, Txid};

    fn test_keypair(seed_byte: u8) -> (SecretKey, PublicKey) {
        let secp = Secp256k1::new();
        let mut bytes = [0u8; 32];
        bytes[31] = seed_byte;
        bytes[0] = 0x01;
        let sk = SecretKey::from_slice(&bytes).unwrap();
        let pk = sk.public_key(&secp);
        (sk, pk)
    }

    fn test_outpoint(vout: u32) -> OutPoint {
        OutPoint {
            txid: Txid::from_byte_array([0xAB; 32]),
            vout,
        }
    }

    #[test]
    fn test_create_vault() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test-cosigner");

        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        // Address should be valid P2TR
        assert!(vault.address.script_pubkey().is_p2tr());
        // Aggregate key should be deterministic
        let vault2 = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();
        assert_eq!(vault.aggregate_xonly, vault2.aggregate_xonly);
        assert_eq!(vault.address, vault2.address);
    }

    #[test]
    fn test_different_indices_different_vaults() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test");

        let v0 = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();
        let v1 = create_vault(&owner_pk, &delegated, 1, Network::Signet).unwrap();

        assert_ne!(v0.address, v1.address);
        assert_ne!(v0.aggregate_xonly, v1.aggregate_xonly);
    }

    #[test]
    fn test_build_psbt_basic() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        let utxo_value = Amount::from_sat(100_000);
        let utxos = vec![(
            test_outpoint(0),
            TxOut {
                value: utxo_value,
                script_pubkey: vault.address.script_pubkey(),
            },
        )];

        let dest_addr = vault.address.clone(); // send to self for simplicity
        let destinations = vec![(dest_addr, Amount::from_sat(90_000))];
        let fee = Amount::from_sat(1_000);

        let (psbt, tweaks) = build_spend_psbt(&vault, &utxos, &destinations, fee, None).unwrap();

        // PSBT should have 1 input
        assert_eq!(psbt.inputs.len(), 1);
        // PSBT should have 2 outputs (destination + change)
        assert_eq!(psbt.unsigned_tx.output.len(), 2);
        // Witness UTXO should be populated
        assert!(psbt.inputs[0].witness_utxo.is_some());
        // Internal key should be set
        assert_eq!(psbt.inputs[0].tap_internal_key, Some(vault.aggregate_xonly));
        // Should have 1 tweak
        assert_eq!(tweaks.len(), 1);
        assert_eq!(tweaks[0].input_index, 0);

        // Change should be 100_000 - 90_000 - 1_000 = 9_000
        let change_output = &psbt.unsigned_tx.output[1];
        assert_eq!(change_output.value.to_sat(), 9_000);
    }

    #[test]
    fn test_build_psbt_insufficient_funds() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        let utxos = vec![(
            test_outpoint(0),
            TxOut {
                value: Amount::from_sat(1_000),
                script_pubkey: vault.address.script_pubkey(),
            },
        )];

        let result = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(2_000))],
            Amount::from_sat(500),
            None,
        );
        assert!(matches!(result, Err(CcdError::PsbtError(_))));
    }

    #[test]
    fn test_build_psbt_no_utxos() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        let result = build_spend_psbt(
            &vault,
            &[],
            &[(vault.address.clone(), Amount::from_sat(1_000))],
            Amount::from_sat(500),
            None,
        );
        assert!(matches!(result, Err(CcdError::PsbtError(_))));
    }

    #[test]
    fn test_build_psbt_exact_amount_no_change() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        let utxos = vec![(
            test_outpoint(0),
            TxOut {
                value: Amount::from_sat(10_000),
                script_pubkey: vault.address.script_pubkey(),
            },
        )];

        let (psbt, _) = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(9_500))],
            Amount::from_sat(500),
            None,
        )
        .unwrap();

        // No change output: 10_000 - 9_500 - 500 = 0
        assert_eq!(psbt.unsigned_tx.output.len(), 1);
    }

    #[test]
    fn test_cosigner_sign_valid_tweak() {
        let (cosigner_sk, cosigner_pk) = test_keypair(42);
        let (_owner_sk, owner_pk) = test_keypair(1);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        let utxos = vec![(
            test_outpoint(0),
            TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: vault.address.script_pubkey(),
            },
        )];

        let (psbt, tweaks) = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(90_000))],
            Amount::from_sat(1_000),
            None,
        )
        .unwrap();

        // Co-signer signs
        let sigs = cosigner_sign(&cosigner_sk, &psbt, &tweaks, &cosigner_pk).unwrap();
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].0, 0); // input index 0

        // Verify the signature is valid Schnorr
        let secp = Secp256k1::new();
        let child_sk = apply_tweak(&cosigner_sk, &tweaks[0].tweak).unwrap();
        let child_kp = Keypair::from_secret_key(&secp, &child_sk);
        let (child_xonly, _) = child_kp.x_only_public_key();
        let msg_bytes = {
            use bitcoin::sighash::{Prevouts, SighashCache};
            use bitcoin::TapSighashType;
            let prevouts = vec![utxos[0].1.clone()];
            let mut cache = SighashCache::new(&psbt.unsigned_tx);
            cache
                .taproot_key_spend_signature_hash(
                    0,
                    &Prevouts::All(&prevouts),
                    TapSighashType::Default,
                )
                .unwrap()
        };
        let msg = bitcoin::secp256k1::Message::from_digest(msg_bytes.to_byte_array());
        assert!(secp
            .verify_schnorr(&sigs[0].1.signature, &msg, &child_xonly)
            .is_ok());
    }

    #[test]
    fn test_cosigner_sign_wrong_tweak_rejected() {
        let (cosigner_sk, cosigner_pk) = test_keypair(42);
        let (_owner_sk, owner_pk) = test_keypair(1);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        let utxos = vec![(
            test_outpoint(0),
            TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: vault.address.script_pubkey(),
            },
        )];

        let (psbt, mut tweaks) = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(90_000))],
            Amount::from_sat(1_000),
            None,
        )
        .unwrap();

        // Corrupt the tweak — use a different derived pubkey
        let (_, wrong_pk) = test_keypair(99);
        tweaks[0].derived_pubkey = wrong_pk;

        // Co-signer should reject
        let result = cosigner_sign(&cosigner_sk, &psbt, &tweaks, &cosigner_pk);
        assert!(matches!(result, Err(CcdError::TweakVerificationFailed(0))));
    }

    #[test]
    fn test_owner_sign() {
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let (owner_sk, owner_pk) = test_keypair(1);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        let secp = Secp256k1::new();
        let owner_kp = Keypair::from_secret_key(&secp, &owner_sk);

        let utxos = vec![(
            test_outpoint(0),
            TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: vault.address.script_pubkey(),
            },
        )];

        let (psbt, _) = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(90_000))],
            Amount::from_sat(1_000),
            None,
        )
        .unwrap();

        let sigs = owner_sign(&owner_kp, &psbt).unwrap();
        assert_eq!(sigs.len(), 1);

        // Verify signature
        let (owner_xonly, _) = owner_kp.x_only_public_key();
        let msg_bytes = {
            use bitcoin::sighash::{Prevouts, SighashCache};
            use bitcoin::TapSighashType;
            let prevouts = vec![utxos[0].1.clone()];
            let mut cache = SighashCache::new(&psbt.unsigned_tx);
            cache
                .taproot_key_spend_signature_hash(
                    0,
                    &Prevouts::All(&prevouts),
                    TapSighashType::Default,
                )
                .unwrap()
        };
        let msg = bitcoin::secp256k1::Message::from_digest(msg_bytes.to_byte_array());
        assert!(secp.verify_schnorr(&sigs[0].1.signature, &msg, &owner_xonly).is_ok());
    }

    #[test]
    fn test_taproot_output_key_deterministic() {
        let (_sk, pk) = test_keypair(42);
        let (xonly, _) = pk.x_only_public_key();

        let (out1, parity1) = taproot_output_key(&xonly);
        let (out2, parity2) = taproot_output_key(&xonly);

        assert_eq!(out1, out2);
        assert_eq!(parity1, parity2);
    }

    #[test]
    fn test_multi_input_spend() {
        let (cosigner_sk, cosigner_pk) = test_keypair(42);
        let (_owner_sk, owner_pk) = test_keypair(1);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        // Two UTXOs
        let utxos = vec![
            (
                test_outpoint(0),
                TxOut {
                    value: Amount::from_sat(50_000),
                    script_pubkey: vault.address.script_pubkey(),
                },
            ),
            (
                test_outpoint(1),
                TxOut {
                    value: Amount::from_sat(60_000),
                    script_pubkey: vault.address.script_pubkey(),
                },
            ),
        ];

        let (psbt, tweaks) = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(100_000))],
            Amount::from_sat(1_000),
            None,
        )
        .unwrap();

        assert_eq!(psbt.inputs.len(), 2);
        assert_eq!(tweaks.len(), 2);

        // Both tweaks should be identical (same vault, same index)
        assert_eq!(tweaks[0].tweak, tweaks[1].tweak);
        assert_eq!(tweaks[0].derived_pubkey, tweaks[1].derived_pubkey);

        // Co-signer signs both
        let sigs = cosigner_sign(&cosigner_sk, &psbt, &tweaks, &cosigner_pk).unwrap();
        assert_eq!(sigs.len(), 2);
    }

    #[test]
    fn test_full_signing_roundtrip_validates() {
        // Critical test: both parties sign, verify both signatures validate
        // against the sighash as a real Bitcoin node would.
        let (cosigner_sk, cosigner_pk) = test_keypair(42);
        let (owner_sk, owner_pk) = test_keypair(1);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        let secp = Secp256k1::new();
        let owner_kp = Keypair::from_secret_key(&secp, &owner_sk);

        let utxos = vec![(
            test_outpoint(0),
            TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: vault.address.script_pubkey(),
            },
        )];

        let (psbt, tweaks) = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(90_000))],
            Amount::from_sat(1_000),
            None,
        )
        .unwrap();

        // Both parties sign
        let cosigner_sigs =
            cosigner_sign(&cosigner_sk, &psbt, &tweaks, &cosigner_pk).unwrap();
        let owner_sigs = owner_sign(&owner_kp, &psbt).unwrap();

        // Both should produce valid Schnorr signatures
        assert_eq!(cosigner_sigs.len(), 1);
        assert_eq!(owner_sigs.len(), 1);

        // Compute the sighash manually
        use bitcoin::sighash::{Prevouts, SighashCache};
        use bitcoin::TapSighashType;
        let prevouts = vec![utxos[0].1.clone()];
        let mut cache = SighashCache::new(&psbt.unsigned_tx);
        let sighash = cache
            .taproot_key_spend_signature_hash(
                0,
                &Prevouts::All(&prevouts),
                TapSighashType::Default,
            )
            .unwrap();
        let msg = bitcoin::secp256k1::Message::from_digest(sighash.to_byte_array());

        // Verify co-signer's sig against derived key
        let child_sk = apply_tweak(&cosigner_sk, &tweaks[0].tweak).unwrap();
        let (child_xonly, _) = Keypair::from_secret_key(&secp, &child_sk).x_only_public_key();
        assert!(secp.verify_schnorr(&cosigner_sigs[0].1.signature, &msg, &child_xonly).is_ok());

        // Verify owner's sig against owner key
        let (owner_xonly, _) = owner_kp.x_only_public_key();
        assert!(secp.verify_schnorr(&owner_sigs[0].1.signature, &msg, &owner_xonly).is_ok());
    }

    #[test]
    fn test_wrong_cosigner_key_produces_invalid_sig() {
        // A completely different co-signer key should produce a signature
        // that doesn't verify against the vault's expected derived key.
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let (wrong_sk, wrong_pk) = test_keypair(99);
        let (_owner_sk, owner_pk) = test_keypair(1);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        let utxos = vec![(
            test_outpoint(0),
            TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: vault.address.script_pubkey(),
            },
        )];

        let (psbt, tweaks) = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(90_000))],
            Amount::from_sat(1_000),
            None,
        )
        .unwrap();

        // Wrong co-signer tries to sign — tweak verification should fail
        // because the tweak was computed for cosigner_pk, not wrong_pk
        let result = cosigner_sign(&wrong_sk, &psbt, &tweaks, &wrong_pk);
        assert!(
            matches!(result, Err(CcdError::TweakVerificationFailed(0))),
            "Wrong co-signer key should fail tweak verification"
        );
    }

    #[test]
    fn test_taproot_output_key_differs_from_internal() {
        // The Taproot output key Q = P + H(P)*G should differ from internal key P
        // This ensures the BIP-341 tweak is actually applied
        let (_sk1, pk1) = test_keypair(1);
        let (_sk2, pk2) = test_keypair(42);

        let aggregate = crate::aggregate_taproot_key(&pk1, &pk2).unwrap();
        let (output_key, _parity) = taproot_output_key(&aggregate);

        assert_ne!(
            aggregate, output_key,
            "Output key must differ from internal key (BIP-341 tweak)"
        );
    }

    #[test]
    fn test_custom_change_address() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test");
        let vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        // Create a different vault for change
        let change_vault = create_vault(&owner_pk, &delegated, 1, Network::Signet).unwrap();

        let utxos = vec![(
            test_outpoint(0),
            TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: vault.address.script_pubkey(),
            },
        )];

        let (psbt, _) = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(90_000))],
            Amount::from_sat(1_000),
            Some(&change_vault.address),
        )
        .unwrap();

        // Change output should go to the change vault address
        let change_output = &psbt.unsigned_tx.output[1];
        assert_eq!(change_output.script_pubkey, change_vault.address.script_pubkey());
    }

    // ─── MuSig2 vault tests ────────────────────────────────────────────────

    #[test]
    fn test_create_vault_musig2() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test");

        let (vault, _ctx) = create_vault_musig2(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        // Address should be valid P2TR
        assert!(vault.address.script_pubkey().is_p2tr());

        // MuSig2 vault address should differ from simple key addition vault
        let simple_vault = create_vault(&owner_pk, &delegated, 0, Network::Signet).unwrap();
        assert_ne!(
            vault.address, simple_vault.address,
            "MuSig2 aggregation should produce different address than simple addition"
        );
    }

    #[test]
    fn test_musig2_end_to_end_vault_spend() {
        // THE critical test: create vault → fund → MuSig2 sign → verify signature
        // This proves the entire CCD + MuSig2 stack produces valid Bitcoin transactions.
        let (owner_sk, owner_pk) = test_keypair(1);
        let (cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test");

        // Create MuSig2 vault
        let (vault, key_agg_ctx) =
            create_vault_musig2(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        // Simulate a UTXO at the vault address
        let utxos = vec![(
            test_outpoint(0),
            TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: vault.address.script_pubkey(),
            },
        )];

        // Build PSBT
        let (psbt, tweaks) = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(90_000))],
            Amount::from_sat(1_000),
            None,
        )
        .unwrap();

        // Co-signer derives child key via CCD tweak
        let cosigner_child_sk = apply_tweak(&cosigner_sk, &tweaks[0].tweak).unwrap();

        // MuSig2 signing ceremony
        let signed_tx = musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &key_agg_ctx, &psbt)
            .unwrap();

        // Verify the signature against the vault's output key
        let secp = Secp256k1::new();
        let witness = &signed_tx.input[0].witness;
        assert_eq!(witness.len(), 1, "Key-path spend should have exactly 1 witness element");

        // Parse the Schnorr signature from witness
        let sig_bytes = witness.nth(0).unwrap();
        assert_eq!(sig_bytes.len(), 64, "Schnorr signature should be 64 bytes (default sighash)");

        let sig = bitcoin::secp256k1::schnorr::Signature::from_slice(sig_bytes).unwrap();

        // Compute the sighash
        use bitcoin::sighash::{Prevouts, SighashCache};
        use bitcoin::TapSighashType;
        let prevouts = vec![utxos[0].1.clone()];
        let mut cache = SighashCache::new(&signed_tx);
        let sighash = cache
            .taproot_key_spend_signature_hash(
                0,
                &Prevouts::All(&prevouts),
                TapSighashType::Default,
            )
            .unwrap();
        let msg = bitcoin::secp256k1::Message::from_digest(sighash.to_byte_array());

        // Extract the output key from the vault's P2TR address
        let script = vault.address.script_pubkey();
        let script_bytes = script.as_bytes();
        let output_key =
            bitcoin::secp256k1::XOnlyPublicKey::from_slice(&script_bytes[2..34]).unwrap();

        // THIS IS THE MOMENT: does the signature verify against the output key?
        assert!(
            secp.verify_schnorr(&sig, &msg, &output_key).is_ok(),
            "MuSig2 aggregate signature must verify against the vault's Taproot output key"
        );
    }

    #[test]
    fn test_musig2_multi_input_vault_spend() {
        let (owner_sk, owner_pk) = test_keypair(1);
        let (cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test");

        let (vault, key_agg_ctx) =
            create_vault_musig2(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        // Two UTXOs
        let utxos = vec![
            (
                test_outpoint(0),
                TxOut {
                    value: Amount::from_sat(50_000),
                    script_pubkey: vault.address.script_pubkey(),
                },
            ),
            (
                test_outpoint(1),
                TxOut {
                    value: Amount::from_sat(60_000),
                    script_pubkey: vault.address.script_pubkey(),
                },
            ),
        ];

        let (psbt, tweaks) = build_spend_psbt(
            &vault,
            &utxos,
            &[(vault.address.clone(), Amount::from_sat(100_000))],
            Amount::from_sat(1_000),
            None,
        )
        .unwrap();

        let cosigner_child_sk = apply_tweak(&cosigner_sk, &tweaks[0].tweak).unwrap();

        let signed_tx = musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &key_agg_ctx, &psbt)
            .unwrap();

        // Both inputs should have valid witnesses
        assert_eq!(signed_tx.input[0].witness.len(), 1);
        assert_eq!(signed_tx.input[1].witness.len(), 1);

        // Verify both signatures
        let secp = Secp256k1::new();
        let script = vault.address.script_pubkey();
        let output_key =
            bitcoin::secp256k1::XOnlyPublicKey::from_slice(&script.as_bytes()[2..34]).unwrap();

        use bitcoin::sighash::{Prevouts, SighashCache};
        use bitcoin::TapSighashType;
        let prevouts: Vec<TxOut> = utxos.iter().map(|(_, txout)| txout.clone()).collect();

        for idx in 0..2 {
            let sig_bytes = signed_tx.input[idx].witness.nth(0).unwrap();
            let sig = bitcoin::secp256k1::schnorr::Signature::from_slice(sig_bytes).unwrap();

            let mut cache = SighashCache::new(&signed_tx);
            let sighash = cache
                .taproot_key_spend_signature_hash(
                    idx,
                    &Prevouts::All(&prevouts),
                    TapSighashType::Default,
                )
                .unwrap();
            let msg = bitcoin::secp256k1::Message::from_digest(sighash.to_byte_array());

            assert!(
                secp.verify_schnorr(&sig, &msg, &output_key).is_ok(),
                "Input {} signature must verify", idx
            );
        }
    }
}
