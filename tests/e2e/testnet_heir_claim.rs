//! Real Testnet Heir Claim Test
//!
//! This test performs a REAL heir claim on Bitcoin testnet3.
//! It creates an inheritance policy with a 1-block CSV timelock,
//! funds it from the owner's P2WPKH wallet, waits for confirmation,
//! then builds, signs, and broadcasts the heir claim transaction.
//!
//! This is the critical end-to-end proof that inheritance unlocks correctly.
//!
//! Run with:
//!   cargo test -p nostring-e2e --test testnet_heir_claim -- --ignored --nocapture

use bitcoin::{
    absolute::LockTime,
    bip32::{DerivationPath, Xpub},
    hashes::Hash,
    sighash::{EcdsaSighashType, SighashCache},
    transaction::Version,
    Address, Amount, CompressedPublicKey, Network, OutPoint, ScriptBuf, Sequence, Transaction,
    TxIn, TxOut, WScriptHash, Witness,
};
use miniscript::descriptor::DescriptorPublicKey;
use miniscript::Descriptor;
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
const FUNDING_AMOUNT_SATS: u64 = 5_000;
const FEE_SATS: u64 = 500;
const HEIR_CLAIM_FEE_SATS: u64 = 500;

/// Connect to a testnet Electrum server, trying multiple servers
fn connect_testnet() -> ElectrumClient {
    let servers = [
        "ssl://blockstream.info:993",
        "ssl://mempool.space:60002",
        "ssl://electrum.blockstream.info:60002",
    ];

    for server in &servers {
        println!("  Trying {}...", server);
        match ElectrumClient::new(server, Network::Testnet) {
            Ok(client) => {
                println!("  ✓ Connected to {}", server);
                return client;
            }
            Err(e) => {
                println!("  ✗ Failed: {}", e);
            }
        }
    }
    panic!("Could not connect to any testnet Electrum server");
}

/// Derive the explicit witness script from a multi-path descriptor at a given index
fn derive_witness_script(
    descriptor: &Descriptor<DescriptorPublicKey>,
    index: u32,
) -> ScriptBuf {
    let secp = bitcoin::secp256k1::Secp256k1::verification_only();
    let single_descs = descriptor.clone().into_single_descriptors().unwrap();
    let receive_desc = &single_descs[0];
    let derived = receive_desc.derived_descriptor(&secp, index).unwrap();
    derived
        .explicit_script()
        .expect("P2WSH descriptor must have explicit script")
}

/// Derive the script_pubkey (P2WSH) from a multi-path descriptor at a given index
fn derive_script_pubkey(
    descriptor: &Descriptor<DescriptorPublicKey>,
    index: u32,
) -> ScriptBuf {
    let secp = bitcoin::secp256k1::Secp256k1::verification_only();
    let single_descs = descriptor.clone().into_single_descriptors().unwrap();
    let receive_desc = &single_descs[0];
    let derived = receive_desc.derived_descriptor(&secp, index).unwrap();
    derived.script_pubkey()
}

