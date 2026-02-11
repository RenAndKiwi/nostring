//! MuSig2 (BIP-327) signature aggregation for CCD vaults.
//!
//! Bridges between our secp256k1 0.29 types and the musig2 crate's types
//! via byte serialization. Both use libsecp256k1 underneath.
//!
//! The MuSig2 protocol runs in two rounds:
//!   Round 1: Nonce exchange (both parties generate and share PubNonces)
//!   Round 2: Partial signing + aggregation → single Schnorr signature
//!
//! The final signature is indistinguishable from a single-signer P2TR spend.

use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey};
use musig2::{AggNonce, KeyAggContext, LiftedSignature, PubNonce, SecNonce};

use crate::types::CcdError;

// ─── Type conversion helpers ────────────────────────────────────────────────

/// Convert our secp256k1 0.29 PublicKey to musig2's secp256k1 0.31 PublicKey.
fn pubkey_to_musig(pk: &PublicKey) -> Result<musig2::secp256k1::PublicKey, CcdError> {
    musig2::secp256k1::PublicKey::from_slice(&pk.serialize())
        .map_err(|e| CcdError::DerivationFailed(format!("pubkey conversion: {}", e)))
}

/// Convert our secp256k1 0.29 SecretKey to musig2's secp256k1 0.31 SecretKey.
fn seckey_to_musig(sk: &SecretKey) -> Result<musig2::secp256k1::SecretKey, CcdError> {
    #[allow(deprecated)]
    musig2::secp256k1::SecretKey::from_slice(&sk.secret_bytes())
        .map_err(|e| CcdError::SigningError(format!("seckey conversion: {}", e)))
}

// ─── Key Aggregation ────────────────────────────────────────────────────────

/// Aggregate two public keys using MuSig2 (BIP-327) key aggregation.
///
/// Unlike simple key addition (Phase 2), MuSig2 key aggregation includes
/// KeyAgg coefficients that prevent rogue-key attacks.
///
/// Returns the KeyAggContext (needed for signing) and the aggregated x-only pubkey.
pub fn musig2_key_agg(
    owner_pubkey: &PublicKey,
    cosigner_pubkey: &PublicKey,
) -> Result<(KeyAggContext, bitcoin::key::XOnlyPublicKey), CcdError> {
    let owner_m = pubkey_to_musig(owner_pubkey)?;
    let cosigner_m = pubkey_to_musig(cosigner_pubkey)?;

    let key_agg_ctx = KeyAggContext::new(vec![owner_m, cosigner_m])
        .map_err(|e| CcdError::DerivationFailed(format!("key aggregation: {}", e)))?;

    let agg_pk: musig2::secp256k1::PublicKey = key_agg_ctx.aggregated_pubkey();
    let (xonly, _parity) = agg_pk.x_only_public_key();

    let our_xonly = bitcoin::key::XOnlyPublicKey::from_slice(&xonly.serialize())
        .map_err(|e| CcdError::DerivationFailed(format!("xonly conversion: {}", e)))?;

    Ok((key_agg_ctx, our_xonly))
}

/// Apply the BIP-341 Taproot tweak to a KeyAggContext.
///
/// The on-chain output key is Q = P + H("TapTweak" || P) * G.
/// Both parties must sign for Q, not P. This tweaks the KeyAggContext
/// so that partial signatures produce a valid signature for Q.
pub fn musig2_key_agg_tweaked(
    owner_pubkey: &PublicKey,
    cosigner_pubkey: &PublicKey,
) -> Result<(KeyAggContext, bitcoin::key::XOnlyPublicKey), CcdError> {
    let owner_m = pubkey_to_musig(owner_pubkey)?;
    let cosigner_m = pubkey_to_musig(cosigner_pubkey)?;

    let key_agg_ctx = KeyAggContext::new(vec![owner_m, cosigner_m])
        .map_err(|e| CcdError::DerivationFailed(format!("key aggregation: {}", e)))?;

    // Apply the BIP-341 taproot tweak for key-path-only spending (no script tree).
    // This computes: t = H("TapTweak" || P), Q = P + t*G
    let tweaked_ctx = key_agg_ctx
        .with_unspendable_taproot_tweak()
        .map_err(|e| CcdError::DerivationFailed(format!("taproot tweak: {}", e)))?;

    let tweaked_pk: musig2::secp256k1::PublicKey = tweaked_ctx.aggregated_pubkey();
    let (tweaked_xonly, _) = tweaked_pk.x_only_public_key();

    let our_tweaked_xonly = bitcoin::key::XOnlyPublicKey::from_slice(&tweaked_xonly.serialize())
        .map_err(|e| CcdError::DerivationFailed(format!("xonly conversion: {}", e)))?;

    Ok((tweaked_ctx, our_tweaked_xonly))
}

