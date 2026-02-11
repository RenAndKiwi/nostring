//! CCD Vault End-to-End Demo
//!
//! Full Chain Code Delegation lifecycle on Bitcoin testnet3:
//!   1. Owner derives keys from mnemonic (nostring-core)
//!   2. Co-signer derives keys from separate mnemonic
//!   3. Owner registers co-signer + generates chain code (nostring-ccd)
//!   4. Owner creates MuSig2 Taproot vault (nostring-ccd)
//!   5. Query vault balance (nostring-electrum)
//!   6. Build spend PSBT (nostring-ccd)
//!   7. MuSig2 signing ceremony (nostring-ccd)
//!   8. Broadcast signed transaction (nostring-electrum)
//!
//! Run with:
//!   cargo test -p nostring-e2e --test ccd_vault_demo -- --ignored --nocapture
//!
//! This test uses hardcoded test mnemonics. NOT for production use.

use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::{Amount, Network};

// ─── Test Mnemonics (NOT FOR PRODUCTION) ───
// Owner: the existing test wallet used throughout NoString E2E tests
const OWNER_MNEMONIC: &str =
    "wrap bubble bunker win flat south life shed twelve payment super taste";
// Co-signer: a separate 12-word mnemonic for the co-signing party
const COSIGNER_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

/// Derive the owner's signing key from the test mnemonic.
/// Returns (secret_key, public_key) at BIP-84 testnet path m/84'/1'/0'/0/0.
fn derive_owner_keys() -> (SecretKey, bitcoin::secp256k1::PublicKey) {
    let secp = Secp256k1::new();
    let mnemonic = nostring_core::seed::parse_mnemonic(OWNER_MNEMONIC).unwrap();
    let seed = nostring_core::seed::derive_seed(&mnemonic, "");
    let master =
        nostring_core::keys::derive_bitcoin_master_for_network(&seed, Network::Testnet).unwrap();
    let path: bitcoin::bip32::DerivationPath = "m/0/0".parse().unwrap();
    let derived = master.derive_priv(&secp, &path).unwrap();
    let sk = derived.private_key;
    let pk = sk.public_key(&secp);
    (sk, pk)
}

/// Derive the co-signer's key from their own mnemonic.
fn derive_cosigner_keys() -> (SecretKey, bitcoin::secp256k1::PublicKey) {
    let secp = Secp256k1::new();
    let mnemonic = nostring_core::seed::parse_mnemonic(COSIGNER_MNEMONIC).unwrap();
    let seed = nostring_core::seed::derive_seed(&mnemonic, "");
    let master =
        nostring_core::keys::derive_bitcoin_master_for_network(&seed, Network::Testnet).unwrap();
    let path: bitcoin::bip32::DerivationPath = "m/0/0".parse().unwrap();
    let derived = master.derive_priv(&secp, &path).unwrap();
    let sk = derived.private_key;
    let pk = sk.public_key(&secp);
    (sk, pk)
}

/// Derive a deterministic chain code from the owner's seed.
fn derive_chain_code() -> nostring_ccd::types::ChainCode {
    let mnemonic = nostring_core::seed::parse_mnemonic(OWNER_MNEMONIC).unwrap();
    let seed = nostring_core::seed::derive_seed(&mnemonic, "");
    nostring_ccd::derive_chain_code_from_seed(&seed)
}

/// Print vault info without requiring network access.
#[test]
fn test_ccd_vault_info() {
    println!("\n=== CCD Vault Info ===\n");

    let (owner_sk, owner_pk) = derive_owner_keys();
    let (cosigner_sk, cosigner_pk) = derive_cosigner_keys();
    let chain_code = derive_chain_code();

    println!("Owner pubkey:     {}", owner_pk);
    println!("Co-signer pubkey: {}", cosigner_pk);

    // Register co-signer
    let delegated = nostring_ccd::register_cosigner_with_chain_code(
        cosigner_pk,
        chain_code,
        "e2e-demo-cosigner",
    );

    // Create vault at index 0
    let (vault, _ctx) =
        nostring_ccd::vault::create_vault_musig2(&owner_pk, &delegated, 0, Network::Testnet)
            .unwrap();

    println!("Vault address:    {}", vault.address);
    println!("Internal key:     {}", vault.aggregate_xonly);
    println!(
        "Derived co-signer pubkey: {}",
        vault.cosigner_derived_pubkey
    );
    println!("Network:          testnet3");
    println!();

    // Show addresses for first 3 indices
    for i in 0..3 {
        let chain_code_i = derive_chain_code();
        let delegated_i =
            nostring_ccd::register_cosigner_with_chain_code(cosigner_pk, chain_code_i, "e2e-demo");
        let (v, _) =
            nostring_ccd::vault::create_vault_musig2(&owner_pk, &delegated_i, i, Network::Testnet)
                .unwrap();
        println!("  Index {}: {}", i, v.address);
    }

    println!("\n=== Keys derived from mnemonics via nostring-core ===");
    println!(
        "Owner mnemonic:     {} (12 words)",
        OWNER_MNEMONIC.split_whitespace().count()
    );
    println!(
        "Co-signer mnemonic: {} (12 words)",
        COSIGNER_MNEMONIC.split_whitespace().count()
    );
    println!("Chain code: derived from owner seed via HMAC-SHA512");
}