/// Real testnet heir claim: fund P2WSH, wait for CSV, claim as heir
///
/// Full flow:
/// 1. Owner sends funds from P2WPKH → P2WSH inheritance address (CSV 1 block)
/// 2. Wait for funding tx to confirm (1 block)
/// 3. After 1 more block, heir path is spendable
/// 4. Build heir claim tx spending via heir branch
/// 5. Sign with heir key and broadcast
/// 6. Verify confirmation
#[test]
#[ignore = "REAL TESTNET BROADCAST - run manually with --nocapture"]
fn test_real_testnet_heir_claim() {
    init_rustls();
    println!("\n============================================================");
    println!("  REAL TESTNET HEIR CLAIM — END-TO-END");
    println!("============================================================\n");

    let secp = bitcoin::secp256k1::Secp256k1::new();

    // ========================================================================
    // Step 1: Derive owner keys from testnet mnemonic
    // ========================================================================
    println!("Step 1: Deriving owner keys from mnemonic...");

    let mnemonic = nostring_core::seed::parse_mnemonic(TEST_MNEMONIC).unwrap();
    let seed = nostring_core::seed::derive_seed(&mnemonic, "");

    let owner_master =
        nostring_core::keys::derive_bitcoin_master_for_network(&seed, Network::Testnet).unwrap();
    let owner_xpub = Xpub::from_priv(&secp, &owner_master);
    let owner_fingerprint = owner_xpub.fingerprint();

    // First receive address m/84'/1'/0'/0/0
    let child_path: DerivationPath = "m/0/0".parse().unwrap();
    let child_priv = owner_master.derive_priv(&secp, &child_path).unwrap();
    let child_pubkey = child_priv.private_key.public_key(&secp);
    let compressed_pubkey = CompressedPublicKey(child_pubkey);

    let owner_address = Address::p2wpkh(&compressed_pubkey, Network::Testnet);
    assert_eq!(owner_address.to_string(), EXPECTED_ADDRESS);

    println!("  ✓ Owner address: {}", owner_address);
    println!("  ✓ Owner fingerprint: {}", owner_fingerprint);

    // ========================================================================
    // Step 2: Generate heir keypair (deterministic for repeatability)
    // ========================================================================
    println!("\nStep 2: Generating heir keypair...");

    let heir_mnemonic = nostring_core::seed::generate_mnemonic_24().unwrap();
    let heir_seed = nostring_core::seed::derive_seed(&heir_mnemonic, "");
    let heir_master =
        nostring_core::keys::derive_bitcoin_master_for_network(&heir_seed, Network::Testnet)
            .unwrap();
    let heir_xpub = Xpub::from_priv(&secp, &heir_master);
    let heir_fingerprint = heir_xpub.fingerprint();

    // Derive heir's child key for signing (m/0/0 from m/84'/1'/0')
    let heir_child_path: DerivationPath = "m/0/0".parse().unwrap();
    let heir_child_priv = heir_master.derive_priv(&secp, &heir_child_path).unwrap();
    let heir_child_pubkey = heir_child_priv.private_key.public_key(&secp);

    println!("  ✓ Heir fingerprint: {}", heir_fingerprint);
    println!("  ✓ Heir pubkey: {}", heir_child_pubkey);
    println!("  ✓ Heir mnemonic: {}", heir_mnemonic);

    // ========================================================================
    // Step 3: Create inheritance policy with CSV 1 block (minimum timelock)
    // ========================================================================
    println!("\nStep 3: Creating inheritance policy (CSV 1 block)...");

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

    let timelock = Timelock::from_blocks(1).expect("CSV 1 block");
    let policy = InheritancePolicy::simple(
        owner_desc_key.clone(),
        heir_desc_key.clone(),
        timelock,
    )
    .unwrap();

    let descriptor = policy.to_wsh_descriptor().unwrap();

    println!("  ✓ Policy: owner immediate + heir after 1 block (CSV)");
    println!("  ✓ Descriptor: {}", descriptor);

    // Derive the P2WSH inheritance address at index 0
    let inheritance_spk = derive_script_pubkey(&descriptor, 0);
    let witness_script = derive_witness_script(&descriptor, 0);

    assert!(inheritance_spk.is_p2wsh(), "Must be P2WSH");
    println!("  ✓ Inheritance scriptPubKey: {}", inheritance_spk.to_hex_string());
    println!("  ✓ Witness script ({} bytes): {}", witness_script.len(), witness_script.to_hex_string());

    // Verify the witness script hashes to the P2WSH script_pubkey
    let expected_wsh = ScriptBuf::new_p2wsh(&WScriptHash::hash(witness_script.as_bytes()));
    assert_eq!(inheritance_spk, expected_wsh, "Witness script hash mismatch!");
    println!("  ✓ Witness script hash verified");

    // ========================================================================
    // Step 4: Connect to testnet and find owner UTXOs
    // ========================================================================
    println!("\nStep 4: Connecting to testnet Electrum...");

    let client = connect_testnet();

    let height = client.get_height().expect("Failed to get block height");
    println!("  ✓ Current testnet height: {}", height);

    let owner_spk = owner_address.script_pubkey();
    let utxos = client
        .get_utxos_for_script(owner_spk.as_script())
        .expect("Failed to get UTXOs");

    println!("  ✓ Owner UTXOs found: {}", utxos.len());
    for utxo in &utxos {
        println!(
            "    - {}:{} = {} sats (height {})",
            utxo.outpoint.txid, utxo.outpoint.vout,
            utxo.value.to_sat(), utxo.height
        );
    }

    if utxos.is_empty() {
        panic!("No UTXOs available at owner address {}. Fund it first!", EXPECTED_ADDRESS);
    }

    // Use the largest UTXO
    let best_utxo = utxos.iter().max_by_key(|u| u.value.to_sat()).unwrap();
    let spend_value = best_utxo.value.to_sat();

    println!("  ✓ Using UTXO: {}:{} ({} sats)",
        best_utxo.outpoint.txid, best_utxo.outpoint.vout, spend_value);

    assert!(
        spend_value >= FUNDING_AMOUNT_SATS + FEE_SATS,
        "UTXO too small: {} < {} + {}",
        spend_value, FUNDING_AMOUNT_SATS, FEE_SATS
    );

    // ========================================================================
    // Step 5: Build funding tx (P2WPKH → P2WSH inheritance + change back)
    // ========================================================================
    println!("\nStep 5: Building funding transaction (P2WPKH → P2WSH)...");

    let change_value = spend_value - FUNDING_AMOUNT_SATS - FEE_SATS;

    let funding_tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: best_utxo.outpoint,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(),
        }],
        output: vec![
            // Output 0: inheritance P2WSH
            TxOut {
                value: Amount::from_sat(FUNDING_AMOUNT_SATS),
                script_pubkey: inheritance_spk.clone(),
            },
            // Output 1: change back to owner P2WPKH
            TxOut {
                value: Amount::from_sat(change_value),
                script_pubkey: owner_spk.clone(),
            },
        ],
    };

    println!("  ✓ Output 0: {} sats → P2WSH inheritance", FUNDING_AMOUNT_SATS);
    println!("  ✓ Output 1: {} sats → owner change", change_value);
    println!("  ✓ Fee: {} sats", FEE_SATS);

    // ========================================================================
    // Step 6: Sign the funding tx with owner's key (P2WPKH)
    // ========================================================================
    println!("\nStep 6: Signing funding transaction...");

    let funding_sighash = {
        let mut cache = SighashCache::new(&funding_tx);
        cache
            .p2wpkh_signature_hash(
                0,
                &owner_spk,
                Amount::from_sat(spend_value),
                EcdsaSighashType::All,
            )
            .expect("Failed to compute sighash")
    };

    let funding_msg = bitcoin::secp256k1::Message::from_digest(funding_sighash.to_byte_array());
    let funding_sig = secp.sign_ecdsa(&funding_msg, &child_priv.private_key);

    // Verify
    secp.verify_ecdsa(&funding_msg, &funding_sig, &child_pubkey)
        .expect("Funding signature verification failed!");

    let mut funding_sig_bytes = funding_sig.serialize_der().to_vec();
    funding_sig_bytes.push(EcdsaSighashType::All.to_u32() as u8);

    let mut signed_funding_tx = funding_tx;
    let mut witness = Witness::new();
    witness.push(&funding_sig_bytes);
    witness.push(child_pubkey.serialize());
    signed_funding_tx.input[0].witness = witness;

    let funding_txid = signed_funding_tx.compute_txid();
    let funding_hex = bitcoin::consensus::encode::serialize_hex(&signed_funding_tx);

    println!("  ✓ Funding tx signed. Txid: {}", funding_txid);
    println!("  ✓ Raw tx: {} bytes", funding_hex.len() / 2);

    // ========================================================================
    // Step 7: Broadcast funding tx
    // ========================================================================
    println!("\nStep 7: Broadcasting funding transaction...");

    let broadcast_txid = client
        .broadcast(&signed_funding_tx)
        .expect("FUNDING BROADCAST FAILED");

    assert_eq!(broadcast_txid, funding_txid);
    println!("  ✓ FUNDING BROADCAST SUCCESS!");
    println!("  ✓ Txid: {}", funding_txid);
    println!("  ✓ Explorer: https://mempool.space/testnet/tx/{}", funding_txid);

    // ========================================================================
    // Step 8: Wait for funding tx to confirm
    // ========================================================================
    println!("\nStep 8: Waiting for funding tx to confirm...");
    println!("  (Testnet blocks are ~10 min average, but can be irregular)");

    let funding_vout = 0u32; // Our inheritance output is vout 0

    // Poll for confirmation — up to 30 minutes
    let max_wait = std::time::Duration::from_secs(30 * 60);
    let poll_interval = std::time::Duration::from_secs(30);
    let start = std::time::Instant::now();

    let mut confirmed = false;
    let mut funding_height = 0u32;

    while start.elapsed() < max_wait {
        // Check if funding tx output appears as UTXO on the inheritance script
        match client.get_utxos_for_script(inheritance_spk.as_script()) {
            Ok(utxos) => {
                if let Some(utxo) = utxos.iter().find(|u| {
                    u.outpoint.txid == funding_txid && u.outpoint.vout == funding_vout
                }) {
                    if utxo.height > 0 {
                        funding_height = utxo.height;
                        confirmed = true;
                        println!("  ✓ Funding tx confirmed at height {}!", funding_height);
                        break;
                    } else {
                        println!("  ⏳ Funding tx seen but unconfirmed ({}s elapsed)...",
                            start.elapsed().as_secs());
                    }
                } else {
                    println!("  ⏳ Funding tx not yet visible ({}s elapsed)...",
                        start.elapsed().as_secs());
                }
            }
            Err(e) => {
                println!("  ⚠ Error checking: {} ({}s elapsed)", e, start.elapsed().as_secs());
            }
        }

        std::thread::sleep(poll_interval);
    }

    if !confirmed {
        println!("\n  ⚠ FUNDING TX DID NOT CONFIRM within 30 minutes.");
        println!("  Funding txid: {}", funding_txid);
        println!("  The heir claim will need to be attempted later.");
        println!("  Inheritance P2WSH script: {}", inheritance_spk.to_hex_string());
        println!("  Witness script: {}", witness_script.to_hex_string());
        println!("  Heir mnemonic: {}", heir_mnemonic);
        panic!("Funding tx did not confirm in time. Save the txid and retry later.");
    }

    // ========================================================================
    // Step 9: Wait for CSV timelock to mature (1 more block after confirmation)
    // ========================================================================
    println!("\nStep 9: Waiting for CSV timelock to mature (need 1 block after confirmation)...");
    println!("  Funding confirmed at height {}. Heir can spend at height {} (CSV 1).",
        funding_height, funding_height + 1);

    let mut current_height = client.get_height().unwrap();

    // We need current_height >= funding_height + 1 for CSV 1 to be satisfied
    while current_height < funding_height + 1 {
        println!("  ⏳ Current height: {}. Need: {}. Waiting...",
            current_height, funding_height + 1);
        std::thread::sleep(poll_interval);
        current_height = client.get_height().unwrap();
    }

    println!("  ✓ CSV timelock matured! Current height: {} >= {}",
        current_height, funding_height + 1);

    // ========================================================================
    // Step 10: Build the heir claim transaction
    // ========================================================================
    println!("\nStep 10: Building heir claim transaction...");

    // Heir claims to their own P2WPKH address
    let heir_compressed = CompressedPublicKey(heir_child_pubkey);
    let heir_receive_address = Address::p2wpkh(&heir_compressed, Network::Testnet);

    let claim_value = FUNDING_AMOUNT_SATS - HEIR_CLAIM_FEE_SATS;

    // CRITICAL: For CSV spending, the input sequence must encode the timelock value.
    // CSV 1 block means sequence = 1 (with relative timelock flag).
    let csv_sequence = Sequence::from_height(1); // CSV 1 block

    let heir_claim_tx = Transaction {
        version: Version::TWO, // Required for BIP-68 (CSV)
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: funding_txid,
                vout: funding_vout,
            },
            script_sig: ScriptBuf::new(), // Empty for SegWit
            sequence: csv_sequence,       // CSV 1 block
            witness: Witness::default(),  // Will be filled after signing
        }],
        output: vec![TxOut {
            value: Amount::from_sat(claim_value),
            script_pubkey: heir_receive_address.script_pubkey(),
        }],
    };

    println!("  ✓ Input: {}:{} (CSV sequence: {:?})", funding_txid, funding_vout, csv_sequence);
    println!("  ✓ Output: {} sats → {} (heir P2WPKH)", claim_value, heir_receive_address);
    println!("  ✓ Fee: {} sats", HEIR_CLAIM_FEE_SATS);

    // ========================================================================
    // Step 11: Sign the heir claim (P2WSH with heir branch)
    // ========================================================================
    println!("\nStep 11: Signing heir claim transaction...");

    // For P2WSH, the sighash commits to the witness script (not the scriptPubKey)
    let claim_sighash = {
        let mut cache = SighashCache::new(&heir_claim_tx);
        cache
            .p2wsh_signature_hash(
                0,
                &witness_script,
                Amount::from_sat(FUNDING_AMOUNT_SATS),
                EcdsaSighashType::All,
            )
            .expect("Failed to compute P2WSH sighash")
    };

    let claim_msg = bitcoin::secp256k1::Message::from_digest(claim_sighash.to_byte_array());
    let claim_sig = secp.sign_ecdsa(&claim_msg, &heir_child_priv.private_key);

    // Verify locally
    secp.verify_ecdsa(&claim_msg, &claim_sig, &heir_child_pubkey)
        .expect("Heir claim signature verification failed!");

    let mut claim_sig_bytes = claim_sig.serialize_der().to_vec();
    claim_sig_bytes.push(EcdsaSighashType::All.to_u32() as u8);

    println!("  ✓ ECDSA signature created and verified locally");

    // ========================================================================
    // Step 12: Construct the witness for the heir branch
    // ========================================================================
    println!("\nStep 12: Constructing witness stack...");

    // The compiled miniscript for or_d(pk(owner), and_v(v:pk(heir), older(1)))
    // produces this script:
    //   <owner_pubkey> OP_CHECKSIG OP_IFDUP OP_NOTIF
    //     <heir_pubkey> OP_CHECKSIGVERIFY
    //     <1> OP_CHECKSEQUENCEVERIFY
    //   OP_ENDIF
    //
    // This uses pk() not pkh() — the heir pubkey is embedded in the script.
    //
    // To satisfy the heir branch (the NOTIF path):
    //   1. Owner's CHECKSIG consumes one stack item and fails → pushes 0
    //   2. IFDUP on 0 does nothing
    //   3. NOTIF takes the branch (because top is 0/false)
    //   4. <heir_pubkey> CHECKSIGVERIFY consumes heir's signature from stack
    //   5. CSV checks sequence
    //
    // Witness stack (bottom to top, then script):
    //   <heir_sig>            — consumed by CHECKSIGVERIFY in heir branch
    //   <empty>               — consumed by owner's CHECKSIG (fails → 0)
    //   <witness_script>      — the full script (required for P2WSH)

    let mut signed_claim_tx = heir_claim_tx;
    let mut claim_witness = Witness::new();

    // Witness items pushed bottom-to-top:
    // The script executes top-down consuming from the stack.
    // Owner CHECKSIG eats the top item (empty → fails → 0).
    // Then NOTIF enters heir branch, CHECKSIGVERIFY eats next item (heir sig).
    claim_witness.push(&claim_sig_bytes);          // heir signature (consumed by CHECKSIGVERIFY)
    claim_witness.push(&[]);                       // empty — fails owner CHECKSIG → enters NOTIF
    claim_witness.push(witness_script.as_bytes()); // the witness script itself (P2WSH requirement)

    signed_claim_tx.input[0].witness = claim_witness;

    let claim_txid = signed_claim_tx.compute_txid();
    let claim_hex = bitcoin::consensus::encode::serialize_hex(&signed_claim_tx);

    println!("  ✓ Witness stack constructed (3 items):");
    println!("    [0] heir signature ({} bytes)", claim_sig_bytes.len());
    println!("    [1] empty (fails owner CHECKSIG → enters heir branch)");
    println!("    [2] witness script ({} bytes)", witness_script.len());
    println!("  ✓ Heir claim txid: {}", claim_txid);
    println!("  ✓ Raw tx: {} bytes", claim_hex.len() / 2);
    println!("  ✓ Raw hex: {}", claim_hex);

    // ========================================================================
    // Step 13: Broadcast the heir claim transaction
    // ========================================================================
    println!("\nStep 13: Broadcasting heir claim transaction...");

    let claim_broadcast_txid = client
        .broadcast(&signed_claim_tx)
        .expect("HEIR CLAIM BROADCAST FAILED — transaction was rejected by the network");

    assert_eq!(claim_broadcast_txid, claim_txid);

    println!("  ✓ HEIR CLAIM BROADCAST SUCCESS!");
    println!("  ✓ Txid: {}", claim_broadcast_txid);
    println!("  ✓ Explorer: https://mempool.space/testnet/tx/{}", claim_broadcast_txid);

    // ========================================================================
    // Step 14: Wait for heir claim to confirm
    // ========================================================================
    println!("\nStep 14: Waiting for heir claim to confirm...");

    let claim_start = std::time::Instant::now();
    let mut claim_confirmed = false;

    while claim_start.elapsed() < max_wait {
        match client.get_utxos_for_script(heir_receive_address.script_pubkey().as_script()) {
            Ok(utxos) => {
                if let Some(utxo) = utxos.iter().find(|u| u.outpoint.txid == claim_txid) {
                    if utxo.height > 0 {
                        claim_confirmed = true;
                        println!("  ✓ Heir claim confirmed at height {}!", utxo.height);
                        break;
                    } else {
                        println!("  ⏳ Heir claim in mempool ({}s elapsed)...",
                            claim_start.elapsed().as_secs());
                    }
                } else {
                    println!("  ⏳ Heir claim not yet visible ({}s elapsed)...",
                        claim_start.elapsed().as_secs());
                }
            }
            Err(e) => {
                println!("  ⚠ Error checking: {} ({}s elapsed)", e, claim_start.elapsed().as_secs());
            }
        }
        std::thread::sleep(poll_interval);
    }

    // ========================================================================
    // Summary
    // ========================================================================
    println!("\n============================================================");
    println!("  HEIR CLAIM TEST COMPLETE");
    println!("============================================================");
    println!("  Funding txid:    {}", funding_txid);
    println!("  Claim txid:      {}", claim_txid);
    println!("  Funding amount:  {} sats", FUNDING_AMOUNT_SATS);
    println!("  Claimed amount:  {} sats", claim_value);
    println!("  CSV timelock:    1 block");
    println!("  Heir address:    {}", heir_receive_address);
    println!("  Claim confirmed: {}", claim_confirmed);
    println!("  Descriptor:      {}", descriptor);
    println!("  Heir mnemonic:   {}", heir_mnemonic);
    println!("============================================================");

    if claim_confirmed {
        println!("\n  🎉 FULL END-TO-END HEIR CLAIM VERIFIED ON TESTNET! 🎉\n");
    } else {
        println!("\n  ⚠ Heir claim broadcast succeeded but didn't confirm within the wait window.");
        println!("  Check the explorer for confirmation status.\n");
    }
}