// ─── Nonce Generation ───────────────────────────────────────────────────────

/// Generate a nonce pair (secret + public) for a MuSig2 signing session.
///
/// CRITICAL: The returned SecNonce MUST be used exactly once and then dropped.
/// Reusing a SecNonce across different messages reveals the private key.
pub fn generate_nonce(
    seckey: &SecretKey,
    key_agg_ctx: &KeyAggContext,
    message: Option<&[u8]>,
) -> Result<(SecNonce, PubNonce), CcdError> {
    let sk_m = seckey_to_musig(seckey)?;
    let agg_pk: musig2::secp256k1::PublicKey = key_agg_ctx.aggregated_pubkey();

    let mut nonce_seed = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce_seed);

    let msg_vec: Option<Vec<u8>> = message.map(|m| m.to_vec());

    let mut builder = SecNonce::build(nonce_seed)
        .with_seckey(sk_m)
        .with_aggregated_pubkey(agg_pk);

    if let Some(ref msg) = msg_vec {
        builder = builder.with_message(msg);
    }

    let secnonce = builder.build();
    let pubnonce = secnonce.public_nonce();

    Ok((secnonce, pubnonce))
}

// ─── Partial Signing ────────────────────────────────────────────────────────

/// Produce a partial signature for a MuSig2 session.
///
/// Both parties call this after exchanging PubNonces. The `agg_nonce` is
/// computed from both parties' PubNonces.
///
/// The SecNonce is consumed (moved) to prevent reuse.
pub fn partial_sign(
    seckey: &SecretKey,
    secnonce: SecNonce,
    key_agg_ctx: &KeyAggContext,
    agg_nonce: &AggNonce,
    message: &[u8; 32],
) -> Result<musig2::PartialSignature, CcdError> {
    let sk_m = seckey_to_musig(seckey)?;

    musig2::sign_partial(key_agg_ctx, sk_m, secnonce, agg_nonce, message)
        .map_err(|e| CcdError::SigningError(format!("partial sign: {}", e)))
}

/// Verify a partial signature from a specific signer.
pub fn verify_partial_signature(
    key_agg_ctx: &KeyAggContext,
    partial_sig: &musig2::PartialSignature,
    agg_nonce: &AggNonce,
    signer_pubkey: &PublicKey,
    signer_pubnonce: &PubNonce,
    message: &[u8; 32],
) -> bool {
    let pk_m = match pubkey_to_musig(signer_pubkey) {
        Ok(pk) => pk,
        Err(_) => return false,
    };

    musig2::verify_partial(
        key_agg_ctx,
        *partial_sig,
        agg_nonce,
        pk_m,
        signer_pubnonce,
        message,
    )
    .is_ok()
}

// ─── Signature Aggregation ──────────────────────────────────────────────────

/// Aggregate partial signatures into a final Schnorr signature.
///
/// The result is a standard 64-byte BIP-340 Schnorr signature, valid
/// under the aggregated (and possibly taproot-tweaked) public key.
pub fn aggregate_signatures(
    key_agg_ctx: &KeyAggContext,
    agg_nonce: &AggNonce,
    partial_sigs: &[musig2::PartialSignature],
    message: &[u8; 32],
) -> Result<[u8; 64], CcdError> {
    let sig: LiftedSignature = musig2::aggregate_partial_signatures(
        key_agg_ctx,
        agg_nonce,
        partial_sigs.iter().copied(),
        message,
    )
    .map_err(|e| CcdError::SigningError(format!("sig aggregation: {}", e)))?;

    Ok(sig.serialize())
}

/// Verify a final aggregated Schnorr signature against the aggregate x-only pubkey.
pub fn verify_aggregated_signature(
    aggregate_xonly: &bitcoin::key::XOnlyPublicKey,
    signature: &[u8; 64],
    message: &[u8; 32],
) -> bool {
    let secp = Secp256k1::verification_only();
    let sig = match bitcoin::secp256k1::schnorr::Signature::from_slice(signature) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let msg = bitcoin::secp256k1::Message::from_digest(*message);

    let xonly = match bitcoin::secp256k1::XOnlyPublicKey::from_slice(&aggregate_xonly.serialize()) {
        Ok(x) => x,
        Err(_) => return false,
    };

    secp.verify_schnorr(&sig, &msg, &xonly).is_ok()
}