/// Full CCD vault lifecycle on testnet3.
///
/// Prerequisites:
///   - Vault must be funded (send testnet coins to the address printed by test_ccd_vault_info)
///   - Network access to Electrum server
#[test]
#[ignore = "requires network access + funded vault"]
fn test_ccd_vault_full_lifecycle() {
    use bitcoin::key::TapTweak;
    use nostring_electrum::{default_server, ElectrumClient};

    println!("\n=== CCD Vault Full Lifecycle Demo ===\n");

    // ─── Step 1: Derive owner keys from mnemonic ───
    let (owner_sk, owner_pk) = derive_owner_keys();
    println!("Step 1: Owner keys derived from mnemonic");
    println!("  Pubkey: {}", owner_pk);

    // ─── Step 2: Derive co-signer keys from separate mnemonic ───
    let (cosigner_sk, cosigner_pk) = derive_cosigner_keys();
    println!("Step 2: Co-signer keys derived from separate mnemonic");
    println!("  Pubkey: {}", cosigner_pk);

    // ─── Step 3: Register co-signer with deterministic chain code ───
    let chain_code = derive_chain_code();
    let delegated = nostring_ccd::register_cosigner_with_chain_code(
        cosigner_pk,
        chain_code,
        "e2e-demo-cosigner",
    );
    println!("Step 3: Co-signer registered with chain code");

    // ─── Step 4: Create MuSig2 Taproot vault ───
    let (vault, key_agg_ctx) =
        nostring_ccd::vault::create_vault_musig2(&owner_pk, &delegated, 0, Network::Testnet)
            .unwrap();

    let secp = Secp256k1::new();
    let (output_key, _) = vault.aggregate_xonly.tap_tweak(&secp, None);
    println!("Step 4: MuSig2 Taproot vault created");
    println!("  Address:      {}", vault.address);
    println!("  Internal key: {}", vault.aggregate_xonly);
    println!("  Output key:   {}", output_key.to_x_only_public_key());

    // ─── Step 5: Query vault balance via Electrum ───
    let client = ElectrumClient::new(default_server(Network::Testnet), Network::Testnet)
        .expect("Failed to connect to testnet Electrum");

    let height = client.get_height().unwrap();
    println!("Step 5: Connected to Electrum (testnet height: {})", height);

    let utxos = client.get_utxos(&vault.address).unwrap();
    if utxos.is_empty() {
        println!("  ⚠️  No UTXOs at vault address.");
        println!("  Fund it: {}", vault.address);
        panic!(
            "Vault not funded. Send testnet coins to {} first.",
            vault.address
        );
    }

    let mut total = Amount::ZERO;
    println!("  UTXOs:");
    for u in &utxos {
        println!(
            "    {}:{} = {} sat (confirmed: {})",
            u.outpoint.txid,
            u.outpoint.vout,
            u.value.to_sat(),
            if u.height > 0 { "yes" } else { "no" }
        );
        total += u.value;
    }
    println!("  Total balance: {} sat", total.to_sat());

    // ─── Step 6: Build spend PSBT ───
    let fee = Amount::from_sat(300);
    assert!(total > fee, "Balance too low for fee");

    let utxo_pairs: Vec<_> = utxos
        .iter()
        .map(|u| {
            (
                u.outpoint,
                bitcoin::TxOut {
                    value: u.value,
                    script_pubkey: vault.address.script_pubkey(),
                },
            )
        })
        .collect();

    // Self-spend (send back to same vault minus fee)
    let (psbt, tweaks) = nostring_ccd::vault::build_spend_psbt(
        &vault,
        &utxo_pairs,
        &[(vault.address.clone(), total - fee)],
        fee,
        None,
    )
    .unwrap();

    println!("Step 6: PSBT built");
    println!("  Inputs:  {}", psbt.inputs.len());
    println!("  Outputs: {}", psbt.unsigned_tx.output.len());
    println!("  Fee:     {} sat", fee.to_sat());

    // Verify tap_internal_key is set
    for (i, input) in psbt.inputs.iter().enumerate() {
        assert_eq!(
            input.tap_internal_key,
            Some(vault.aggregate_xonly),
            "Input {} missing tap_internal_key",
            i
        );
    }

    // ─── Step 7: MuSig2 signing ceremony ───
    // In production, this would be split across two machines with Nostr transport.
    // Here we simulate both parties locally.

    // CCD: derive co-signer's child key for this vault
    let cosigner_child_sk = nostring_ccd::apply_tweak(&cosigner_sk, &tweaks[0].tweak).unwrap();

    println!("Step 7: MuSig2 signing ceremony");
    println!("  CCD tweak applied to co-signer key");

    let signed_tx =
        nostring_ccd::vault::musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &key_agg_ctx, &psbt)
            .unwrap();

    // Verify witness structure
    for (i, input) in signed_tx.input.iter().enumerate() {
        let wit: Vec<&[u8]> = input.witness.iter().collect();
        assert_eq!(wit.len(), 1, "Input {} must have 1 witness element", i);
        assert_eq!(wit[0].len(), 64, "Input {} sig must be 64 bytes", i);
    }
    println!(
        "  {} inputs signed with 64-byte Schnorr signatures",
        signed_tx.input.len()
    );

    // ─── Step 8: Broadcast ───
    let tx_bytes = bitcoin::consensus::serialize(&signed_tx);
    println!("Step 8: Broadcasting ({} bytes)", tx_bytes.len());

    let txid = client.broadcast(&signed_tx).expect("Broadcast failed");

    println!("  ✅ BROADCAST SUCCESSFUL");
    println!("  Txid: {}", txid);
    println!("  View: https://mempool.space/testnet/tx/{}", txid);

    // ─── Summary ───
    println!("\n=== CCD Vault Lifecycle Complete ===");
    println!("  Mnemonic → BIP-84 keys → CCD registration → MuSig2 Taproot vault");
    println!("  → UTXO discovery → PSBT construction → MuSig2 ceremony");
    println!("  → 64-byte Schnorr signature → broadcast accepted by testnet3");
    println!("  All crates working together: nostring-core + nostring-ccd + nostring-electrum");
}
