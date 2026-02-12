//! End-to-end integration test for the full inheritable vault lifecycle.
//!
//! Proves that nostring-ccd (MuSig2) and nostring-inherit (Taproot inheritance)
//! compose correctly:
//!
//! 1. Create inheritable vault (key-path MuSig2 + script-path heir timelock)
//! 2. Owner checks in via MuSig2 key-path spend
//! 3. Heir claims via script-path spend after timelock
//! 4. Both transactions pass Bitcoin Core consensus verification

use bitcoin::consensus::Encodable;
use bitcoin::hashes::Hash as _;
use bitcoin::secp256k1::{Keypair, PublicKey, Secp256k1, SecretKey};
use bitcoin::sighash::{Prevouts, SighashCache, TapSighashType};
use bitcoin::taproot::{LeafVersion, Signature as TapSignature, TapLeafHash};
use bitcoin::{Address, Amount, Network, OutPoint, TxOut, Witness};
use miniscript::descriptor::DescriptorPublicKey;
use std::str::FromStr;

use nostring_ccd::compute_tweak;
use nostring_ccd::musig::{aggregate_nonces, aggregate_signatures, generate_nonce, partial_sign};
use nostring_ccd::register_cosigner_with_chain_code;
use nostring_ccd::types::ChainCode;
use nostring_inherit::policy::{PathInfo, Timelock};
use nostring_inherit::taproot::{build_heir_claim_psbt, create_inheritable_vault};
use nostring_inherit::taproot_checkin::{build_taproot_checkin_psbt, TaprootCheckinConfig};

fn test_keypair(seed: u8) -> (SecretKey, PublicKey) {
    let secp = Secp256k1::new();
    let mut bytes = [0u8; 32];
    bytes[31] = seed;
    bytes[0] = 0x01;
    let sk = SecretKey::from_slice(&bytes).unwrap();
    let pk = sk.public_key(&secp);
    (sk, pk)
}

fn test_chain_code() -> ChainCode {
    ChainCode([0xAB; 32])
}

/// Consensus-verify a transaction against its inputs using libbitcoinconsensus.
fn consensus_verify(tx_bytes: &[u8], spent_outputs: &[TxOut], input_index: usize) {
    let txout = &spent_outputs[input_index];

    let all_utxos: Vec<bitcoinconsensus::Utxo> = spent_outputs
        .iter()
        .map(|o| {
            let sb = o.script_pubkey.as_bytes();
            bitcoinconsensus::Utxo {
                script_pubkey: sb.as_ptr(),
                script_pubkey_len: sb.len() as u32,
                value: o.value.to_sat() as i64,
            }
        })
        .collect();

    let result = bitcoinconsensus::verify(
        txout.script_pubkey.as_bytes(),
        txout.value.to_sat(),
        tx_bytes,
        Some(&all_utxos),
        input_index,
    );

    assert!(
        result.is_ok(),
        "consensus verification failed for input {}: {:?}",
        input_index,
        result.err()
    );
}

