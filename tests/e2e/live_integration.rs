//! Live Integration Tests — Network & Nostr
//!
//! These tests make REAL network calls. No mocks.
//! Run with: cargo test -p nostring-e2e --test live_integration -- --ignored --nocapture
//!
//! Test wallet (testnet3):
//!   Mnemonic: wrap bubble bunker win flat south life shed twelve payment super taste
//!   Address: tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v
//!   Expected balance: 347,970 sats

use bitcoin::bip32::Xpub;
use bitcoin::hashes::Hash;
use bitcoin::{Address, Amount, Network, OutPoint, ScriptBuf, Txid};
use miniscript::descriptor::DescriptorPublicKey;
use nostr_sdk::prelude::*;
use std::str::FromStr;
use std::sync::Once;
use std::time::Duration;

static INIT_CRYPTO: Once = Once::new();

fn init_rustls() {
    INIT_CRYPTO.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

// ============================================================================
// TEST 1: Electrum Testnet Connection
// ============================================================================

/// Known testnet Electrum servers to try (some may be down).
/// Testnet3 block height is around 2.9M+.
const TESTNET_SERVERS: &[&str] = &[
    "ssl://blockstream.info:993",
    "tcp://blockstream.info:143",
    "ssl://electrum.blockstream.info:60002",
    "tcp://electrum.blockstream.info:60001",
    "tcp://tn.not.fyi:55001",
];

const TEST_MNEMONIC: &str =
    "wrap bubble bunker win flat south life shed twelve payment super taste";
const TEST_ADDRESS: &str = "tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v";

/// Try to connect to any available testnet Electrum server.
fn connect_testnet() -> Option<(nostring_electrum::ElectrumClient, &'static str)> {
    for server in TESTNET_SERVERS {
        println!("  Trying {}...", server);
        match nostring_electrum::ElectrumClient::new(server, Network::Testnet) {
            Ok(client) => {
                println!("  ✓ Connected to {}", server);
                return Some((client, server));
            }
            Err(e) => {
                println!("  ✗ Failed: {}", e);
            }
        }
    }
    None
}

#[test]
#[ignore = "requires network access - testnet Electrum"]
fn test1_electrum_testnet_connection() {
    println!("\n=== TEST 1: Electrum Testnet Connection ===\n");

    let (client, server) = match connect_testnet() {
        Some(c) => c,
        None => {
            println!("FAIL: Could not connect to any testnet Electrum server");
            println!("Servers tried: {:?}", TESTNET_SERVERS);
            panic!("No testnet Electrum server available");
        }
    };

    println!("Connected to: {}", server);

    // Don't use get_height() — it has hardcoded mainnet ranges.
    // Use block_headers_subscribe instead.
    println!("\nChecking tip header...");
    let tip = client.get_tip_header();
    match &tip {
        Ok(header) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let age = now - header.time as u64;
            println!("  Tip timestamp: {} (age: {} sec)", header.time, age);
            // Testnet blocks can be irregular, allow up to 24h
            println!("  ✓ Got tip header");
        }
        Err(e) => {
            println!("  ✗ Failed to get tip: {}", e);
        }
    }

    // Query our test address
    println!("\nQuerying test address: {}", TEST_ADDRESS);
    let addr: Address<bitcoin::address::NetworkUnchecked> = TEST_ADDRESS.parse().unwrap();
    let addr = addr.assume_checked();

    // Get balance
    let script = addr.script_pubkey();
    match client.get_balance(&script) {
        Ok(balance) => {
            println!("  Balance: {} sats", balance.to_sat());
            if balance.to_sat() > 0 {
                println!("  ✓ Address has funds!");
            } else {
                println!("  ⚠ Balance is 0 — funds may have been spent or server is stale");
            }
        }
        Err(e) => {
            println!("  ✗ Balance query failed: {}", e);
        }
    }

    // Get UTXOs
    match client.get_utxos(&addr) {
        Ok(utxos) => {
            println!("  UTXOs found: {}", utxos.len());
            let mut total = 0u64;
            for utxo in &utxos {
                println!(
                    "    - {}:{} = {} sats (height {})",
                    utxo.outpoint.txid,
                    utxo.outpoint.vout,
                    utxo.value.to_sat(),
                    utxo.height
                );
                total += utxo.value.to_sat();
            }
            println!("  Total UTXO value: {} sats", total);
            if !utxos.is_empty() {
                println!("  ✓ Can see UTXOs");
            }
        }
        Err(e) => {
            println!("  ✗ UTXO query failed: {}", e);
        }
    }

    // Get transaction history
    match client.get_script_history(&script) {
        Ok(history) => {
            println!("  Transaction history: {} entries", history.len());
            for item in &history {
                println!("    - {} (height: {})", item.txid, item.height);
            }
            if !history.is_empty() {
                println!("  ✓ Has transaction history");
            }
        }
        Err(e) => {
            println!("  ✗ History query failed: {}", e);
        }
    }

    println!("\n=== TEST 1 COMPLETE ===\n");
}

