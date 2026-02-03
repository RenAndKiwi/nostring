//! End-to-end testnet validation
//! 
//! Run with: cargo test --test e2e_testnet -- --ignored --nocapture

use bitcoin::Network;
use nostring_core::{derive_seed, generate_mnemonic, derive_bitcoin_master_for_network, derive_bitcoin_address};
use nostring_electrum::ElectrumClient;
use nostring_inherit::{InheritancePolicy, Timelock};

/// Full flow test on testnet
/// 
/// Prerequisites:
/// - Testnet coins at tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v
#[test]
#[ignore = "requires testnet setup"]
fn test_full_inheritance_flow() {
    println!("\n=== NoString E2E Testnet Test ===\n");
    
    // 1. Use our test wallet
    let mnemonic = nostring_core::parse_mnemonic(
        "wrap bubble bunker win flat south life shed twelve payment super taste"
    ).expect("valid mnemonic");
    
    let seed = derive_seed(&mnemonic, "");
    println!("✓ Seed derived from mnemonic");
    
    // 2. Derive testnet keys
    let master = derive_bitcoin_master_for_network(&seed, Network::Testnet)
        .expect("derive master");
    let owner_address = derive_bitcoin_address(&master, false, 0, Network::Testnet)
        .expect("derive address");
    
    println!("✓ Owner address: {}", owner_address);
    assert_eq!(owner_address.to_string(), "tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v");
    
    // 3. Connect to testnet Electrum
    let client = ElectrumClient::new("ssl://blockstream.info:993", Network::Testnet)
        .expect("connect to testnet");
    println!("✓ Connected to testnet Electrum");
    
    // 4. Check current height
    let height = client.get_height().expect("get height");
    println!("✓ Current testnet height: {}", height);
    
    // 5. Check our UTXOs
    let script = owner_address.script_pubkey();
    let utxos = client.get_utxos(&script).expect("get utxos");
    println!("✓ Found {} UTXO(s)", utxos.len());
    
    for utxo in &utxos {
        println!("  - {}:{} = {} sats (height {})", 
            utxo.outpoint.txid, 
            utxo.outpoint.vout,
            utxo.value.to_sat(),
            utxo.height
        );
    }
    
    assert!(!utxos.is_empty(), "Expected at least one UTXO");
    
    // 6. Create a simple heir (using a test xpub)
    // This is just for policy testing - not a real heir
    let heir_xpub = "tpubDC5FSnBiZDMmhiuCmWAYsLwgLYrrT9rAqvTySfuCCrgsWz8wxMXUS9Tb9iVMvcRbvFcAHGkMD5Kx8koh4GquNGNTfohfk7pgjhaPCdXpoba";
    
    println!("\n✓ Test heir xpub (for policy creation only)");
    
    // 7. Create inheritance policy (100 blocks for testing)
    let policy = InheritancePolicy::simple(
        &master.to_string(),
        heir_xpub,
        Timelock::Blocks(100),
    ).expect("create policy");
    
    println!("✓ Created inheritance policy");
    println!("  Timelock: 100 blocks (~17 hours on testnet)");
    
    // 8. Get the policy descriptor
    let descriptor = policy.to_descriptor().expect("to descriptor");
    println!("✓ Policy descriptor: {}...", &descriptor[..80]);
    
    println!("\n=== E2E Test PASSED ===\n");
    println!("Summary:");
    println!("- Testnet connection: OK");
    println!("- UTXO discovery: OK ({} sats)", utxos.iter().map(|u| u.value.to_sat()).sum::<u64>());
    println!("- Policy creation: OK");
    println!("\nNext steps:");
    println!("1. Fund the policy address");
    println!("2. Test check-in PSBT generation");
    println!("3. Test broadcast flow");
}
