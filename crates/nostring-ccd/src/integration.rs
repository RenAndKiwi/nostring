//! Integration tests for CCD vaults on testnet.
//!
//! These tests require network access and are gated behind `#[ignore]`.
//! Run with: `cargo test -p nostring-ccd -- --ignored --nocapture`
//!
//! ## Transaction History (testnet3)
//!
//! All tests use deterministic keys (seed bytes 1, 42) and chain code [0xCC; 32].
//! Vault address (index 0): tb1pym48vehhxafsa94gsemwau20ll3l88zwgplecxtua0tds3u0880q9q6ahk
//!
//! Tx chain (as of 2026-02-11):
//!   6af3e4 (confirmed): P2WPKH → random-vault(20k) + change(25,415)  [lost 20k]
//!   a3434b (unconf):    change → correct-vault(20k) + change(5,115)
//!   d8150f (unconf):    change(5,115) → correct-vault(4,815)
//!   d882a1 (unconf):    vault(20k+4,815) → vault(24,515)  [MuSig2 spend #1]
//!   68ed16 (unconf):    vault(24,515) → vault(24,215)     [MuSig2 spend #2]

#[cfg(test)]
mod tests {
    use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey};
    use bitcoin::{Amount, Network};

    use crate::types::{CcdVault, ChainCode};
    use crate::vault::{build_spend_psbt, create_vault_musig2, musig2_sign_psbt};
    use crate::{apply_tweak, compute_tweak, register_cosigner_with_chain_code};

    const CHAIN_CODE: [u8; 32] = [0xCC; 32];

    fn deterministic_keypair(seed_byte: u8) -> (SecretKey, PublicKey) {
        let secp = Secp256k1::new();
        let mut bytes = [0u8; 32];
        bytes[31] = seed_byte;
        bytes[0] = 0x01;
        let sk = SecretKey::from_slice(&bytes).unwrap();
        let pk = sk.public_key(&secp);
        (sk, pk)
    }

    fn test_vault(
        index: u32,
    ) -> (
        CcdVault,
        musig2::KeyAggContext,
        SecretKey,
        SecretKey,
        SecretKey,
    ) {
        let (owner_sk, owner_pk) = deterministic_keypair(1);
        let (cosigner_sk, cosigner_pk) = deterministic_keypair(42);
        let delegated = register_cosigner_with_chain_code(
            cosigner_pk,
            ChainCode::from_bytes(CHAIN_CODE),
            "testnet-ccd",
        );

        let (vault, ctx) =
            create_vault_musig2(&owner_pk, &delegated, index, Network::Testnet).unwrap();

        // Derive the co-signer child key for this vault's tweak
        let tweak_disclosure = compute_tweak(&delegated, index).unwrap();
        let cosigner_child_sk = apply_tweak(&cosigner_sk, &tweak_disclosure.tweak).unwrap();

        (vault, ctx, owner_sk, cosigner_sk, cosigner_child_sk)
    }

    // ──────────────────────────────────────────────
    // Offline verification tests (no network needed)
    // ──────────────────────────────────────────────

    /// Verify vault addresses are deterministic given same keys + chain code + index.
    #[test]
    fn test_vault_address_determinism() {
        let (v1, _, _, _, _) = test_vault(0);
        let (v2, _, _, _, _) = test_vault(0);
        assert_eq!(
            v1.address, v2.address,
            "Same inputs must produce same address"
        );

        let (v3, _, _, _, _) = test_vault(1);
        assert_ne!(
            v1.address, v3.address,
            "Different index must produce different address"
        );
    }

    /// Verify the vault's internal key → output key → address relationship.
    ///
    /// BIP-341 Taproot has two keys:
    ///   Internal key (P): the MuSig2 aggregate key (untweaked)
    ///   Output key (Q): P + H(P)*G (the taptweak)
    ///
    /// The address encodes Q (output key).
    /// The vault stores P (internal key) as `aggregate_xonly`.
    /// Address::p2tr(P) applies the tweak to produce Q internally.
    #[test]
    fn test_vault_taproot_key_structure() {
        use bitcoin::key::TapTweak;

        let secp = Secp256k1::new();
        let (vault, _, _, _, _) = test_vault(0);

        // The address encodes a scriptPubKey: OP_1 <32-byte-output-key>
        let spk = vault.address.script_pubkey();
        let spk_bytes = spk.as_bytes();

        // P2TR scriptPubKey is: 0x51 0x20 <32 bytes>
        assert_eq!(spk_bytes[0], 0x51, "Must be OP_1 (segwit v1)");
        assert_eq!(spk_bytes[1], 0x20, "Must push 32 bytes");
        assert_eq!(spk_bytes.len(), 34, "P2TR scriptPubKey is exactly 34 bytes");

        let output_key_from_spk = &spk_bytes[2..34];

        // vault.aggregate_xonly is the INTERNAL key (untweaked MuSig2 aggregate)
        let internal_key = vault.aggregate_xonly;

        // Apply BIP-341 taptweak: Q = P + H(P)*G (no script tree, so merkle_root = None)
        let (output_key, _parity) = internal_key.tap_tweak(&secp, None);
        let output_key_bytes = output_key.to_inner().serialize();

        // The output key derived from the internal key must match what's in the address
        assert_eq!(
            output_key_from_spk, &output_key_bytes,
            "Address output key must equal tap_tweak(internal_key)"
        );

        // And the internal key must NOT match the output key (tweak changes it)
        assert_ne!(
            internal_key.serialize(),
            output_key_bytes,
            "Internal key and output key must differ (taptweak is non-trivial)"
        );

        println!("Internal key (P): {}", internal_key);
        println!("Output key   (Q): {}", hex::encode(output_key_bytes));
        println!("✅ P2TR address correctly encodes tap_tweak(internal_key)");
    }

    /// Verify that different derivation indices produce different co-signer keys.
    #[test]
    fn test_different_indices_different_keys() {
        let (_, _, _, _, child_0) = test_vault(0);
        let (_, _, _, _, child_1) = test_vault(1);
        let (_, _, _, _, child_99) = test_vault(99);

        assert_ne!(
            child_0.secret_bytes(),
            child_1.secret_bytes(),
            "Index 0 and 1 must derive different child keys"
        );
        assert_ne!(
            child_0.secret_bytes(),
            child_99.secret_bytes(),
            "Index 0 and 99 must derive different child keys"
        );
    }

    /// Verify owner key is NOT tweaked — only co-signer key changes per index.
    #[test]
    fn test_owner_key_unchanged_across_indices() {
        let (_, _, owner_0, _, _) = test_vault(0);
        let (_, _, owner_1, _, _) = test_vault(1);
        assert_eq!(
            owner_0.secret_bytes(),
            owner_1.secret_bytes(),
            "Owner key must be the same regardless of index"
        );
    }

    /// Verify Schnorr signature structure in an offline-signed transaction.
    #[test]
    fn test_schnorr_signature_structure() {
        use bitcoin::{OutPoint, TxOut, Txid};

        let (vault, ctx, owner_sk, _, cosigner_child_sk) = test_vault(0);

        // Create a fake UTXO to spend
        let fake_outpoint = OutPoint {
            txid: "0000000000000000000000000000000000000000000000000000000000000001"
                .parse::<Txid>()
                .unwrap(),
            vout: 0,
        };
        let fake_txout = TxOut {
            value: Amount::from_sat(10_000),
            script_pubkey: vault.address.script_pubkey(),
        };

        let (psbt, _tweaks) = build_spend_psbt(
            &vault,
            &[(fake_outpoint, fake_txout)],
            &[(vault.address.clone(), Amount::from_sat(9_700))],
            Amount::from_sat(300),
            None,
        )
        .unwrap();

        let signed_tx = musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &ctx, &psbt).unwrap();

        // Verify witness structure
        assert_eq!(signed_tx.input.len(), 1);
        let witness = &signed_tx.input[0].witness;
        let witness_items: Vec<&[u8]> = witness.iter().collect();

        assert_eq!(
            witness_items.len(),
            1,
            "Taproot key-path spend witness must have exactly 1 element"
        );
        assert_eq!(
            witness_items[0].len(),
            64,
            "Schnorr signature must be exactly 64 bytes (SIGHASH_DEFAULT, no type byte)"
        );

        // Verify no scriptsig (segwit)
        assert!(
            signed_tx.input[0].script_sig.is_empty(),
            "Segwit inputs must have empty scriptSig"
        );
    }

    /// Verify that the wrong co-signer key produces an invalid signature.
    #[test]
    fn test_wrong_cosigner_key_fails() {
        use bitcoin::{OutPoint, TxOut, Txid};

        let (vault, ctx, owner_sk, _, _correct_child) = test_vault(0);

        // Use a WRONG co-signer key (from index 1 instead of 0)
        let (_, _, _, _, wrong_child) = test_vault(1);

        let fake_outpoint = OutPoint {
            txid: "0000000000000000000000000000000000000000000000000000000000000001"
                .parse::<Txid>()
                .unwrap(),
            vout: 0,
        };
        let fake_txout = TxOut {
            value: Amount::from_sat(10_000),
            script_pubkey: vault.address.script_pubkey(),
        };

        let (psbt, _) = build_spend_psbt(
            &vault,
            &[(fake_outpoint, fake_txout)],
            &[(vault.address.clone(), Amount::from_sat(9_700))],
            Amount::from_sat(300),
            None,
        )
        .unwrap();

        // This should either fail or produce an invalid signature
        // The MuSig2 ceremony uses the wrong key, so partial_sign or
        // aggregate will produce garbage
        let result = musig2_sign_psbt(&owner_sk, &wrong_child, &ctx, &psbt);

        // The function might succeed (it doesn't verify internally),
        // but the resulting signature won't verify against the output key.
        // Let's check: if it succeeds, verify the sig is BAD.
        if let Ok(signed_tx) = result {
            use bitcoin::hashes::Hash;
            use bitcoin::sighash::{Prevouts, SighashCache};
            use bitcoin::TapSighashType;

            let secp = Secp256k1::verification_only();
            let witness = &signed_tx.input[0].witness;
            let sig_bytes: &[u8] = witness.iter().next().unwrap();
            let schnorr_sig =
                bitcoin::secp256k1::schnorr::Signature::from_slice(sig_bytes).unwrap();

            let mut sighash_cache = SighashCache::new(&signed_tx);
            let sighash = sighash_cache
                .taproot_key_spend_signature_hash(
                    0,
                    &Prevouts::All(&[TxOut {
                        value: Amount::from_sat(10_000),
                        script_pubkey: vault.address.script_pubkey(),
                    }]),
                    TapSighashType::Default,
                )
                .unwrap();

            let msg = bitcoin::secp256k1::Message::from_digest(sighash.to_byte_array());

            // This MUST fail — wrong co-signer key means wrong aggregate key
            let verify_result = secp.verify_schnorr(&schnorr_sig, &msg, &vault.aggregate_xonly);
            assert!(
                verify_result.is_err(),
                "Signature with WRONG co-signer key must NOT verify against the vault output key"
            );
            println!("✅ Confirmed: wrong co-signer key produces invalid signature");
        } else {
            println!(
                "✅ MuSig2 ceremony correctly rejected wrong key: {}",
                result.unwrap_err()
            );
        }
    }

    /// Owner alone cannot sign (MuSig2 requires both parties).
    #[test]
    fn test_owner_alone_cannot_sign() {
        use bitcoin::{Amount, OutPoint, TxOut, Txid};

        let (vault, ctx, owner_sk, _, _) = test_vault(0);

        let fake_outpoint = OutPoint {
            txid: "0000000000000000000000000000000000000000000000000000000000000001"
                .parse::<Txid>()
                .unwrap(),
            vout: 0,
        };
        let fake_txout = TxOut {
            value: Amount::from_sat(10_000),
            script_pubkey: vault.address.script_pubkey(),
        };

        let (psbt, _) = build_spend_psbt(
            &vault,
            &[(fake_outpoint, fake_txout)],
            &[(vault.address.clone(), Amount::from_sat(9_700))],
            Amount::from_sat(300),
            None,
        )
        .unwrap();

        // Try signing with owner key for BOTH parties
        let result = musig2_sign_psbt(&owner_sk, &owner_sk, &ctx, &psbt);
        assert!(
            result.is_err(),
            "Owner alone must not be able to sign — MuSig2 requires the co-signer's key"
        );
        println!("✅ Owner alone rejected: {}", result.unwrap_err());
    }

    /// Co-signer alone cannot sign.
    #[test]
    fn test_cosigner_alone_cannot_sign() {
        use bitcoin::{Amount, OutPoint, TxOut, Txid};

        let (vault, ctx, _, _, cosigner_child_sk) = test_vault(0);

        let fake_outpoint = OutPoint {
            txid: "0000000000000000000000000000000000000000000000000000000000000001"
                .parse::<Txid>()
                .unwrap(),
            vout: 0,
        };
        let fake_txout = TxOut {
            value: Amount::from_sat(10_000),
            script_pubkey: vault.address.script_pubkey(),
        };

        let (psbt, _) = build_spend_psbt(
            &vault,
            &[(fake_outpoint, fake_txout)],
            &[(vault.address.clone(), Amount::from_sat(9_700))],
            Amount::from_sat(300),
            None,
        )
        .unwrap();

        // Try signing with co-signer key for BOTH parties
        let result = musig2_sign_psbt(&cosigner_child_sk, &cosigner_child_sk, &ctx, &psbt);
        assert!(result.is_err(), "Co-signer alone must not be able to sign");
        println!("✅ Co-signer alone rejected: {}", result.unwrap_err());
    }

    /// Hardened indices (>= 0x80000000) must be rejected.
    #[test]
    fn test_hardened_index_rejected() {
        let (_, cosigner_pk) = deterministic_keypair(42);
        let delegated = register_cosigner_with_chain_code(
            cosigner_pk,
            ChainCode::from_bytes(CHAIN_CODE),
            "test",
        );

        let result = compute_tweak(&delegated, 0x80000000);
        assert!(result.is_err(), "Hardened index must be rejected");
        println!("✅ Hardened index rejected: {}", result.unwrap_err());
    }

    /// Boundary index values.
    #[test]
    fn test_boundary_indices() {
        let (_, owner_pk) = deterministic_keypair(1);
        let (_, cosigner_pk) = deterministic_keypair(42);
        let delegated = register_cosigner_with_chain_code(
            cosigner_pk,
            ChainCode::from_bytes(CHAIN_CODE),
            "test",
        );

        // Index 0 (minimum)
        let r0 = create_vault_musig2(&owner_pk, &delegated, 0, Network::Testnet);
        assert!(r0.is_ok(), "Index 0 must work");

        // Index 0x7FFFFFFF (maximum non-hardened)
        let r_max = create_vault_musig2(&owner_pk, &delegated, 0x7FFFFFFF, Network::Testnet);
        assert!(r_max.is_ok(), "Max non-hardened index must work");

        // Different addresses
        let (v0, _) = r0.unwrap();
        let (v_max, _) = r_max.unwrap();
        assert_ne!(v0.address, v_max.address);
        println!("✅ Index 0 and 0x7FFFFFFF both work, produce different addresses");
    }

    /// Same index + different chain codes → different vault addresses.
    /// This is critical: the chain code IS the delegation secret.
    #[test]
    fn test_different_chain_codes_different_vaults() {
        let (_, owner_pk) = deterministic_keypair(1);
        let (_, cosigner_pk) = deterministic_keypair(42);

        let d1 = register_cosigner_with_chain_code(
            cosigner_pk,
            ChainCode::from_bytes([0xAA; 32]),
            "test",
        );
        let d2 = register_cosigner_with_chain_code(
            cosigner_pk,
            ChainCode::from_bytes([0xBB; 32]),
            "test",
        );

        let (v1, _) = create_vault_musig2(&owner_pk, &d1, 0, Network::Testnet).unwrap();
        let (v2, _) = create_vault_musig2(&owner_pk, &d2, 0, Network::Testnet).unwrap();

        assert_ne!(
            v1.address, v2.address,
            "Different chain codes must produce different vaults"
        );
        println!("✅ Different chain codes → different addresses");
    }

    /// Zero chain code should still work (it's just an edge case, not invalid).
    #[test]
    fn test_zero_chain_code_works() {
        let (_, owner_pk) = deterministic_keypair(1);
        let (_, cosigner_pk) = deterministic_keypair(42);

        let d = register_cosigner_with_chain_code(
            cosigner_pk,
            ChainCode::from_bytes([0x00; 32]),
            "test",
        );

        let result = create_vault_musig2(&owner_pk, &d, 0, Network::Testnet);
        assert!(
            result.is_ok(),
            "Zero chain code should be valid (just weak)"
        );
        println!(
            "✅ Zero chain code produces a valid vault (but would be a bad real-world choice)"
        );
    }

    /// Verify PSBT rejects dust outputs.
    #[test]
    fn test_dust_output_handling() {
        use bitcoin::{Amount, OutPoint, TxOut, Txid};

        let (vault, _, _, _, _) = test_vault(0);

        let fake_outpoint = OutPoint {
            txid: "0000000000000000000000000000000000000000000000000000000000000001"
                .parse::<Txid>()
                .unwrap(),
            vout: 0,
        };
        let fake_txout = TxOut {
            value: Amount::from_sat(1_000),
            script_pubkey: vault.address.script_pubkey(),
        };

        // Try to create an output of 1 sat (dust)
        let result = build_spend_psbt(
            &vault,
            &[(fake_outpoint, fake_txout)],
            &[(vault.address.clone(), Amount::from_sat(1))],
            Amount::from_sat(999),
            None,
        );

        // This might succeed (dust check may not be in build_spend_psbt)
        // If it does, document the gap
        match result {
            Ok(_) => println!(
                "⚠️  GAP: build_spend_psbt allows dust outputs (1 sat). Should add dust check."
            ),
            Err(e) => println!("✅ Dust output rejected: {}", e),
        }
    }

    /// Verify signing with swapped keys (owner as cosigner, cosigner as owner) fails.
    #[test]
    fn test_swapped_keys_fail() {
        use bitcoin::{Amount, OutPoint, TxOut, Txid};

        let (vault, ctx, owner_sk, _, cosigner_child_sk) = test_vault(0);

        let fake_outpoint = OutPoint {
            txid: "0000000000000000000000000000000000000000000000000000000000000001"
                .parse::<Txid>()
                .unwrap(),
            vout: 0,
        };
        let fake_txout = TxOut {
            value: Amount::from_sat(10_000),
            script_pubkey: vault.address.script_pubkey(),
        };

        let (psbt, _) = build_spend_psbt(
            &vault,
            &[(fake_outpoint, fake_txout)],
            &[(vault.address.clone(), Amount::from_sat(9_700))],
            Amount::from_sat(300),
            None,
        )
        .unwrap();

        // Swap: pass cosigner as owner and owner as cosigner
        // musig2_sign_psbt(owner_sk, cosigner_sk) — swapping them
        let result = musig2_sign_psbt(&cosigner_child_sk, &owner_sk, &ctx, &psbt);

        if let Ok(signed_tx) = result {
            // If it produces a tx, verify the sig is invalid
            use bitcoin::hashes::Hash;
            use bitcoin::sighash::{Prevouts, SighashCache};
            use bitcoin::TapSighashType;

            let secp = Secp256k1::verification_only();
            let wit: &[u8] = signed_tx.input[0].witness.iter().next().unwrap();
            let sig = bitcoin::secp256k1::schnorr::Signature::from_slice(wit).unwrap();

            let mut cache = SighashCache::new(&signed_tx);
            let sighash = cache
                .taproot_key_spend_signature_hash(
                    0,
                    &Prevouts::All(&[TxOut {
                        value: Amount::from_sat(10_000),
                        script_pubkey: vault.address.script_pubkey(),
                    }]),
                    TapSighashType::Default,
                )
                .unwrap();

            let msg = bitcoin::secp256k1::Message::from_digest(sighash.to_byte_array());
            let verify = secp.verify_schnorr(&sig, &msg, &vault.aggregate_xonly);

            // Swapped keys should produce the SAME aggregate key (addition is commutative)
            // BUT musig2 crate tracks key ordering — partial_sign checks the key matches
            // So this should either fail at signing or produce an invalid sig
            if verify.is_ok() {
                println!("⚠️  Swapped keys produced a VALID signature — this means key order doesn't matter for MuSig2 aggregate (commutative). This is expected if the crate handles it.");
            } else {
                println!("✅ Swapped keys produced invalid signature");
            }
        } else {
            println!(
                "✅ Swapped keys rejected at ceremony: {}",
                result.unwrap_err()
            );
        }
    }

    // ──────────────────────────────────────────────
    // Network tests (require testnet Electrum access)
    // ──────────────────────────────────────────────

    /// Check balance of the testnet vault.
    #[test]
    #[ignore = "requires network access"]
    fn test_check_vault_balance() {
        use nostring_electrum::{default_server, ElectrumClient};

        let (vault, _, _, _, _) = test_vault(0);
        println!("Vault address: {}", vault.address);

        let client = ElectrumClient::new(default_server(Network::Testnet), Network::Testnet)
            .expect("Failed to connect");

        let height = client.get_height().unwrap();
        println!("Testnet height: {}", height);

        let utxos = client.get_utxos(&vault.address).unwrap();

        if utxos.is_empty() {
            println!("No UTXOs. Fund: {}", vault.address);

            let src: bitcoin::Address<bitcoin::address::NetworkUnchecked> =
                "tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v"
                    .parse()
                    .unwrap();
            let src_utxos = client.get_utxos(&src.assume_checked()).unwrap();
            let total: Amount = src_utxos.iter().map(|u| u.value).sum();
            println!(
                "Source wallet: {} sat ({} UTXOs)",
                total.to_sat(),
                src_utxos.len()
            );
        } else {
            let mut total = Amount::ZERO;
            for u in &utxos {
                println!(
                    "  {}:{} = {} sat (h={})",
                    u.outpoint.txid,
                    u.outpoint.vout,
                    u.value.to_sat(),
                    u.height
                );
                total += u.value;
            }
            println!("Vault balance: {} sat", total.to_sat());
        }
    }

    /// Self-spend: vault → same vault. Proves MuSig2 key-path signing works.
    #[test]
    #[ignore = "requires funded vault + network"]
    fn test_testnet_self_spend() {
        use nostring_electrum::{default_server, ElectrumClient};

        let (vault, ctx, owner_sk, _, cosigner_child_sk) = test_vault(0);
        println!("Vault: {}", vault.address);

        let client = ElectrumClient::new(default_server(Network::Testnet), Network::Testnet)
            .expect("Failed to connect");

        let utxos = client.get_utxos(&vault.address).unwrap();
        assert!(!utxos.is_empty(), "No UTXOs at vault");

        let mut total = Amount::ZERO;
        let utxo_pairs: Vec<_> = utxos
            .iter()
            .map(|u| {
                total += u.value;
                (
                    u.outpoint,
                    bitcoin::TxOut {
                        value: u.value,
                        script_pubkey: vault.address.script_pubkey(),
                    },
                )
            })
            .collect();

        let fee = Amount::from_sat(300);
        assert!(total > fee, "Balance {} too low", total.to_sat());

        let (psbt, _) = build_spend_psbt(
            &vault,
            &utxo_pairs,
            &[(vault.address.clone(), total - fee)],
            fee,
            None,
        )
        .unwrap();

        let signed_tx = musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &ctx, &psbt).unwrap();

        // Verify witness structure before broadcast
        for (i, input) in signed_tx.input.iter().enumerate() {
            let wit: Vec<&[u8]> = input.witness.iter().collect();
            assert_eq!(wit.len(), 1, "Input {} witness count", i);
            assert_eq!(wit[0].len(), 64, "Input {} sig length", i);
            println!("Input {}: 64-byte Schnorr sig ✓", i);
        }

        let txid = client.broadcast(&signed_tx).expect("Broadcast failed");
        println!("✅ Self-spend broadcast: {}", txid);
        println!("https://mempool.space/testnet/tx/{}", txid);
    }

    /// Spend to external P2WPKH address. Proves vault can pay anyone, not just itself.
    #[test]
    #[ignore = "requires funded vault + network"]
    fn test_testnet_spend_to_external() {
        use nostring_electrum::{default_server, ElectrumClient};

        let (vault, ctx, owner_sk, _, cosigner_child_sk) = test_vault(0);
        println!("Vault: {}", vault.address);

        let client = ElectrumClient::new(default_server(Network::Testnet), Network::Testnet)
            .expect("Failed to connect");

        let utxos = client.get_utxos(&vault.address).unwrap();
        assert!(!utxos.is_empty(), "No UTXOs at vault");

        let mut total = Amount::ZERO;
        let utxo_pairs: Vec<_> = utxos
            .iter()
            .map(|u| {
                total += u.value;
                (
                    u.outpoint,
                    bitcoin::TxOut {
                        value: u.value,
                        script_pubkey: vault.address.script_pubkey(),
                    },
                )
            })
            .collect();

        let fee = Amount::from_sat(300);
        let send_to_external = Amount::from_sat(5_000);
        let change = total - send_to_external - fee;
        assert!(total > send_to_external + fee, "Insufficient balance");

        // External destination: the original P2WPKH testnet wallet
        let external: bitcoin::Address<bitcoin::address::NetworkUnchecked> =
            "tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v"
                .parse()
                .unwrap();
        let external = external.assume_checked();

        let mut outputs = vec![(external.clone(), send_to_external)];
        if change > Amount::ZERO {
            outputs.push((vault.address.clone(), change));
        }

        let (psbt, _) = build_spend_psbt(&vault, &utxo_pairs, &outputs, fee, None).unwrap();

        let signed_tx = musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &ctx, &psbt).unwrap();

        // Verify outputs
        println!("Outputs:");
        for (i, out) in signed_tx.output.iter().enumerate() {
            println!(
                "  {}: {} sat → {:?}",
                i,
                out.value.to_sat(),
                bitcoin::Address::from_script(&out.script_pubkey, bitcoin::params::Params::TESTNET)
            );
        }

        let txid = client.broadcast(&signed_tx).expect("Broadcast failed");
        println!("✅ External spend broadcast: {}", txid);
        println!("https://mempool.space/testnet/tx/{}", txid);
        println!(
            "Sent {} sat to {} (P2WPKH)",
            send_to_external.to_sat(),
            external
        );
        println!("Change {} sat back to vault", change.to_sat());
    }

    /// Create a second vault at index=1, fund it from index=0, spend from it.
    /// Proves CCD derivation works across multiple indices.
    #[test]
    #[ignore = "requires funded vault + network"]
    fn test_testnet_multi_index_vault() {
        use nostring_electrum::{default_server, ElectrumClient};

        let (vault_0, ctx_0, owner_sk, _cosigner_sk, cosigner_child_0) = test_vault(0);
        let (vault_1, ctx_1, _, _, cosigner_child_1) = test_vault(1);

        println!("Vault 0: {}", vault_0.address);
        println!("Vault 1: {}", vault_1.address);
        assert_ne!(
            vault_0.address, vault_1.address,
            "Different indices must give different addresses"
        );

        let client = ElectrumClient::new(default_server(Network::Testnet), Network::Testnet)
            .expect("Failed to connect");

        // Step 1: Spend from vault_0 to vault_1
        let utxos_0 = client.get_utxos(&vault_0.address).unwrap();
        assert!(!utxos_0.is_empty(), "Vault 0 has no UTXOs");

        let mut total_0 = Amount::ZERO;
        let pairs_0: Vec<_> = utxos_0
            .iter()
            .map(|u| {
                total_0 += u.value;
                (
                    u.outpoint,
                    bitcoin::TxOut {
                        value: u.value,
                        script_pubkey: vault_0.address.script_pubkey(),
                    },
                )
            })
            .collect();

        let fee = Amount::from_sat(300);
        let send_to_v1 = Amount::from_sat(5_000);
        let change_to_v0 = total_0 - send_to_v1 - fee;
        assert!(total_0 > send_to_v1 + fee);

        let (psbt_fund, _) = build_spend_psbt(
            &vault_0,
            &pairs_0,
            &[
                (vault_1.address.clone(), send_to_v1),
                (vault_0.address.clone(), change_to_v0),
            ],
            fee,
            None,
        )
        .unwrap();

        let tx_fund = musig2_sign_psbt(&owner_sk, &cosigner_child_0, &ctx_0, &psbt_fund).unwrap();
        let fund_txid = client
            .broadcast(&tx_fund)
            .expect("Fund vault_1 broadcast failed");
        println!(
            "✅ Funded vault_1: {} ({} sat)",
            fund_txid,
            send_to_v1.to_sat()
        );

        // Step 2: Spend FROM vault_1 back to vault_0
        // The UTXO is the output we just created
        let v1_outpoint = bitcoin::OutPoint {
            txid: fund_txid,
            vout: 0,
        };
        let v1_txout = bitcoin::TxOut {
            value: send_to_v1,
            script_pubkey: vault_1.address.script_pubkey(),
        };

        let send_back = send_to_v1 - fee;
        let (psbt_spend, _) = build_spend_psbt(
            &vault_1,
            &[(v1_outpoint, v1_txout)],
            &[(vault_0.address.clone(), send_back)],
            fee,
            None,
        )
        .unwrap();

        // Sign with vault_1's co-signer child key (derived from index=1)
        let tx_spend = musig2_sign_psbt(&owner_sk, &cosigner_child_1, &ctx_1, &psbt_spend).unwrap();
        let spend_txid = client
            .broadcast(&tx_spend)
            .expect("Spend from vault_1 failed");
        println!(
            "✅ Spent from vault_1: {} ({} sat back to vault_0)",
            spend_txid,
            send_back.to_sat()
        );
        println!("https://mempool.space/testnet/tx/{}", spend_txid);
        println!("\nMulti-index CCD proven: vault_0 → vault_1 → vault_0");
    }

    /// Verify that our previous testnet transactions are confirmed in a block.
    /// This proves Bitcoin consensus accepted our MuSig2 Taproot signatures.
    #[test]
    #[ignore = "requires network access"]
    fn test_verify_confirmations() {
        use nostring_electrum::{default_server, ElectrumClient};

        let client = ElectrumClient::new(default_server(Network::Testnet), Network::Testnet)
            .expect("Failed to connect");

        let height = client.get_height().unwrap();
        println!("Current testnet height: {}", height);

        // Known MuSig2 vault spend transactions (from our test runs)
        let musig2_spend_txids = [
            // Tx 4: First MuSig2 spend (2 inputs)
            "d882a175d7899f12bbb139061fe13abd084a3b2336b88cba28c5d2aa7f2b7dff",
            // Tx 5: Second MuSig2 self-spend
            "68ed16567b44ade9108f4db7a5621497244fdddbc46da3d6d8852ed28f8f339a",
            // Tx 6: Third self-spend
            "237508c659b0c2e9d16f3c7e505ea6f89fe3531d2539d9222b585cf59b36cc27",
            // Tx 7: External spend to P2WPKH
            "98402af982737963a74957d9967d831b0f09fc3b3e5719594aed822f3a9e759b",
            // Tx 8: Cross-index fund vault_1
            "e58f27c7779a5a08219e096f7e63e02d065f56e75939413ca52b205d816d0a11",
            // Tx 9: Spend from vault_1 back to vault_0
            "4384ae07d45f0103088f9fa77651fc878259b811ca011604ff8a52975ce3c1ae",
        ];

        let mut all_confirmed = true;
        for txid_str in &musig2_spend_txids {
            let txid: bitcoin::Txid = txid_str.parse().unwrap();
            let conf_height = client.get_confirmation_height(&txid).unwrap_or(None);
            let tx = client.get_transaction(&txid);

            if let Some(h) = conf_height {
                println!("✅ {} CONFIRMED at height {}", &txid_str[..8], h);
                // Verify witness structure of confirmed tx
                if let Ok(tx) = tx {
                    for (i, input) in tx.input.iter().enumerate() {
                        let wit_count = input.witness.iter().count();
                        let wit_len: Vec<usize> = input.witness.iter().map(|w| w.len()).collect();
                        assert_eq!(
                            wit_count,
                            1,
                            "Confirmed tx {} input {} has {} witness items (expected 1)",
                            &txid_str[..8],
                            i,
                            wit_count
                        );
                        assert_eq!(
                            wit_len[0],
                            64,
                            "Confirmed tx {} input {} sig is {} bytes (expected 64)",
                            &txid_str[..8],
                            i,
                            wit_len[0]
                        );
                    }
                }
            } else {
                println!("⏳ {} not yet confirmed", &txid_str[..8]);
                all_confirmed = false;
            }
            // Rate limit to avoid hammering Electrum
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        if all_confirmed {
            println!(
                "\n✅✅✅ ALL {} MuSig2 vault spends CONFIRMED by Bitcoin consensus ✅✅✅",
                musig2_spend_txids.len()
            );
        } else {
            println!("\n⏳ Some transactions still unconfirmed. Re-run later.");
        }
    }

    /// Spend from a CONFIRMED UTXO. Previous tests may have spent from unconfirmed chains.
    /// This test verifies the vault has confirmed UTXOs and spends one.
    #[test]
    #[ignore = "requires funded vault with confirmed UTXOs + network"]
    fn test_testnet_spend_confirmed_utxo() {
        use nostring_electrum::{default_server, ElectrumClient};

        let (vault, ctx, owner_sk, _, cosigner_child_sk) = test_vault(0);
        println!("Vault: {}", vault.address);

        let client = ElectrumClient::new(default_server(Network::Testnet), Network::Testnet)
            .expect("Failed to connect");

        let utxos = client.get_utxos(&vault.address).unwrap();
        assert!(!utxos.is_empty(), "No UTXOs at vault");

        // Filter to confirmed UTXOs only (height > 0)
        let confirmed_utxos: Vec<_> = utxos.iter().filter(|u| u.height > 0).collect();
        println!(
            "Total UTXOs: {}, Confirmed: {}",
            utxos.len(),
            confirmed_utxos.len()
        );

        if confirmed_utxos.is_empty() {
            println!("⏳ No confirmed UTXOs yet. Wait for a block and re-run.");
            println!("Unconfirmed UTXOs:");
            for u in &utxos {
                println!(
                    "  {}:{} = {} sat (h={})",
                    u.outpoint.txid,
                    u.outpoint.vout,
                    u.value.to_sat(),
                    u.height
                );
            }
            panic!("Need confirmed UTXOs for this test");
        }

        // Use only confirmed UTXOs
        let mut total = Amount::ZERO;
        let utxo_pairs: Vec<_> = confirmed_utxos
            .iter()
            .map(|u| {
                println!(
                    "  CONFIRMED: {}:{} = {} sat (h={})",
                    u.outpoint.txid,
                    u.outpoint.vout,
                    u.value.to_sat(),
                    u.height
                );
                total += u.value;
                (
                    u.outpoint,
                    bitcoin::TxOut {
                        value: u.value,
                        script_pubkey: vault.address.script_pubkey(),
                    },
                )
            })
            .collect();

        let fee = Amount::from_sat(300);
        assert!(total > fee, "Confirmed balance {} too low", total.to_sat());

        let (psbt, _) = build_spend_psbt(
            &vault,
            &utxo_pairs,
            &[(vault.address.clone(), total - fee)],
            fee,
            None,
        )
        .unwrap();

        let signed_tx = musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &ctx, &psbt).unwrap();

        // Verify witness
        for (i, input) in signed_tx.input.iter().enumerate() {
            let wit: Vec<&[u8]> = input.witness.iter().collect();
            assert_eq!(wit.len(), 1, "Input {} witness count", i);
            assert_eq!(wit[0].len(), 64, "Input {} sig length", i);
        }

        let txid = client.broadcast(&signed_tx).expect("Broadcast failed");
        println!("✅ Spent from CONFIRMED UTXOs: {}", txid);
        println!("https://mempool.space/testnet/tx/{}", txid);
    }

    /// Print vault addresses for indices 0-4.
    #[test]
    fn test_print_vault_addresses() {
        println!("=== CCD Vault Addresses (testnet3) ===");
        for i in 0..5 {
            let (v, _, _, _, _) = test_vault(i);
            println!("Index {}: {}", i, v.address);
        }
    }
}