// ─── Aggregate Nonce ────────────────────────────────────────────────────────

/// Compute the aggregate nonce from all parties' PubNonces.
pub fn aggregate_nonces(pubnonces: &[PubNonce]) -> AggNonce {
    musig2::AggNonce::sum(pubnonces)
}

// ─── Serialization helpers for transport ────────────────────────────────────

/// Serialize a PubNonce to bytes for Nostr transport.
pub fn pubnonce_to_bytes(pubnonce: &PubNonce) -> Vec<u8> {
    pubnonce.serialize().to_vec()
}

/// Deserialize a PubNonce from bytes.
pub fn pubnonce_from_bytes(bytes: &[u8]) -> Result<PubNonce, CcdError> {
    PubNonce::from_bytes(bytes)
        .map_err(|e| CcdError::TransportError(format!("invalid pubnonce: {}", e)))
}

/// Serialize a partial signature to bytes.
pub fn partial_sig_to_bytes(sig: &musig2::PartialSignature) -> [u8; 32] {
    sig.serialize()
}

/// Deserialize a partial signature from bytes.
pub fn partial_sig_from_bytes(bytes: &[u8; 32]) -> Result<musig2::PartialSignature, CcdError> {
    musig2::PartialSignature::from_slice(bytes)
        .map_err(|e| CcdError::TransportError(format!("invalid partial sig: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::secp256k1::Secp256k1;

    fn test_keypair(seed_byte: u8) -> (SecretKey, PublicKey) {
        let secp = Secp256k1::new();
        let mut bytes = [0u8; 32];
        bytes[31] = seed_byte;
        bytes[0] = 0x01;
        let sk = SecretKey::from_slice(&bytes).unwrap();
        let pk = sk.public_key(&secp);
        (sk, pk)
    }

    #[test]
    fn test_musig2_key_aggregation() {
        let (_sk1, pk1) = test_keypair(1);
        let (_sk2, pk2) = test_keypair(42);

        let (_ctx, agg_xonly) = musig2_key_agg(&pk1, &pk2).unwrap();

        // Should be deterministic
        let (_ctx2, agg_xonly2) = musig2_key_agg(&pk1, &pk2).unwrap();
        assert_eq!(agg_xonly, agg_xonly2);

        // Aggregate key should differ from either individual key
        let (pk1_xonly, _) = pk1.x_only_public_key();
        let (pk2_xonly, _) = pk2.x_only_public_key();
        assert_ne!(agg_xonly.serialize(), pk1_xonly.serialize());
        assert_ne!(agg_xonly.serialize(), pk2_xonly.serialize());
    }

    #[test]
    fn test_musig2_full_signing_roundtrip() {
        let (owner_sk, owner_pk) = test_keypair(1);
        let (cosigner_sk, cosigner_pk) = test_keypair(42);

        // Key aggregation
        let (key_agg_ctx, agg_xonly) = musig2_key_agg(&owner_pk, &cosigner_pk).unwrap();

        // Message to sign (simulated sighash)
        let message = [0xABu8; 32];

        // Round 1: Both generate nonces
        let (owner_secnonce, owner_pubnonce) =
            generate_nonce(&owner_sk, &key_agg_ctx, Some(&message)).unwrap();
        let (cosigner_secnonce, cosigner_pubnonce) =
            generate_nonce(&cosigner_sk, &key_agg_ctx, Some(&message)).unwrap();

        // Aggregate nonces
        let agg_nonce = aggregate_nonces(&[owner_pubnonce.clone(), cosigner_pubnonce.clone()]);

        // Round 2: Both produce partial signatures
        let owner_partial =
            partial_sign(&owner_sk, owner_secnonce, &key_agg_ctx, &agg_nonce, &message).unwrap();
        let cosigner_partial =
            partial_sign(&cosigner_sk, cosigner_secnonce, &key_agg_ctx, &agg_nonce, &message)
                .unwrap();

        // Verify partial signatures
        assert!(verify_partial_signature(
            &key_agg_ctx,
            &owner_partial,
            &agg_nonce,
            &owner_pk,
            &owner_pubnonce,
            &message,
        ));
        assert!(verify_partial_signature(
            &key_agg_ctx,
            &cosigner_partial,
            &agg_nonce,
            &cosigner_pk,
            &cosigner_pubnonce,
            &message,
        ));

        // Aggregate into final signature
        let final_sig = aggregate_signatures(
            &key_agg_ctx,
            &agg_nonce,
            &[owner_partial, cosigner_partial],
            &message,
        )
        .unwrap();

        // Verify final signature against aggregate key
        assert!(verify_aggregated_signature(&agg_xonly, &final_sig, &message));
    }

    #[test]
    fn test_musig2_taproot_tweaked_signing() {
        let (owner_sk, owner_pk) = test_keypair(1);
        let (cosigner_sk, cosigner_pk) = test_keypair(42);

        // Taproot-tweaked key aggregation
        let (tweaked_ctx, tweaked_xonly) =
            musig2_key_agg_tweaked(&owner_pk, &cosigner_pk).unwrap();

        // Untweaked aggregation for comparison
        let (_untweaked_ctx, untweaked_xonly) = musig2_key_agg(&owner_pk, &cosigner_pk).unwrap();

        // Tweaked key should differ from untweaked
        assert_ne!(tweaked_xonly, untweaked_xonly);

        // Full signing with tweaked context
        let message = [0xCDu8; 32];

        let (owner_secnonce, owner_pubnonce) =
            generate_nonce(&owner_sk, &tweaked_ctx, Some(&message)).unwrap();
        let (cosigner_secnonce, cosigner_pubnonce) =
            generate_nonce(&cosigner_sk, &tweaked_ctx, Some(&message)).unwrap();

        let agg_nonce = aggregate_nonces(&[owner_pubnonce, cosigner_pubnonce]);

        let owner_partial =
            partial_sign(&owner_sk, owner_secnonce, &tweaked_ctx, &agg_nonce, &message).unwrap();
        let cosigner_partial =
            partial_sign(&cosigner_sk, cosigner_secnonce, &tweaked_ctx, &agg_nonce, &message)
                .unwrap();

        let final_sig = aggregate_signatures(
            &tweaked_ctx,
            &agg_nonce,
            &[owner_partial, cosigner_partial],
            &message,
        )
        .unwrap();

        // Verify against the TWEAKED key (this is what Bitcoin nodes check)
        assert!(verify_aggregated_signature(&tweaked_xonly, &final_sig, &message));

        // Should NOT verify against the untweaked key
        assert!(!verify_aggregated_signature(&untweaked_xonly, &final_sig, &message));
    }

    #[test]
    fn test_musig2_wrong_key_partial_sig_rejected() {
        let (owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(42);
        let (wrong_sk, _wrong_pk) = test_keypair(99);

        let (key_agg_ctx, _) = musig2_key_agg(&owner_pk, &cosigner_pk).unwrap();
        let message = [0xABu8; 32];

        let (_owner_secnonce, owner_pubnonce) =
            generate_nonce(&owner_sk, &key_agg_ctx, Some(&message)).unwrap();
        let (wrong_secnonce, wrong_pubnonce) =
            generate_nonce(&wrong_sk, &key_agg_ctx, Some(&message)).unwrap();

        let agg_nonce = aggregate_nonces(&[owner_pubnonce, wrong_pubnonce.clone()]);

        // Wrong key signs as co-signer
        let wrong_partial =
            partial_sign(&wrong_sk, wrong_secnonce, &key_agg_ctx, &agg_nonce, &message);

        // Partial sign may succeed, but verification against cosigner_pk should fail
        if let Ok(partial) = wrong_partial {
            assert!(
                !verify_partial_signature(
                    &key_agg_ctx,
                    &partial,
                    &agg_nonce,
                    &cosigner_pk,  // expected signer
                    &wrong_pubnonce,
                    &message,
                ),
                "Partial sig from wrong key should fail verification against expected key"
            );
        }
    }

    #[test]
    fn test_musig2_mismatched_messages_invalid() {
        let (owner_sk, owner_pk) = test_keypair(1);
        let (cosigner_sk, cosigner_pk) = test_keypair(42);

        let (key_agg_ctx, _agg_xonly) = musig2_key_agg(&owner_pk, &cosigner_pk).unwrap();
        let message = [0xABu8; 32];
        let different_message = [0xFFu8; 32];

        let (owner_secnonce, owner_pubnonce) =
            generate_nonce(&owner_sk, &key_agg_ctx, Some(&message)).unwrap();
        let (cosigner_secnonce, cosigner_pubnonce) =
            generate_nonce(&cosigner_sk, &key_agg_ctx, Some(&message)).unwrap();

        let agg_nonce = aggregate_nonces(&[owner_pubnonce, cosigner_pubnonce]);

        let owner_partial =
            partial_sign(&owner_sk, owner_secnonce, &key_agg_ctx, &agg_nonce, &message).unwrap();

        // Co-signer signs a DIFFERENT message
        let cosigner_partial = partial_sign(
            &cosigner_sk,
            cosigner_secnonce,
            &key_agg_ctx,
            &agg_nonce,
            &different_message,
        )
        .unwrap();

        // Aggregation should FAIL because partial sigs don't match
        let result = aggregate_signatures(
            &key_agg_ctx,
            &agg_nonce,
            &[owner_partial, cosigner_partial],
            &message,
        );

        assert!(
            result.is_err(),
            "Aggregation with mismatched messages should fail verification"
        );
    }

    #[test]
    fn test_pubnonce_serialization_roundtrip() {
        let (sk, pk) = test_keypair(1);
        let (key_agg_ctx, _) = musig2_key_agg(&pk, &test_keypair(42).1).unwrap();

        let (_secnonce, pubnonce) = generate_nonce(&sk, &key_agg_ctx, None).unwrap();

        let bytes = pubnonce_to_bytes(&pubnonce);
        let recovered = pubnonce_from_bytes(&bytes).unwrap();

        assert_eq!(pubnonce.serialize(), recovered.serialize());
    }

    #[test]
    fn test_partial_sig_serialization_roundtrip() {
        let (owner_sk, owner_pk) = test_keypair(1);
        let (cosigner_sk, cosigner_pk) = test_keypair(42);

        let (key_agg_ctx, _) = musig2_key_agg(&owner_pk, &cosigner_pk).unwrap();
        let message = [0xABu8; 32];

        let (owner_secnonce, owner_pubnonce) =
            generate_nonce(&owner_sk, &key_agg_ctx, Some(&message)).unwrap();
        let (_cosigner_secnonce, cosigner_pubnonce) =
            generate_nonce(&cosigner_sk, &key_agg_ctx, Some(&message)).unwrap();

        let agg_nonce = aggregate_nonces(&[owner_pubnonce, cosigner_pubnonce]);

        let partial =
            partial_sign(&owner_sk, owner_secnonce, &key_agg_ctx, &agg_nonce, &message).unwrap();

        let bytes = partial_sig_to_bytes(&partial);
        let recovered = partial_sig_from_bytes(&bytes).unwrap();

        assert_eq!(partial.serialize(), recovered.serialize());
    }

    #[test]
    fn test_musig2_with_ccd_derived_keys() {
        // Integration test: CCD tweak → derived co-signer key → MuSig2 signing
        // This is the actual flow for vault spending.
        let (cosigner_sk, cosigner_pk) = test_keypair(42);
        let (owner_sk, owner_pk) = test_keypair(1);

        let delegated = crate::register_cosigner(cosigner_pk, "test");
        let disclosure = crate::compute_tweak(&delegated, 0).unwrap();

        // Owner knows both keys
        let cosigner_derived_pk = disclosure.derived_pubkey;

        // MuSig2 key aggregation with the DERIVED co-signer key
        let (key_agg_ctx, agg_xonly) =
            musig2_key_agg(&owner_pk, &cosigner_derived_pk).unwrap();

        let message = [0xABu8; 32];

        // Co-signer derives their child secret key
        let cosigner_child_sk = crate::apply_tweak(&cosigner_sk, &disclosure.tweak).unwrap();

        // Both generate nonces
        let (owner_secnonce, owner_pubnonce) =
            generate_nonce(&owner_sk, &key_agg_ctx, Some(&message)).unwrap();
        let (cosigner_secnonce, cosigner_pubnonce) =
            generate_nonce(&cosigner_child_sk, &key_agg_ctx, Some(&message)).unwrap();

        let agg_nonce = aggregate_nonces(&[owner_pubnonce, cosigner_pubnonce]);

        // Both sign (co-signer uses DERIVED key)
        let owner_partial =
            partial_sign(&owner_sk, owner_secnonce, &key_agg_ctx, &agg_nonce, &message).unwrap();
        let cosigner_partial = partial_sign(
            &cosigner_child_sk,
            cosigner_secnonce,
            &key_agg_ctx,
            &agg_nonce,
            &message,
        )
        .unwrap();

        // Aggregate
        let final_sig = aggregate_signatures(
            &key_agg_ctx,
            &agg_nonce,
            &[owner_partial, cosigner_partial],
            &message,
        )
        .unwrap();

        // Verify against aggregate key
        assert!(
            verify_aggregated_signature(&agg_xonly, &final_sig, &message),
            "CCD-derived MuSig2 signature must be valid"
        );
    }

    #[test]
    fn test_taproot_output_key_matches_p2tr_address() {
        // Verify that our MuSig2 tweaked key matches what bitcoin::Address::p2tr produces
        let (_sk1, pk1) = test_keypair(1);
        let (_sk2, pk2) = test_keypair(42);

        // Get untweaked aggregate (internal key)
        let (_ctx, internal_xonly) = musig2_key_agg(&pk1, &pk2).unwrap();

        // Get tweaked aggregate (output key)
        let (_tweaked_ctx, output_xonly) = musig2_key_agg_tweaked(&pk1, &pk2).unwrap();

        // Build P2TR address from internal key using bitcoin crate
        let secp = Secp256k1::new();
        let addr = bitcoin::Address::p2tr(&secp, internal_xonly, None, bitcoin::Network::Signet);

        // Extract the output key from the address script
        // P2TR script: OP_1 OP_PUSH32 <output_key>
        let script = addr.script_pubkey();
        let script_bytes = script.as_bytes();
        // P2TR: 0x51 0x20 <32 bytes>
        assert_eq!(script_bytes[0], 0x51); // OP_1
        assert_eq!(script_bytes[1], 0x20); // push 32
        let addr_output_key =
            bitcoin::key::XOnlyPublicKey::from_slice(&script_bytes[2..34]).unwrap();

        assert_eq!(
            output_xonly, addr_output_key,
            "MuSig2 tweaked key must match P2TR address output key"
        );
    }

    #[test]
    fn test_both_sigs_valid_different_nonces() {
        // Different nonce seeds produce different but equally valid signatures
        let (owner_sk, owner_pk) = test_keypair(1);
        let (cosigner_sk, cosigner_pk) = test_keypair(42);

        let (key_agg_ctx, agg_xonly) = musig2_key_agg(&owner_pk, &cosigner_pk).unwrap();
        let message = [0xABu8; 32];

        // First signing
        let (on1, opn1) = generate_nonce(&owner_sk, &key_agg_ctx, Some(&message)).unwrap();
        let (cn1, cpn1) = generate_nonce(&cosigner_sk, &key_agg_ctx, Some(&message)).unwrap();
        let an1 = aggregate_nonces(&[opn1, cpn1]);
        let op1 = partial_sign(&owner_sk, on1, &key_agg_ctx, &an1, &message).unwrap();
        let cp1 = partial_sign(&cosigner_sk, cn1, &key_agg_ctx, &an1, &message).unwrap();
        let sig1 = aggregate_signatures(&key_agg_ctx, &an1, &[op1, cp1], &message).unwrap();

        // Second signing (different nonces)
        let (on2, opn2) = generate_nonce(&owner_sk, &key_agg_ctx, Some(&message)).unwrap();
        let (cn2, cpn2) = generate_nonce(&cosigner_sk, &key_agg_ctx, Some(&message)).unwrap();
        let an2 = aggregate_nonces(&[opn2, cpn2]);
        let op2 = partial_sign(&owner_sk, on2, &key_agg_ctx, &an2, &message).unwrap();
        let cp2 = partial_sign(&cosigner_sk, cn2, &key_agg_ctx, &an2, &message).unwrap();
        let sig2 = aggregate_signatures(&key_agg_ctx, &an2, &[op2, cp2], &message).unwrap();

        // Both valid, but different (different nonces)
        assert!(verify_aggregated_signature(&agg_xonly, &sig1, &message));
        assert!(verify_aggregated_signature(&agg_xonly, &sig2, &message));
        // Signatures will almost certainly differ due to random nonces
    }
}