#[test]
fn test_full_inheritance_lifecycle() {
    let secp = Secp256k1::new();
    let (owner_sk, owner_pk) = test_keypair(1);
    let (cosigner_sk, cosigner_pk) = test_keypair(2);
    let (heir_sk, heir_pk) = test_keypair(3);

    let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

    let heir_xonly = heir_pk.x_only_public_key().0;
    let heir_desc = DescriptorPublicKey::from_str(&format!("{}", heir_xonly)).unwrap();

    // ═══════════════════════════════════════════════════════════════════════
    // STEP 1: Create inheritable vault
    // ═══════════════════════════════════════════════════════════════════════
    let vault = create_inheritable_vault(
        &owner_pk,
        &delegated,
        0,
        PathInfo::Single(heir_desc),
        Timelock::from_blocks(1).unwrap(), // 1 block for testability
        0,
        Network::Testnet,
    )
    .unwrap();

    assert!(vault.address.to_string().starts_with("tb1p"));
    assert!(!vault.recovery_scripts.is_empty());

    // ═══════════════════════════════════════════════════════════════════════
    // STEP 2: Build check-in PSBT
    // ═══════════════════════════════════════════════════════════════════════
    let funding_outpoint = OutPoint {
        txid: bitcoin::Txid::from_byte_array([0xAA; 32]),
        vout: 0,
    };
    let funding_txout = TxOut {
        value: Amount::from_sat(50_000),
        script_pubkey: vault.address.script_pubkey(),
    };

    let checkin_config = TaprootCheckinConfig {
        vault: vault.clone(),
        utxos: vec![(funding_outpoint, funding_txout.clone())],
        fee_rate: 2.0,
        extra_outputs: vec![],
    };

    let checkin_result = build_taproot_checkin_psbt(&checkin_config).unwrap();
    assert_eq!(
        checkin_result.psbt.unsigned_tx.output[0].script_pubkey,
        vault.address.script_pubkey(),
        "check-in output must recreate the same vault address"
    );

    // ═══════════════════════════════════════════════════════════════════════
    // STEP 3: Sign check-in via MuSig2 key-path
    // ═══════════════════════════════════════════════════════════════════════
    // Derive the cosigner's key at address_index 0
    let disclosure = compute_tweak(&delegated, 0).unwrap();
    let cosigner_derived_sk = {
        let tweak_sk = SecretKey::from_slice(&disclosure.tweak.to_be_bytes()).unwrap();
        cosigner_sk.add_tweak(&tweak_sk.into()).unwrap()
    };

    // Reconstruct MuSig2 KeyAggContext from the vault (includes taproot tweak)
    let (tweaked_ctx, tweaked_xonly) = vault.key_agg_context().unwrap();

    // Verify the tweaked key matches the vault's output key (sanity check —
    // key_agg_context() already asserts this, but we verify explicitly)
    let output_key = vault.taproot_spend_info.output_key().to_x_only_public_key();
    assert_eq!(
        tweaked_xonly.serialize(),
        output_key.serialize(),
        "MuSig2 tweaked key must match Taproot output key"
    );

    // Compute sighash for the check-in transaction
    let mut sighash_cache = SighashCache::new(&checkin_result.psbt.unsigned_tx);
    let prevouts = Prevouts::All(&[funding_txout.clone()]);
    let sighash = sighash_cache
        .taproot_key_spend_signature_hash(0, &prevouts, TapSighashType::Default)
        .unwrap();
    let msg_bytes: [u8; 32] = *sighash.as_byte_array();

    // Round 1: Generate nonces
    let (owner_secnonce, owner_pubnonce) =
        generate_nonce(&owner_sk, &tweaked_ctx, Some(&msg_bytes)).unwrap();
    let (cosigner_secnonce, cosigner_pubnonce) =
        generate_nonce(&cosigner_derived_sk, &tweaked_ctx, Some(&msg_bytes)).unwrap();

    // Aggregate nonces
    let agg_nonce = aggregate_nonces(&[owner_pubnonce, cosigner_pubnonce]);

    // Round 2: Partial signatures
    let owner_partial = partial_sign(
        &owner_sk,
        owner_secnonce,
        &tweaked_ctx,
        &agg_nonce,
        &msg_bytes,
    )
    .unwrap();
    let cosigner_partial = partial_sign(
        &cosigner_derived_sk,
        cosigner_secnonce,
        &tweaked_ctx,
        &agg_nonce,
        &msg_bytes,
    )
    .unwrap();

    // Aggregate into final Schnorr signature
    let final_sig_bytes = aggregate_signatures(
        &tweaked_ctx,
        &agg_nonce,
        &[owner_partial, cosigner_partial],
        &msg_bytes,
    )
    .unwrap();

    // Build the signed check-in transaction
    let mut checkin_tx = checkin_result.psbt.unsigned_tx.clone();
    let schnorr_sig = bitcoin::secp256k1::schnorr::Signature::from_slice(&final_sig_bytes).unwrap();
    let tap_sig = TapSignature {
        signature: schnorr_sig,
        sighash_type: TapSighashType::Default,
    };
    checkin_tx.input[0].witness = Witness::new();
    checkin_tx.input[0].witness.push(tap_sig.to_vec());

    // Consensus-verify the check-in transaction
    let mut checkin_bytes = Vec::new();
    checkin_tx.consensus_encode(&mut checkin_bytes).unwrap();
    consensus_verify(&checkin_bytes, &[funding_txout.clone()], 0);

    // ═══════════════════════════════════════════════════════════════════════
    // STEP 4: Build heir claim PSBT (after timelock expiry)
    // ═══════════════════════════════════════════════════════════════════════
    // The check-in output is now the new vault UTXO
    let checkin_outpoint = OutPoint {
        txid: bitcoin::Txid::from_byte_array([0xBB; 32]),
        vout: 0,
    };
    let checkin_txout = TxOut {
        value: checkin_result.checkin_amount,
        script_pubkey: vault.address.script_pubkey(),
    };

    let destination = Address::p2tr(&secp, heir_xonly, None, Network::Testnet);

    let heir_psbt = build_heir_claim_psbt(
        &vault,
        0,
        &[(checkin_outpoint, checkin_txout.clone())],
        &destination,
        Amount::from_sat(300),
    )
    .unwrap();

    // ═══════════════════════════════════════════════════════════════════════
    // STEP 5: Sign heir claim via script-path
    // ═══════════════════════════════════════════════════════════════════════
    let recovery_script = &vault.recovery_scripts[0].1;
    let leaf_hash = TapLeafHash::from_script(recovery_script, LeafVersion::TapScript);

    let mut heir_sighash_cache = SighashCache::new(&heir_psbt.unsigned_tx);
    let heir_prevouts = Prevouts::All(&[checkin_txout.clone()]);
    let heir_sighash = heir_sighash_cache
        .taproot_script_spend_signature_hash(0, &heir_prevouts, leaf_hash, TapSighashType::Default)
        .unwrap();
    let heir_msg = bitcoin::secp256k1::Message::from_digest(*heir_sighash.as_byte_array());
    let heir_keypair = Keypair::from_secret_key(&secp, &heir_sk);
    let heir_schnorr = secp.sign_schnorr(&heir_msg, &heir_keypair);

    let control_block = vault
        .taproot_spend_info
        .control_block(&(recovery_script.clone(), LeafVersion::TapScript))
        .expect("control block must exist");

    let heir_tap_sig = TapSignature {
        signature: heir_schnorr,
        sighash_type: TapSighashType::Default,
    };

    let mut heir_tx = heir_psbt.unsigned_tx.clone();
    heir_tx.input[0].witness = Witness::new();
    heir_tx.input[0].witness.push(heir_tap_sig.to_vec());
    heir_tx.input[0].witness.push(recovery_script.as_bytes());
    heir_tx.input[0].witness.push(control_block.serialize());

    // ═══════════════════════════════════════════════════════════════════════
    // STEP 6: Consensus-verify heir claim
    // ═══════════════════════════════════════════════════════════════════════
    let mut heir_bytes = Vec::new();
    heir_tx.consensus_encode(&mut heir_bytes).unwrap();
    consensus_verify(&heir_bytes, &[checkin_txout], 0);

    // Both transactions pass Bitcoin Core's consensus rules.
    // The full lifecycle works: create vault → check-in (key-path) → heir claim (script-path).
}