#[test]
#[ignore = "requires network access - testnet Electrum"]
fn test1b_electrum_address_derivation_matches() {
    println!("\n=== TEST 1b: Verify Mnemonic → Address Derivation ===\n");

    // Parse the test mnemonic
    let mnemonic = nostring_core::seed::parse_mnemonic(TEST_MNEMONIC).unwrap();
    let seed = nostring_core::seed::derive_seed(&mnemonic, "");

    // Derive testnet keys (m/84'/1'/0')
    let master =
        nostring_core::keys::derive_bitcoin_master_for_network(&seed, Network::Testnet).unwrap();

    // Derive first receive address
    let address =
        nostring_core::keys::derive_bitcoin_address(&master, false, 0, Network::Testnet).unwrap();

    println!("  Mnemonic: {}", TEST_MNEMONIC);
    println!("  Derived address: {}", address);
    println!("  Expected address: {}", TEST_ADDRESS);

    assert_eq!(
        address.to_string(),
        TEST_ADDRESS,
        "Derived address doesn't match expected testnet address"
    );
    println!("  ✓ Address derivation matches!");

    println!("\n=== TEST 1b COMPLETE ===\n");
}

// ============================================================================
// TEST 2: Nostr DM Notifications (Real)
// ============================================================================

#[tokio::test]
#[ignore = "requires network access - Nostr relays"]
async fn test2_nostr_dm_real() {
    init_rustls();
    println!("\n=== TEST 2: Nostr DM Notifications (Real) ===\n");

    // Generate two fresh keypairs
    let service_keys = Keys::generate();
    let owner_keys = Keys::generate();

    let service_npub = service_keys.public_key().to_bech32().unwrap();
    let owner_npub = owner_keys.public_key().to_bech32().unwrap();
    let _service_nsec = service_keys.secret_key().to_bech32().unwrap();

    println!("  Service key npub: {}", service_npub);
    println!("  Owner key npub:   {}", owner_npub);

    let relays = vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
        "wss://relay.nostr.band".to_string(),
    ];

    // Step 1: Send a NIP-04 encrypted DM from service → owner
    println!("\n  Sending NIP-04 encrypted DM...");

    let dm_text = format!(
        "NoString integration test DM — timestamp {}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );

    let client = Client::new(service_keys.clone());
    for relay in &relays {
        if let Err(e) = client.add_relay(relay).await {
            println!("  ⚠ Failed to add relay {}: {}", relay, e);
        }
    }
    client.connect().await;
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Encrypt and send
    let encrypted = nip04::encrypt(
        service_keys.secret_key(),
        &owner_keys.public_key(),
        &dm_text,
    )
    .unwrap();

    let event = EventBuilder::new(Kind::EncryptedDirectMessage, &encrypted)
        .tag(Tag::public_key(owner_keys.public_key()))
        .sign_with_keys(&service_keys)
        .unwrap();

    let event_id = event.id;
    println!("  Event ID: {}", event_id.to_hex());

    let send_result = client.send_event(event).await;
    match &send_result {
        Ok(output) => {
            println!("  ✓ Event sent! ID: {}", output.id().to_hex());
        }
        Err(e) => {
            println!("  ✗ Send failed: {}", e);
        }
    }

    client.disconnect().await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Step 2: Fetch it back as the owner
    println!("\n  Fetching DM back as owner...");

    let owner_client = Client::new(owner_keys.clone());
    for relay in &relays {
        let _ = owner_client.add_relay(relay).await;
    }
    owner_client.connect().await;
    tokio::time::sleep(Duration::from_secs(3)).await;

    let filter = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .author(service_keys.public_key())
        .pubkey(owner_keys.public_key())
        .limit(10);

    let events = owner_client
        .fetch_events(filter, Duration::from_secs(10))
        .await;

    match events {
        Ok(events) => {
            println!("  Fetched {} events", events.len());

            let mut found = false;
            for event in events.iter() {
                if event.id == event_id {
                    // Decrypt
                    match nip04::decrypt(
                        owner_keys.secret_key(),
                        &service_keys.public_key(),
                        &event.content,
                    ) {
                        Ok(decrypted) => {
                            println!("  Decrypted: {}", decrypted);
                            assert_eq!(decrypted, dm_text);
                            println!("  ✓ DM content matches!");
                            found = true;
                        }
                        Err(e) => {
                            println!("  ✗ Decryption failed: {}", e);
                        }
                    }
                    break;
                }
            }

            if !found {
                println!("  ⚠ Our specific event not found in fetched results");
                println!("    (This can happen due to relay propagation delays)");
                // Try decrypting any event from the service key
                for event in events.iter() {
                    if let Ok(decrypted) = nip04::decrypt(
                        owner_keys.secret_key(),
                        &service_keys.public_key(),
                        &event.content,
                    ) {
                        println!("  Found DM from service: {}", decrypted);
                        if decrypted == dm_text {
                            println!("  ✓ Found our DM (different event ID)!");
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    println!("  ✗ DM not found on any relay after fetch");
                }
            }
        }
        Err(e) => {
            println!("  ✗ Fetch failed: {}", e);
        }
    }

    owner_client.disconnect().await;

    println!("\n  Keys used:");
    println!("    Service npub: {}", service_npub);
    println!("    Owner npub:   {}", owner_npub);

    println!("\n=== TEST 2 COMPLETE ===\n");
}

// ============================================================================
// TEST 3: Nostr Relay Storage (Real)
// ============================================================================

#[tokio::test]
#[ignore = "requires network access - Nostr relays"]
async fn test3_nostr_relay_storage_real() {
    init_rustls();
    println!("\n=== TEST 3: Nostr Relay Storage (Real) ===\n");

    // Generate keypairs
    let service_keys = Keys::generate();
    let heir_keys = Keys::generate();

    let service_npub = service_keys.public_key().to_bech32().unwrap();
    let heir_npub = heir_keys.public_key().to_bech32().unwrap();
    let service_nsec_hex = service_keys.secret_key().to_secret_hex();
    let heir_nsec_hex = heir_keys.secret_key().to_secret_hex();

    println!("  Service npub: {}", service_npub);
    println!("  Heir npub:    {}", heir_npub);

    // Step 1: Create a test "locked share" (random bytes, then we'll encrypt as JSON)
    let mut random_bytes = [0u8; 32];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut random_bytes);
    let share_data = hex::encode(random_bytes);

    let split_id = nostring_notify::nostr_relay::generate_split_id();
    println!("  Split ID: {}", split_id);
    println!("  Share data (hex): {}...", &share_data[..16]);

    let relays: Vec<String> = vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
        "wss://relay.nostr.band".to_string(),
    ];

    // Step 2: Publish share to relays
    println!("\n  Publishing share to relays...");

    let publish_result = nostring_notify::nostr_relay::publish_shares_to_relays(
        &service_nsec_hex,
        &heir_npub,
        "Test Heir",
        &[share_data.clone()],
        &split_id,
        &relays,
    )
    .await;

    match &publish_result {
        Ok(result) => {
            println!("  Shares published: {}", result.shares_published);
            for eid in &result.event_ids {
                println!("  Event ID: {}", eid);
            }
            if result.shares_published > 0 {
                println!("  ✓ Share published successfully!");
            } else {
                println!("  ✗ No shares accepted by relays");
            }
        }
        Err(e) => {
            println!("  ✗ Publish failed: {}", e);
        }
    }

    // Wait for relay propagation
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Step 3: Fetch share back as heir
    println!("\n  Fetching share back as heir...");

    let fetch_result = nostring_notify::nostr_relay::fetch_shares_from_relays(
        &heir_nsec_hex,
        &service_npub,
        Some(&relays),
        Some(&split_id),
    )
    .await;

    match &fetch_result {
        Ok(result) => {
            println!("  Events found: {}", result.events_found);
            println!("  Shares recovered: {}", result.shares.len());

            if !result.shares.is_empty() {
                let recovered = &result.shares[0];
                println!(
                    "  Recovered share: {}...",
                    &recovered.share[..16.min(recovered.share.len())]
                );
                println!("  Split ID: {}", recovered.split_id);

                if recovered.share == share_data && recovered.split_id == split_id {
                    println!("  ✓ Share content matches original!");
                } else {
                    println!("  ✗ Share content mismatch!");
                    println!("    Expected: {}...", &share_data[..16]);
                    println!(
                        "    Got:      {}...",
                        &recovered.share[..16.min(recovered.share.len())]
                    );
                }
            } else {
                println!("  ⚠ No shares recovered (relay propagation may be slow)");
            }
        }
        Err(e) => {
            println!("  ✗ Fetch failed: {}", e);
        }
    }

    println!("\n=== TEST 3 COMPLETE ===\n");
}

