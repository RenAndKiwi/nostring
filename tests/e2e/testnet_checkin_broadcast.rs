//! Real Testnet Check-in Broadcast Test
//!
//! This test performs a REAL transaction broadcast on Bitcoin testnet3.
//! It creates an inheritance policy, builds a funding transaction from
//! the owner's P2WPKH wallet to a P2WSH inheritance address, signs it,
//! and broadcasts it to the network.
//!
//! Run with:
//!   cargo test -p nostring-e2e --test testnet_checkin_broadcast -- --ignored --nocapture

use bitcoin::{
    absolute::LockTime,
    bip32::{DerivationPath, Xpub},
    hashes::Hash,
    sighash::{EcdsaSighashType, SighashCache},
    transaction::Version,
    Address, Amount, CompressedPublicKey, Network, OutPoint, ScriptBuf, Sequence, Transaction,
    TxIn, TxOut, Txid, Witness,
};
use miniscript::descriptor::DescriptorPublicKey;
use nostring_electrum::ElectrumClient;
use nostring_inherit::policy::{InheritancePolicy, Timelock};
use std::str::FromStr;
use std::sync::Once;

static INIT_CRYPTO: Once = Once::new();

fn init_rustls() {
    INIT_CRYPTO.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

const TEST_MNEMONIC: &str =
    "wrap bubble bunker win flat south life shed twelve payment super taste";
const EXPECTED_ADDRESS: &str = "tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v";
const UTXO_TXID: &str = "53036e1468b1c1c60ea8f40a3492515c0d91dbd056dcbbc827840d321810bff8";
const UTXO_VOUT: u32 = 1;
const UTXO_VALUE_SATS: u64 = 347_970;
const FEE_SATS: u64 = 500; // ~4 sat/vB, plenty for testnet

/// Real testnet check-in: fund an inheritance P2WSH address
///
/// This is the first step of the inheritance flow:
/// 1. Owner sends funds from P2WPKH wallet → P2WSH inheritance address
/// 2. The inheritance UTXO now has a fresh timelock (CSV resets on creation)
/// 3. Future check-ins would spend from P2WSH → P2WSH (same address)
#[test]
#[ignore = "REAL TESTNET BROADCAST - run manually with --nocapture"]
fn test_real_testnet_checkin_broadcast() {
    init_rustls();
    println!("\n============================================================");
    println!("  REAL TESTNET CHECK-IN BROADCAST");
    println!("============================================================\n");

    let secp = bitcoin::secp256k1::Secp256k1::new();

    // ========================================================================
    // Step 1: Derive owner keys from testnet mnemonic (BIP-84, testnet)
    // ========================================================================
    println!("Step 1: Deriving owner keys from mnemonic...");

    let mnemonic = nostring_core::seed::parse_mnemonic(TEST_MNEMONIC).unwrap();
    let seed = nostring_core::seed::derive_seed(&mnemonic, "");

    // Master at m/84'/1'/0' (BIP-84 testnet)
    let owner_master =
        nostring_core::keys::derive_bitcoin_master_for_network(&seed, Network::Testnet).unwrap();
    let owner_xpub = Xpub::from_priv(&secp, &owner_master);
    let owner_fingerprint = owner_xpub.fingerprint();

    // Child at m/84'/1'/0'/0/0 (first receive address)
    let child_path: DerivationPath = "m/0/0".parse().unwrap();
    let child_priv = owner_master.derive_priv(&secp, &child_path).unwrap();
    let child_pubkey = child_priv.private_key.public_key(&secp);
    let compressed_pubkey = CompressedPublicKey(child_pubkey);

    let owner_address = Address::p2wpkh(&compressed_pubkey, Network::Testnet);
    assert_eq!(
        owner_address.to_string(),
        EXPECTED_ADDRESS,
        "Address derivation mismatch!"
    );

    println!("  ✓ Owner address: {}", owner_address);
    println!("  ✓ Owner fingerprint: {}", owner_fingerprint);
    println!("  ✓ Owner xpub: {}", owner_xpub);

    // ========================================================================
    // Step 2: Generate fresh heir keypair
    // ========================================================================
    println!("\nStep 2: Generating fresh heir keypair...");

    let heir_mnemonic = nostring_core::seed::generate_mnemonic_24().unwrap();
    let heir_seed = nostring_core::seed::derive_seed(&heir_mnemonic, "");
    let heir_master =
        nostring_core::keys::derive_bitcoin_master_for_network(&heir_seed, Network::Testnet)
            .unwrap();
    let heir_xpub = Xpub::from_priv(&secp, &heir_master);
    let heir_fingerprint = heir_xpub.fingerprint();

    println!("  ✓ Heir fingerprint: {}", heir_fingerprint);
    println!("  ✓ Heir xpub: {}", heir_xpub);
    println!("  (Heir mnemonic saved for test records)");

    // ========================================================================
    // Step 3: Create inheritance policy (6-month timelock)
    // ========================================================================
    println!("\nStep 3: Creating inheritance policy...");

    let owner_desc_key = DescriptorPublicKey::from_str(&format!(
        "[{}/84'/1'/0']{}/<0;1>/*",
        owner_fingerprint, owner_xpub
    ))
    .expect("valid owner descriptor key");

    let heir_desc_key = DescriptorPublicKey::from_str(&format!(
        "[{}/84'/1'/0']{}/<0;1>/*",
        heir_fingerprint, heir_xpub
    ))
    .expect("valid heir descriptor key");

    let timelock = Timelock::six_months();
    let policy = InheritancePolicy::simple(owner_desc_key, heir_desc_key, timelock).unwrap();
    let descriptor = policy.to_wsh_descriptor().unwrap();

    println!(
        "  ✓ Policy: owner immediate + heir after {} blocks (~6 months)",
        timelock.blocks()
    );
    println!("  ✓ Descriptor: {}", descriptor);

    // Derive the inheritance P2WSH script_pubkey at index 0
    let single_descs = descriptor.clone().into_single_descriptors().unwrap();
    let receive_desc = &single_descs[0];
    let derived = receive_desc.derived_descriptor(&secp, 0).unwrap();
    let inheritance_spk = derived.script_pubkey();

    assert!(inheritance_spk.is_p2wsh(), "Must be P2WSH");
    println!(
        "  ✓ Inheritance scriptPubKey: {}",
        inheritance_spk.to_hex_string()
    );

    // ========================================================================
    // Step 4: Connect to testnet and verify UTXO
    // ========================================================================
    println!("\nStep 4: Connecting to testnet Electrum...");

    let client = ElectrumClient::new("ssl://blockstream.info:993", Network::Testnet)
        .expect("Failed to connect to testnet Electrum");

    let height = client.get_height().expect("Failed to get block height");
    println!("  ✓ Connected. Current testnet height: {}", height);

    let owner_spk = owner_address.script_pubkey();
    let balance = client
        .get_balance(owner_spk.as_script())
        .expect("Failed to get balance");
    println!("  ✓ Owner balance: {} sats", balance.to_sat());

    // Verify the specific UTXO exists
    let utxos = client
        .get_utxos_for_script(owner_spk.as_script())
        .expect("Failed to get UTXOs");
    println!("  ✓ UTXOs found: {}", utxos.len());
    for utxo in &utxos {
        println!(
            "    - {}:{} = {} sats (height {})",
            utxo.outpoint.txid,
            utxo.outpoint.vout,
            utxo.value.to_sat(),
            utxo.height
        );
    }

    let expected_txid = Txid::from_str(UTXO_TXID).unwrap();
    let our_utxo = utxos
        .iter()
        .find(|u| u.outpoint.txid == expected_txid && u.outpoint.vout == UTXO_VOUT);

    if our_utxo.is_none() {
        println!("\n  ⚠ Expected UTXO {}:{} not found!", UTXO_TXID, UTXO_VOUT);
        println!("  It may have been spent already. Checking available UTXOs...");

        if utxos.is_empty() {
            panic!("No UTXOs available at owner address. Cannot proceed.");
        }

        println!("  Using first available UTXO instead.");
    }

    // Use the expected UTXO or first available
    let (spend_txid, spend_vout, spend_value) = match our_utxo {
        Some(u) => (u.outpoint.txid, u.outpoint.vout, u.value.to_sat()),
        None => {
            let u = &utxos[0];
            (u.outpoint.txid, u.outpoint.vout, u.value.to_sat())
        }
    };

    println!(
        "\n  Using UTXO: {}:{} ({} sats)",
        spend_txid, spend_vout, spend_value
    );
    assert!(
        spend_value > FEE_SATS,
        "UTXO value {} too small for fee {}",
        spend_value,
        FEE_SATS
    );

    // ========================================================================
    // Step 5: Build the check-in transaction (P2WPKH → P2WSH)
    // ========================================================================
    println!("\nStep 5: Building check-in transaction...");

    let output_value = Amount::from_sat(spend_value - FEE_SATS);

    let unsigned_tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: spend_txid,
                vout: spend_vout,
            },
            script_sig: ScriptBuf::new(), // Empty for SegWit
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(),
        }],
        output: vec![TxOut {
            value: output_value,
            script_pubkey: inheritance_spk.clone(),
        }],
    };

    println!(
        "  ✓ Input:  {}:{} ({} sats, P2WPKH)",
        spend_txid, spend_vout, spend_value
    );
    println!(
        "  ✓ Output: {} sats → P2WSH inheritance address",
        output_value.to_sat()
    );
    println!("  ✓ Fee:    {} sats", FEE_SATS);

    // ========================================================================
    // Step 6: Sign the P2WPKH input with owner's private key
    // ========================================================================
    println!("\nStep 6: Signing transaction...");

    // Compute BIP-143 sighash for P2WPKH input
    let sighash = {
        let mut cache = SighashCache::new(&unsigned_tx);
        cache
            .p2wpkh_signature_hash(
                0, // input index
                &owner_spk,
                Amount::from_sat(spend_value),
                EcdsaSighashType::All,
            )
            .expect("Failed to compute sighash")
    };

    let msg = bitcoin::secp256k1::Message::from_digest(sighash.to_byte_array());
    let sig = secp.sign_ecdsa(&msg, &child_priv.private_key);

    // Verify signature before broadcasting (safety check)
    secp.verify_ecdsa(&msg, &sig, &child_pubkey)
        .expect("Signature verification failed! Aborting.");
    println!("  ✓ ECDSA signature created and verified locally");

    // Build witness: [signature || sighash_type, compressed_pubkey]
    let mut sig_bytes = sig.serialize_der().to_vec();
    sig_bytes.push(EcdsaSighashType::All.to_u32() as u8);

    let mut signed_tx = unsigned_tx;
    let mut witness = Witness::new();
    witness.push(&sig_bytes);
    witness.push(child_pubkey.serialize());
    signed_tx.input[0].witness = witness;

    let computed_txid = signed_tx.compute_txid();
    println!("  ✓ Transaction signed. Computed txid: {}", computed_txid);

    // Print raw hex for verification
    let raw_hex = bitcoin::consensus::encode::serialize_hex(&signed_tx);
    println!("  ✓ Raw tx size: {} bytes", raw_hex.len() / 2);
    println!("  ✓ Raw tx hex: {}", raw_hex);

    // ========================================================================
    // Step 7: Broadcast to testnet via Electrum
    // ========================================================================
    println!("\nStep 7: Broadcasting to testnet...");

    let broadcast_txid = client
        .broadcast(&signed_tx)
        .expect("BROADCAST FAILED — transaction was rejected by the network");

    assert_eq!(broadcast_txid, computed_txid);

    println!("  ✓ BROADCAST SUCCESS!");
    println!("  ✓ Txid: {}", broadcast_txid);
    println!(
        "  ✓ Explorer: https://mempool.space/testnet/tx/{}",
        broadcast_txid
    );

    // ========================================================================
    // Summary
    // ========================================================================
    println!("\n============================================================");
    println!("  CHECK-IN BROADCAST COMPLETE");
    println!("============================================================");
    println!("  Txid:        {}", broadcast_txid);
    println!("  From:        {} (P2WPKH)", EXPECTED_ADDRESS);
    println!(
        "  To:          {} (P2WSH inheritance)",
        inheritance_spk.to_hex_string()
    );
    println!("  Amount:      {} sats", output_value.to_sat());
    println!("  Fee:         {} sats", FEE_SATS);
    println!(
        "  Policy:      Owner immediate + Heir after {} blocks (~6 months)",
        timelock.blocks()
    );
    println!("  Descriptor:  {}", descriptor);
    println!(
        "  Heir mnemonic: {} (SAVE THIS for future tests)",
        heir_mnemonic
    );
    println!("============================================================\n");
}