#[test]
fn test_multi_heir_lifecycle() {
    let secp = Secp256k1::new();
    let (owner_sk, owner_pk) = test_keypair(1);
    let (cosigner_sk, cosigner_pk) = test_keypair(2);
    let (heir1_sk, heir1_pk) = test_keypair(10);
    let (heir2_sk, heir2_pk) = test_keypair(11);
    let (_heir3_sk, heir3_pk) = test_keypair(12);

    let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

    let h1_xonly = heir1_pk.x_only_public_key().0;
    let h2_xonly = heir2_pk.x_only_public_key().0;
    let h3_xonly = heir3_pk.x_only_public_key().0;
    let h1_desc = DescriptorPublicKey::from_str(&format!("{}", h1_xonly)).unwrap();
    let h2_desc = DescriptorPublicKey::from_str(&format!("{}", h2_xonly)).unwrap();
    let h3_desc = DescriptorPublicKey::from_str(&format!("{}", h3_xonly)).unwrap();

    // 2-of-3 heirs
    let heirs = PathInfo::multi(2, vec![h1_desc, h2_desc, h3_desc]).unwrap();

    let vault = create_inheritable_vault(
        &owner_pk,
        &delegated,
        0,
        heirs,
        Timelock::from_blocks(1).unwrap(),
        0,
        Network::Testnet,
    )
    .unwrap();

    // ── Check-in via MuSig2 (same as single-heir) ──
    let funding_outpoint = OutPoint {
        txid: bitcoin::Txid::from_byte_array([0xCC; 32]),
        vout: 0,
    };
    let funding_txout = TxOut {
        value: Amount::from_sat(50_000),
        script_pubkey: vault.address.script_pubkey(),
    };

    let checkin_config = TaprootCheckinConfig {
        vault: vault.clone(),
        utxos: vec![(funding_outpoint, funding_txout.clone())],
        fee_rate: 2.0,
        extra_outputs: vec![],
    };

    let checkin_result = build_taproot_checkin_psbt(&checkin_config).unwrap();

    let disclosure = compute_tweak(&delegated, 0).unwrap();
    let cosigner_derived_sk = {
        let tweak_sk = SecretKey::from_slice(&disclosure.tweak.to_be_bytes()).unwrap();
        cosigner_sk.add_tweak(&tweak_sk.into()).unwrap()
    };

    let (tweaked_ctx, _) = vault.key_agg_context().unwrap();

    let mut sighash_cache = SighashCache::new(&checkin_result.psbt.unsigned_tx);
    let prevouts = Prevouts::All(&[funding_txout.clone()]);
    let sighash = sighash_cache
        .taproot_key_spend_signature_hash(0, &prevouts, TapSighashType::Default)
        .unwrap();
    let msg_bytes: [u8; 32] = *sighash.as_byte_array();

    let (owner_secnonce, owner_pubnonce) =
        generate_nonce(&owner_sk, &tweaked_ctx, Some(&msg_bytes)).unwrap();
    let (cosigner_secnonce, cosigner_pubnonce) =
        generate_nonce(&cosigner_derived_sk, &tweaked_ctx, Some(&msg_bytes)).unwrap();
    let agg_nonce = aggregate_nonces(&[owner_pubnonce, cosigner_pubnonce]);

    let owner_partial = partial_sign(
        &owner_sk,
        owner_secnonce,
        &tweaked_ctx,
        &agg_nonce,
        &msg_bytes,
    )
    .unwrap();
    let cosigner_partial = partial_sign(
        &cosigner_derived_sk,
        cosigner_secnonce,
        &tweaked_ctx,
        &agg_nonce,
        &msg_bytes,
    )
    .unwrap();

    let final_sig_bytes = aggregate_signatures(
        &tweaked_ctx,
        &agg_nonce,
        &[owner_partial, cosigner_partial],
        &msg_bytes,
    )
    .unwrap();

    let mut checkin_tx = checkin_result.psbt.unsigned_tx.clone();
    let schnorr_sig = bitcoin::secp256k1::schnorr::Signature::from_slice(&final_sig_bytes).unwrap();
    checkin_tx.input[0].witness = Witness::new();
    checkin_tx.input[0].witness.push(
        TapSignature {
            signature: schnorr_sig,
            sighash_type: TapSighashType::Default,
        }
        .to_vec(),
    );

    let mut checkin_bytes = Vec::new();
    checkin_tx.consensus_encode(&mut checkin_bytes).unwrap();
    consensus_verify(&checkin_bytes, &[funding_txout], 0);

    // ── 2-of-3 heir claim via script-path ──
    let checkin_txout = TxOut {
        value: checkin_result.checkin_amount,
        script_pubkey: vault.address.script_pubkey(),
    };
    let checkin_outpoint = OutPoint {
        txid: bitcoin::Txid::from_byte_array([0xDD; 32]),
        vout: 0,
    };

    let destination = Address::p2tr(&secp, h1_xonly, None, Network::Testnet);
    let heir_psbt = build_heir_claim_psbt(
        &vault,
        0,
        &[(checkin_outpoint, checkin_txout.clone())],
        &destination,
        Amount::from_sat(300),
    )
    .unwrap();

    let recovery_script = &vault.recovery_scripts[0].1;
    let leaf_hash = TapLeafHash::from_script(recovery_script, LeafVersion::TapScript);

    let mut heir_sighash_cache = SighashCache::new(&heir_psbt.unsigned_tx);
    let heir_prevouts = Prevouts::All(&[checkin_txout.clone()]);
    let heir_sighash = heir_sighash_cache
        .taproot_script_spend_signature_hash(0, &heir_prevouts, leaf_hash, TapSighashType::Default)
        .unwrap();
    let heir_msg = bitcoin::secp256k1::Message::from_digest(*heir_sighash.as_byte_array());

    let sig1 = secp.sign_schnorr(&heir_msg, &Keypair::from_secret_key(&secp, &heir1_sk));
    let sig2 = secp.sign_schnorr(&heir_msg, &Keypair::from_secret_key(&secp, &heir2_sk));

    let tap_sig1 = TapSignature {
        signature: sig1,
        sighash_type: TapSighashType::Default,
    };
    let tap_sig2 = TapSignature {
        signature: sig2,
        sighash_type: TapSighashType::Default,
    };

    let control_block = vault
        .taproot_spend_info
        .control_block(&(recovery_script.clone(), LeafVersion::TapScript))
        .unwrap();

    let mut heir_tx = heir_psbt.unsigned_tx.clone();
    heir_tx.input[0].witness = Witness::new();
    // multi_a witness: sigs in reverse key order, empty for non-signers
    heir_tx.input[0].witness.push(&[] as &[u8]); // heir3 did not sign
    heir_tx.input[0].witness.push(tap_sig2.to_vec());
    heir_tx.input[0].witness.push(tap_sig1.to_vec());
    heir_tx.input[0].witness.push(recovery_script.as_bytes());
    heir_tx.input[0].witness.push(control_block.serialize());

    let mut heir_bytes = Vec::new();
    heir_tx.consensus_encode(&mut heir_bytes).unwrap();
    consensus_verify(&heir_bytes, &[checkin_txout], 0);

    // Both MuSig2 key-path check-in AND 2-of-3 script-path heir claim
    // pass Bitcoin Core consensus for the same vault.
}