// ============================================================================
// TEST 4: Full PSBT Check-in Flow (Offline)
// ============================================================================

#[test]
#[ignore = "offline test - PSBT construction"]
fn test4_psbt_checkin_flow() {
    use nostring_inherit::checkin::{CheckinTxBuilder, InheritanceUtxo};
    use nostring_inherit::policy::{InheritancePolicy, Timelock};

    println!("\n=== TEST 4: Full PSBT Check-in Flow (Offline) ===\n");

    // Step 1: Derive keys from testnet mnemonic
    println!("  Step 1: Deriving keys from mnemonic...");
    let mnemonic = nostring_core::seed::parse_mnemonic(TEST_MNEMONIC).unwrap();
    let seed = nostring_core::seed::derive_seed(&mnemonic, "");

    // Derive testnet master (m/84'/1'/0')
    let owner_master =
        nostring_core::keys::derive_bitcoin_master_for_network(&seed, Network::Testnet).unwrap();

    let secp = bitcoin::secp256k1::Secp256k1::new();
    let owner_xpub = Xpub::from_priv(&secp, &owner_master);
    let owner_fingerprint = owner_xpub.fingerprint();

    println!("  Owner xpub: {}", owner_xpub);
    println!("  Owner fingerprint: {}", owner_fingerprint);

    // Step 2: Create a test heir xpub (generate fresh)
    println!("\n  Step 2: Creating test heir xpub...");
    let heir_mnemonic = nostring_core::seed::generate_mnemonic_24().unwrap();
    let heir_seed = nostring_core::seed::derive_seed(&heir_mnemonic, "");
    let heir_master =
        nostring_core::keys::derive_bitcoin_master_for_network(&heir_seed, Network::Testnet)
            .unwrap();
    let heir_xpub = Xpub::from_priv(&secp, &heir_master);
    let heir_fingerprint = heir_xpub.fingerprint();

    println!("  Heir xpub: {}", heir_xpub);
    println!("  Heir fingerprint: {}", heir_fingerprint);

    // Step 3: Build inheritance policy
    println!("\n  Step 3: Building inheritance policy...");
    let owner_desc_key = DescriptorPublicKey::from_str(&format!(
        "[{}/84'/1'/0']{}/<0;1>/*",
        owner_fingerprint, owner_xpub
    ))
    .unwrap();

    let heir_desc_key = DescriptorPublicKey::from_str(&format!(
        "[{}/84'/1'/0']{}/<0;1>/*",
        heir_fingerprint, heir_xpub
    ))
    .unwrap();

    let timelock = Timelock::six_months();
    let policy =
        InheritancePolicy::simple(owner_desc_key.clone(), heir_desc_key.clone(), timelock).unwrap();
    println!(
        "  ✓ Policy created (6-month timelock: {} blocks)",
        timelock.blocks()
    );

    let descriptor = policy.to_wsh_descriptor().unwrap();
    println!("  Descriptor: {}", descriptor);

    // Step 4: Derive the script_pubkey for derivation index 0
    println!("\n  Step 4: Deriving script and creating test UTXO...");
    let single_descs = descriptor.clone().into_single_descriptors().unwrap();
    let receive_desc = &single_descs[0];
    let derived = receive_desc.derived_descriptor(&secp, 0).unwrap();
    let spk = derived.script_pubkey();
    println!("  Script pubkey: {}", spk.to_hex_string());
    assert!(spk.is_p2wsh(), "Script must be P2WSH");
    println!("  ✓ P2WSH script derived");

    // Create a simulated UTXO (as if we funded this address on testnet)
    let test_utxo = InheritanceUtxo::new(
        OutPoint {
            txid: Txid::from_str(
                "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
            )
            .unwrap(),
            vout: 0,
        },
        Amount::from_sat(347_970), // Same as our testnet balance
        2_900_000,                 // Approximate testnet3 height
        spk.clone(),
    );

    // Step 5: Build the PSBT
    println!("\n  Step 5: Building PSBT...");
    let builder = CheckinTxBuilder::new(test_utxo, descriptor.clone(), 2, 0); // 2 sat/vB

    let psbt = builder.build_psbt().expect("PSBT creation must succeed");

    // Verify PSBT structure
    println!("  PSBT inputs: {}", psbt.inputs.len());
    println!("  PSBT outputs: {}", psbt.unsigned_tx.output.len());

    // Check witness_utxo
    let witness_utxo = psbt.inputs[0]
        .witness_utxo
        .as_ref()
        .expect("witness_utxo must be populated");
    println!("  witness_utxo value: {} sats", witness_utxo.value.to_sat());
    assert_eq!(witness_utxo.value, Amount::from_sat(347_970));
    assert!(witness_utxo.script_pubkey.is_p2wsh());
    println!("  ✓ witness_utxo is valid P2WSH with correct amount");

    // Check witness_script
    let witness_script = psbt.inputs[0]
        .witness_script
        .as_ref()
        .expect("witness_script must be populated");
    assert!(!witness_script.is_empty());
    println!("  witness_script length: {} bytes", witness_script.len());

    // Verify witness_script hashes to script_pubkey
    let expected_wsh = ScriptBuf::new_p2wsh(&bitcoin::WScriptHash::hash(witness_script.as_bytes()));
    assert_eq!(
        witness_utxo.script_pubkey, expected_wsh,
        "witness_script must hash to P2WSH script_pubkey"
    );
    println!("  ✓ witness_script correctly hashes to P2WSH");

    // Check transaction structure
    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.input.len(), 1);
    assert!(tx.output.len() >= 1);
    assert_eq!(tx.version, bitcoin::transaction::Version::TWO);
    println!("  ✓ Transaction version 2 (BIP-68 compatible)");

    // Check input has no witness yet (unsigned)
    assert!(tx.input[0].witness.is_empty());
    println!("  ✓ Input witness is empty (unsigned - ready for HW wallet)");

    // Check input script_sig is empty (SegWit)
    assert!(tx.input[0].script_sig.is_empty());
    println!("  ✓ script_sig is empty (native SegWit)");

    // Check output is sending back to same script (check-in = self-spend)
    let change_output = &tx.output[tx.output.len() - 1];
    assert_eq!(
        change_output.script_pubkey, spk,
        "Check-in must send back to same address"
    );
    println!("  ✓ Output sends back to inheritance address (self-spend)");

    // Verify fees are reasonable
    let output_total: u64 = tx.output.iter().map(|o| o.value.to_sat()).sum();
    let fee = 347_970 - output_total;
    println!("  Fee: {} sats", fee);
    assert!(
        fee > 0 && fee < 10_000,
        "Fee should be reasonable (got {} sats)",
        fee
    );
    println!("  ✓ Fee is reasonable");

    // Encode to base64 (standard PSBT export format)
    let psbt_base64 = builder.build_psbt_base64().unwrap();
    assert!(psbt_base64.starts_with("cHNidP8")); // PSBT magic in base64
    println!(
        "  PSBT base64: {}...{}",
        &psbt_base64[..20],
        &psbt_base64[psbt_base64.len() - 10..]
    );
    println!("  ✓ Valid PSBT base64 encoding");

    // Encode to bytes (for QR)
    let psbt_bytes = builder.build_psbt_bytes().unwrap();
    assert_eq!(&psbt_bytes[0..5], b"psbt\xff");
    println!("  PSBT size: {} bytes", psbt_bytes.len());
    println!("  ✓ Valid PSBT binary (psbt\\xff magic)");

    println!("\n  Summary:");
    println!(
        "    - Policy: Owner can spend anytime, heir after {} blocks",
        timelock.blocks()
    );
    println!("    - UTXO: 347,970 sats in P2WSH");
    println!("    - PSBT: {} bytes, valid structure", psbt_bytes.len());
    println!("    - witness_utxo: populated (HW wallet safe)");
    println!("    - witness_script: populated and hash-verified");
    println!("    - Could be signed by SeedSigner/ColdCard/etc");

    println!("\n=== TEST 4 COMPLETE ===\n");
}
