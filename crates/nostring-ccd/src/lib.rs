//! # Chain Code Delegation (CCD)
//!
//! Privacy-preserving collaborative custody for NoString.
//!
//! In standard BIP-32, sharing an xpub with a co-signer reveals the entire key tree.
//! CCD withholds the chain code — the co-signer receives only per-UTXO scalar tweaks
//! at signing time, learning nothing about other keys or transactions.
//!
//! Based on: Jurvis Tan & Jesse Posner, "Chain Code Delegation: Private Access Control
//! for Bitcoin Keys" (Delving Bitcoin, 2025).

pub mod blind;
mod fund_vault;
mod integration;
pub mod musig;
pub mod transport;
pub mod types;
pub mod vault;

use bitcoin::hashes::{sha512, Hash, HashEngine, Hmac, HmacEngine};
use bitcoin::secp256k1::{PublicKey, Scalar, Secp256k1, SecretKey};
use types::*;

/// Derive a deterministic chain code from a seed.
///
/// Uses HMAC-SHA512(seed, "nostring-ccd-chain-code") and takes the first 32 bytes.
/// This makes the vault address reproducible from the mnemonic alone.
///
/// For production, consider using a separate random chain code per co-signer
/// (via `generate_chain_code()`). This helper is primarily useful for demos
/// and recovery scenarios where reproducibility from a single seed matters.
pub fn derive_chain_code_from_seed(seed: &[u8; 64]) -> ChainCode {
    let mut engine = HmacEngine::<sha512::Hash>::new(b"nostring-ccd-chain-code");
    engine.input(seed);
    let hmac_result = Hmac::from_engine(engine);
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&hmac_result[..32]);
    ChainCode(bytes)
}

/// Generate a random chain code (32 bytes from CSPRNG).
///
/// The owner generates this FOR the co-signer's key. The co-signer never sees it.
pub fn generate_chain_code() -> ChainCode {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    ChainCode(bytes)
}

/// Register a co-signer with a specific chain code.
/// Use this for deterministic vault addresses (e.g., tests, recovery).
pub fn register_cosigner_with_chain_code(
    cosigner_pubkey: PublicKey,
    chain_code: ChainCode,
    label: &str,
) -> DelegatedKey {
    DelegatedKey {
        cosigner_pubkey,
        chain_code,
        label: label.to_string(),
    }
}

/// Register a co-signer with an owner-generated chain code.
pub fn register_cosigner(cosigner_pubkey: PublicKey, label: &str) -> DelegatedKey {
    DelegatedKey {
        cosigner_pubkey,
        chain_code: generate_chain_code(),
        label: label.to_string(),
    }
}

/// Compute the BIP-32 scalar tweak for a non-hardened child index.
///
/// This extracts the raw `I_L` value from BIP-32's CKDpub:
///   I = HMAC-SHA512(key=chain_code, data=ser_P(pubkey) || ser_32(index))
///   tweak = parse_256(I_L)  (first 32 bytes as scalar mod n)
///
/// The tweak is sent to the co-signer. They compute:
///   child_privkey = parent_privkey + tweak
///   child_pubkey  = parent_pubkey  + tweak * G
pub fn compute_tweak(
    delegated: &DelegatedKey,
    child_index: u32,
) -> Result<TweakDisclosure, CcdError> {
    // Hardened indices not supported (require private key)
    if child_index >= 0x80000000 {
        return Err(CcdError::HardenedIndex);
    }

    let secp = Secp256k1::new();

    // BIP-32 public derivation:
    // HMAC-SHA512(key=chain_code, data=ser_P(parent_pubkey) || ser_32(index))
    let mut engine = HmacEngine::<sha512::Hash>::new(&delegated.chain_code.0);
    engine.input(&delegated.cosigner_pubkey.serialize());
    engine.input(&child_index.to_be_bytes());
    let hmac_result = Hmac::from_engine(engine);

    // I_L = first 32 bytes (the tweak scalar)
    let il = &hmac_result[..32];

    // Parse as scalar (mod curve order)
    let tweak = Scalar::from_be_bytes({
        let mut arr = [0u8; 32];
        arr.copy_from_slice(il);
        arr
    })
    .map_err(|_| CcdError::TweakOutOfRange)?;

    // Derive expected child pubkey: parent + tweak*G
    let derived_pubkey = derive_child_pubkey(&secp, &delegated.cosigner_pubkey, &tweak)?;

    Ok(TweakDisclosure {
        tweak,
        derived_pubkey,
        child_index,
    })
}