#[test]
fn test_key_agg_context_matches_output_key() {
    // Verify that key_agg_context() produces a key matching the vault's output key.
    // This proves the vault creation and signing use the same key aggregation.
    let (_owner_sk, owner_pk) = test_keypair(1);
    let (_cosigner_sk, cosigner_pk) = test_keypair(2);
    let (_heir_sk, heir_pk) = test_keypair(3);

    let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");
    let heir_xonly = heir_pk.x_only_public_key().0;
    let heir_desc = DescriptorPublicKey::from_str(&format!("{}", heir_xonly)).unwrap();

    let vault = create_inheritable_vault(
        &owner_pk,
        &delegated,
        0,
        PathInfo::Single(heir_desc),
        Timelock::from_blocks(100).unwrap(),
        0,
        Network::Testnet,
    )
    .unwrap();

    let (_, tweaked_xonly) = vault.key_agg_context().unwrap();
    let output_key = vault.taproot_spend_info.output_key().to_x_only_public_key();

    assert_eq!(
        tweaked_xonly.serialize(),
        output_key.serialize(),
        "key_agg_context output must match vault output key for any timelock"
    );
}

#[test]
fn test_simple_addition_cannot_sign() {
    // Prove that the old aggregate_taproot_key (simple addition) produces a
    // DIFFERENT internal key than MuSig2 key aggregation. This is why the
    // migration was necessary.
    use nostring_ccd::musig::musig2_key_agg;

    let (_owner_sk, owner_pk) = test_keypair(1);
    let (_cosigner_sk, cosigner_pk) = test_keypair(2);
    let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");
    let disclosure = compute_tweak(&delegated, 0).unwrap();

    // Simple addition: P = A + B
    let simple_combined = owner_pk.combine(&disclosure.derived_pubkey).unwrap();
    let (simple_xonly, _) = simple_combined.x_only_public_key();

    // MuSig2 aggregation: P = a₁·A + a₂·B (with KeyAgg coefficients)
    let (_, musig_xonly) = musig2_key_agg(&owner_pk, &disclosure.derived_pubkey).unwrap();

    // They MUST differ — that's the whole point of MuSig2's rogue-key protection
    assert_ne!(
        simple_xonly.serialize(),
        musig_xonly.serialize(),
        "simple addition and MuSig2 aggregation must produce different keys"
    );
}