/// Derive a child public key by adding tweak*G to the parent.
fn derive_child_pubkey(
    _secp: &Secp256k1<bitcoin::secp256k1::All>,
    parent: &PublicKey,
    tweak: &Scalar,
) -> Result<PublicKey, CcdError> {
    // tweak * G
    let secp_signing = Secp256k1::signing_only();
    let tweak_point = SecretKey::from_slice(&tweak.to_be_bytes())
        .map_err(|_| CcdError::TweakOutOfRange)?
        .public_key(&secp_signing);

    // parent + tweak*G
    parent
        .combine(&tweak_point)
        .map_err(|_| CcdError::DerivationFailed("point addition failed".into()))
}

/// Co-signer side: apply a tweak to derive the child secret key.
///
/// child_privkey = parent_privkey + tweak (mod n)
pub fn apply_tweak(secret_key: &SecretKey, tweak: &Scalar) -> Result<SecretKey, CcdError> {
    secret_key
        .add_tweak(tweak)
        .map_err(|_| CcdError::TweakOutOfRange)
}

/// Verify that a tweak produces the expected derived public key.
pub fn verify_tweak(
    cosigner_pubkey: &PublicKey,
    tweak: &Scalar,
    expected_derived: &PublicKey,
) -> bool {
    let secp = Secp256k1::new();
    match derive_child_pubkey(&secp, cosigner_pubkey, tweak) {
        Ok(derived) => derived == *expected_derived,
        Err(_) => false,
    }
}

/// Compute a simple Taproot-style aggregated x-only public key.
///
/// For Phase 1 this uses key addition (P_owner + P_cosigner) which produces
/// a MuSig2-compatible aggregate. Full MuSig2 with nonce commitments is Phase 2.
///
/// Returns the x-only public key and parity for the aggregated key.
pub fn aggregate_taproot_key(
    owner_pubkey: &PublicKey,
    cosigner_pubkey: &PublicKey,
) -> Result<bitcoin::key::XOnlyPublicKey, CcdError> {
    let combined = owner_pubkey
        .combine(cosigner_pubkey)
        .map_err(|_| CcdError::DerivationFailed("key aggregation failed".into()))?;

    let (xonly, _parity) = combined.x_only_public_key();
    Ok(xonly)
}

/// Derive a chain of tweaks for a derivation path (e.g., /0/5 = index 0 then index 5).
///
/// Each step uses the derived child pubkey from the previous step as the new parent.
pub fn compute_tweak_path(
    delegated: &DelegatedKey,
    path: &[u32],
) -> Result<Vec<TweakDisclosure>, CcdError> {
    if path.is_empty() {
        return Err(CcdError::InvalidPath("empty path".into()));
    }

    let secp = Secp256k1::new();
    let mut current_pubkey = delegated.cosigner_pubkey;
    let mut current_chain_code = delegated.chain_code.clone();
    let mut tweaks = Vec::with_capacity(path.len());

    for &index in path {
        if index >= 0x80000000 {
            return Err(CcdError::HardenedIndex);
        }

        // HMAC-SHA512
        let mut engine = HmacEngine::<sha512::Hash>::new(&current_chain_code.0);
        engine.input(&current_pubkey.serialize());
        engine.input(&index.to_be_bytes());
        let hmac_result = Hmac::from_engine(engine);

        let il = &hmac_result[..32];
        let ir = &hmac_result[32..];

        let tweak = Scalar::from_be_bytes({
            let mut arr = [0u8; 32];
            arr.copy_from_slice(il);
            arr
        })
        .map_err(|_| CcdError::TweakOutOfRange)?;

        let derived_pubkey = derive_child_pubkey(&secp, &current_pubkey, &tweak)?;

        tweaks.push(TweakDisclosure {
            tweak,
            derived_pubkey,
            child_index: index,
        });

        // Next iteration uses derived child pubkey and I_R as new chain code
        current_pubkey = derived_pubkey;
        current_chain_code.0.copy_from_slice(ir);
    }

    Ok(tweaks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::secp256k1::Secp256k1;

    /// Helper: generate a deterministic keypair from a known seed for reproducible tests
    fn test_keypair(seed_byte: u8) -> (SecretKey, PublicKey) {
        let secp = Secp256k1::new();
        let mut secret_bytes = [0u8; 32];
        secret_bytes[31] = seed_byte;
        secret_bytes[0] = 0x01; // ensure valid scalar
        let sk = SecretKey::from_slice(&secret_bytes).unwrap();
        let pk = sk.public_key(&secp);
        (sk, pk)
    }

    #[test]
    fn test_tweak_roundtrip() {
        // Owner registers co-signer
        let (cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test-cosigner");

        // Owner computes tweak for child index 0
        let disclosure = compute_tweak(&delegated, 0).unwrap();

        // Co-signer applies tweak
        let child_sk = apply_tweak(&cosigner_sk, &disclosure.tweak).unwrap();

        // Verify: child_sk's public key should match disclosure.derived_pubkey
        let secp = Secp256k1::new();
        let child_pk = child_sk.public_key(&secp);
        assert_eq!(child_pk, disclosure.derived_pubkey);
    }

    #[test]
    fn test_tweak_verification() {
        let (_sk, pk) = test_keypair(42);
        let delegated = register_cosigner(pk, "test");
        let disclosure = compute_tweak(&delegated, 0).unwrap();

        // Correct verification
        assert!(verify_tweak(
            &pk,
            &disclosure.tweak,
            &disclosure.derived_pubkey
        ));

        // Wrong parent pubkey
        let (_sk2, pk2) = test_keypair(99);
        assert!(!verify_tweak(
            &pk2,
            &disclosure.tweak,
            &disclosure.derived_pubkey
        ));
    }

    #[test]
    fn test_different_indices_different_tweaks() {
        let (_sk, pk) = test_keypair(42);
        let delegated = register_cosigner(pk, "test");

        let t0 = compute_tweak(&delegated, 0).unwrap();
        let t1 = compute_tweak(&delegated, 1).unwrap();
        let t2 = compute_tweak(&delegated, 2).unwrap();

        // All tweaks should be different
        assert_ne!(t0.tweak, t1.tweak);
        assert_ne!(t1.tweak, t2.tweak);
        assert_ne!(t0.tweak, t2.tweak);

        // All derived pubkeys should be different
        assert_ne!(t0.derived_pubkey, t1.derived_pubkey);
        assert_ne!(t1.derived_pubkey, t2.derived_pubkey);
    }

    #[test]
    fn test_different_cosigners_different_tweaks() {
        let (_sk1, pk1) = test_keypair(42);
        let (_sk2, pk2) = test_keypair(99);

        // Use the same chain code for both to isolate the pubkey difference
        let d1 = register_cosigner(pk1, "cosigner-1");
        let d2 = DelegatedKey {
            cosigner_pubkey: pk2,
            chain_code: d1.chain_code.clone(),
            label: "cosigner-2".into(),
        };

        let t1 = compute_tweak(&d1, 0).unwrap();
        let t2 = compute_tweak(&d2, 0).unwrap();

        // Same chain code, same index, different pubkey → different tweak
        assert_ne!(t1.tweak, t2.tweak);
        assert_ne!(t1.derived_pubkey, t2.derived_pubkey);

        // ChainCode implements ZeroizeOnDrop, manual zeroize not needed in test
    }

    #[test]
    fn test_hardened_index_rejected() {
        let (_sk, pk) = test_keypair(42);
        let delegated = register_cosigner(pk, "test");

        // Hardened index (>= 0x80000000) should fail
        let result = compute_tweak(&delegated, 0x80000000);
        assert!(matches!(result, Err(CcdError::HardenedIndex)));
    }

    #[test]
    fn test_bip32_compatibility() {
        // Verify that CCD tweak math matches standard BIP-32 derivation.
        // Generate a keypair, derive child via CCD, and verify it matches
        // child derived via standard BIP-32 (xpriv + xpub).
        use bitcoin::bip32::{Xpriv, Xpub};
        use bitcoin::Network;

        let secp = Secp256k1::new();

        // Known test seed
        let seed = [0xABu8; 64];
        let master = Xpriv::new_master(Network::Bitcoin, &seed).unwrap();

        // Extract parent pubkey and chain code from master
        let master_xpub = Xpub::from_priv(&secp, &master);
        let parent_pk = master_xpub.public_key;
        let chain_code_bytes = master_xpub.chain_code;

        // Set up CCD with the same chain code
        let delegated = DelegatedKey {
            cosigner_pubkey: parent_pk.into(),
            chain_code: ChainCode(chain_code_bytes.to_bytes()),
            label: "bip32-compat-test".into(),
        };

        // Derive child at index 0 via CCD
        let disclosure = compute_tweak(&delegated, 0).unwrap();

        // Derive child at index 0 via standard BIP-32
        let child_xpub = master_xpub
            .ckd_pub(&secp, bitcoin::bip32::ChildNumber::Normal { index: 0 })
            .unwrap();

        // The derived public keys must match
        let standard_child_pk: PublicKey = child_xpub.public_key.into();
        assert_eq!(
            disclosure.derived_pubkey, standard_child_pk,
            "CCD-derived pubkey must match standard BIP-32 derivation"
        );

        // Also verify via tweak application on the private key side
        let child_sk = apply_tweak(&master.private_key, &disclosure.tweak).unwrap();
        let child_pk_from_sk = child_sk.public_key(&secp);
        assert_eq!(
            child_pk_from_sk, standard_child_pk,
            "Tweak-applied privkey must produce same pubkey as BIP-32"
        );
    }

    #[test]
    fn test_taproot_key_aggregation() {
        let (_sk1, pk1) = test_keypair(42);
        let (_sk2, pk2) = test_keypair(99);

        let aggregate = aggregate_taproot_key(&pk1, &pk2).unwrap();

        // Aggregate should be deterministic
        let aggregate2 = aggregate_taproot_key(&pk1, &pk2).unwrap();
        assert_eq!(aggregate, aggregate2);

        // Different key order should produce different aggregate
        // (unlike full MuSig2 which sorts keys)
        let aggregate_reversed = aggregate_taproot_key(&pk2, &pk1).unwrap();
        // Note: P1+P2 == P2+P1 in EC addition, so these should actually be equal
        assert_eq!(aggregate, aggregate_reversed);
    }

    #[test]
    fn test_boundary_child_index() {
        let (_sk, pk) = test_keypair(42);
        let delegated = register_cosigner(pk, "test");

        // Index 0 (minimum)
        assert!(compute_tweak(&delegated, 0).is_ok());

        // Index 2^31 - 1 (maximum non-hardened)
        assert!(compute_tweak(&delegated, 0x7FFFFFFF).is_ok());

        // Index 2^31 (first hardened — should fail)
        assert!(compute_tweak(&delegated, 0x80000000).is_err());
    }

    #[test]
    fn test_tweak_path_derivation() {
        let (cosigner_sk, cosigner_pk) = test_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "test");

        // Derive path /0/5
        let tweaks = compute_tweak_path(&delegated, &[0, 5]).unwrap();
        assert_eq!(tweaks.len(), 2);

        // Apply both tweaks sequentially on the co-signer side
        let child1_sk = apply_tweak(&cosigner_sk, &tweaks[0].tweak).unwrap();
        let child2_sk = apply_tweak(&child1_sk, &tweaks[1].tweak).unwrap();

        // Final derived pubkey should match
        let secp = Secp256k1::new();
        let final_pk = child2_sk.public_key(&secp);
        assert_eq!(final_pk, tweaks[1].derived_pubkey);
    }

    #[test]
    fn test_near_curve_order_tweak() {
        // The curve order n for secp256k1:
        // FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
        // A tweak >= n should be rejected by Scalar::from_be_bytes.
        // In practice this is ~1 in 2^128, but we verify the error path works.
        let (_sk, _pk) = test_keypair(42);

        // Manually craft a DelegatedKey and force an HMAC that would produce
        // an out-of-range scalar. We can't easily force this, so instead test
        // that Scalar::from_be_bytes rejects the curve order itself.
        let curve_order: [u8; 32] = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C,
            0xD0, 0x36, 0x41, 0x41,
        ];
        assert!(
            Scalar::from_be_bytes(curve_order).is_err(),
            "curve order itself should be rejected as a scalar"
        );

        // One above curve order
        let mut above_order = curve_order;
        above_order[31] = 0x42;
        assert!(
            Scalar::from_be_bytes(above_order).is_err(),
            "value above curve order should be rejected"
        );

        // All 0xFF should be rejected (way above curve order)
        let max_bytes = [0xFF; 32];
        assert!(
            Scalar::from_be_bytes(max_bytes).is_err(),
            "all-0xFF should be rejected"
        );

        // Zero is valid (edge case)
        let zero_bytes = [0u8; 32];
        assert!(
            Scalar::from_be_bytes(zero_bytes).is_ok(),
            "zero should be a valid scalar"
        );
    }

    #[test]
    fn test_bip32_compatibility_multiple_indices() {
        // Verify CCD matches standard BIP-32 across multiple seeds and indices.
        use bitcoin::bip32::{Xpriv, Xpub};
        use bitcoin::Network;

        let secp = Secp256k1::new();

        // Test with multiple seeds
        let seeds: Vec<[u8; 64]> = vec![[0xAB; 64], [0x01; 64], {
            let mut s = [0u8; 64];
            for (i, b) in s.iter_mut().enumerate() {
                *b = i as u8;
            }
            s
        }];

        for (seed_idx, seed) in seeds.iter().enumerate() {
            let master = Xpriv::new_master(Network::Bitcoin, seed).unwrap();
            let master_xpub = Xpub::from_priv(&secp, &master);

            let delegated = DelegatedKey {
                cosigner_pubkey: master_xpub.public_key.into(),
                chain_code: ChainCode(master_xpub.chain_code.to_bytes()),
                label: format!("compat-test-{}", seed_idx),
            };

            // Test indices 0, 1, 7, 42, 1000, 2^31-1
            for &index in &[0u32, 1, 7, 42, 1000, 0x7FFFFFFF] {
                let disclosure = compute_tweak(&delegated, index).unwrap();

                let child_xpub = master_xpub
                    .ckd_pub(&secp, bitcoin::bip32::ChildNumber::Normal { index })
                    .unwrap();

                let standard_pk: PublicKey = child_xpub.public_key.into();
                assert_eq!(
                    disclosure.derived_pubkey, standard_pk,
                    "CCD mismatch for seed {} index {}",
                    seed_idx, index
                );

                // Also verify via private key tweak application
                let child_sk = apply_tweak(&master.private_key, &disclosure.tweak).unwrap();
                assert_eq!(
                    child_sk.public_key(&secp),
                    standard_pk,
                    "Privkey tweak mismatch for seed {} index {}",
                    seed_idx,
                    index
                );
            }
        }
    }

    #[test]
    fn test_tweak_path_matches_bip32_multilevel() {
        // Verify that compute_tweak_path(/0/5) matches
        // xpub.derive_pub(0).derive_pub(5) from standard BIP-32.
        use bitcoin::bip32::{ChildNumber, Xpriv, Xpub};
        use bitcoin::Network;

        let secp = Secp256k1::new();
        let seed = [0xAB; 64];
        let master = Xpriv::new_master(Network::Bitcoin, &seed).unwrap();
        let master_xpub = Xpub::from_priv(&secp, &master);

        let delegated = DelegatedKey {
            cosigner_pubkey: master_xpub.public_key.into(),
            chain_code: ChainCode(master_xpub.chain_code.to_bytes()),
            label: "path-compat".into(),
        };

        // CCD: derive /0/5
        let tweaks = compute_tweak_path(&delegated, &[0, 5]).unwrap();
        assert_eq!(tweaks.len(), 2);

        // Standard BIP-32: derive /0/5
        let child_0 = master_xpub
            .ckd_pub(&secp, ChildNumber::Normal { index: 0 })
            .unwrap();
        let child_0_5 = child_0
            .ckd_pub(&secp, ChildNumber::Normal { index: 5 })
            .unwrap();

        let standard_pk: PublicKey = child_0_5.public_key.into();
        assert_eq!(
            tweaks[1].derived_pubkey, standard_pk,
            "Multi-level CCD path must match standard BIP-32 derivation"
        );

        // Also test a deeper path: /1/2/3
        let tweaks_deep = compute_tweak_path(&delegated, &[1, 2, 3]).unwrap();
        let c1 = master_xpub
            .ckd_pub(&secp, ChildNumber::Normal { index: 1 })
            .unwrap();
        let c1_2 = c1.ckd_pub(&secp, ChildNumber::Normal { index: 2 }).unwrap();
        let c1_2_3 = c1_2
            .ckd_pub(&secp, ChildNumber::Normal { index: 3 })
            .unwrap();
        let deep_pk: PublicKey = c1_2_3.public_key.into();
        assert_eq!(
            tweaks_deep[2].derived_pubkey, deep_pk,
            "Deep path /1/2/3 must match standard BIP-32"
        );
    }

    #[test]
    fn test_empty_path_rejected() {
        let (_sk, pk) = test_keypair(42);
        let delegated = register_cosigner(pk, "test");

        let result = compute_tweak_path(&delegated, &[]);
        assert!(matches!(result, Err(CcdError::InvalidPath(_))));
    }
}
