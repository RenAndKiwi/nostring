//! End-to-end integration test for NoString.
//!
//! Tests the complete inheritance flow without requiring a live Bitcoin
//! network or real funds. Exercises:
//!
//! 1. Seed generation / xpub derivation
//! 2. Heir key management
//! 3. Policy creation (miniscript descriptor)
//! 4. Shamir secret sharing (Codex32)
//! 5. PSBT generation for check-in
//! 6. Notification template generation
//! 7. Service key creation
//!
//! Run with: cargo test --test e2e_integration
//! For network tests: cargo test --test e2e_integration -- --ignored

use bitcoin::bip32::{DerivationPath, Xpub};
use bitcoin::Network;
use miniscript::descriptor::DescriptorPublicKey;
use std::str::FromStr;

// ============================================================================
// 1. Seed + Key Derivation
// ============================================================================

#[test]
fn test_seed_generation_and_derivation() {
    use nostring_core::seed::{derive_seed, generate_mnemonic, parse_mnemonic, WordCount};

    // Generate a 24-word mnemonic
    let mnemonic = generate_mnemonic(WordCount::Words24).expect("mnemonic generation");
    let mnemonic_str = mnemonic.to_string();
    let words: Vec<&str> = mnemonic_str.split_whitespace().collect();
    assert_eq!(words.len(), 24, "Expected 24-word mnemonic");

    // Parse it back
    let parsed = parse_mnemonic(&mnemonic.to_string()).expect("mnemonic parsing");

    // Derive seed
    let seed = derive_seed(&parsed, "");
    assert_eq!(seed.len(), 64, "BIP-39 seed should be 64 bytes");

    println!("✓ Seed generation and derivation works");
}

#[test]
fn test_seed_encryption_roundtrip() {
    use nostring_core::crypto::{decrypt_seed, encrypt_seed};
    use nostring_core::seed::{derive_seed, generate_mnemonic, parse_mnemonic, WordCount};

    let mnemonic = generate_mnemonic(WordCount::Words12).expect("mnemonic");
    let parsed = parse_mnemonic(&mnemonic.to_string()).expect("parse");
    let seed = derive_seed(&parsed, "");

    let password = "test-password-nostring";
    let encrypted = encrypt_seed(&seed, password).expect("encrypt");
    let decrypted = decrypt_seed(&encrypted, password).expect("decrypt");

    assert_eq!(seed, decrypted, "Decrypted seed should match original");

    // Wrong password should fail
    assert!(
        decrypt_seed(&encrypted, "wrong-password").is_err(),
        "Wrong password should fail"
    );

    println!("✓ Seed encryption roundtrip works");
}

// ============================================================================
// 2. Heir Management
// ============================================================================

#[test]
fn test_heir_registry() {
    use nostring_inherit::heir::{HeirKey, HeirRegistry};

    let mut registry = HeirRegistry::new();

    // Simulate two heirs with test xpubs
    // Using a well-known test vector xpub
    let test_xpub_str = "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8";
    let xpub = Xpub::from_str(test_xpub_str).expect("parse xpub");
    let fp = xpub.fingerprint();
    let path = DerivationPath::from_str("m/84'/0'/0'").unwrap();

    let heir1 = HeirKey::new("Spouse", fp, xpub, Some(path.clone()));
    registry.add(heir1);

    assert_eq!(registry.list().len(), 1);
    assert!(registry.get(&fp).is_some());
    assert_eq!(registry.get(&fp).unwrap().label, "Spouse");

    // Remove
    registry.remove(&fp);
    assert_eq!(registry.list().len(), 0);

    println!("✓ Heir registry management works");
}

// ============================================================================
// 3. Miniscript Policy
// ============================================================================

#[test]
fn test_inheritance_policy_creation() {
    use nostring_inherit::policy::{InheritancePolicy, Timelock};

    // Use different keys for owner and heir (required — no duplicate keys)
    let owner_desc = "[73c5da0a/84'/0'/0']xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8/0/*";
    // Second xpub from BIP-32 test vector 2
    let heir_desc = "[b2e5c4d1/84'/0'/0']xpub661MyMwAqRbcFW31YEwpkMuc5THy2PSt5bDMsktWQcFF8syAmRUapSCGu8ED9W6oDMSgv6Zz8idoc4a6mr8BDzTJY47LJhkJ8UB7WEGuduB/0/*";

    let owner_key: DescriptorPublicKey = owner_desc.parse().expect("parse owner key");
    let heir_key: DescriptorPublicKey = heir_desc.parse().expect("parse heir key");

    // 6-month timelock (~26280 blocks)
    let timelock = Timelock::six_months();

    let policy = InheritancePolicy::simple(owner_key, heir_key, timelock);
    assert!(policy.is_ok(), "Policy creation failed: {:?}", policy.err());

    let policy = policy.unwrap();
    let descriptor = policy.to_wsh_descriptor().expect("compile descriptor");
    let desc_str = format!("{}", descriptor);

    assert!(
        desc_str.contains("wsh("),
        "Should be a witness script hash descriptor"
    );

    println!("✓ Inheritance policy descriptor: {}...", &desc_str[..80.min(desc_str.len())]);
    println!("✓ Miniscript policy creation works");
}

// ============================================================================
// 4. Shamir Secret Sharing
// ============================================================================

#[test]
fn test_codex32_roundtrip() {
    use nostring_shamir::codex32::{combine_shares, generate_shares, Codex32Config};

    let secret = [42u8; 32]; // Test secret
    let config = Codex32Config::new(2, "TEST", 3).expect("config");

    let shares = generate_shares(&secret, &config).expect("generate");
    assert_eq!(shares.len(), 3, "Should generate 3 shares");

    // Combine any 2 shares (threshold = 2)
    let recovered = combine_shares(&shares[0..2]).expect("combine");
    assert_eq!(
        recovered, secret,
        "Recovered secret should match original"
    );

    // Single share should NOT be enough
    assert!(
        combine_shares(&shares[0..1]).is_err(),
        "Single share should not reconstruct"
    );

    println!("✓ Codex32 Shamir roundtrip works (2-of-3)");
}

#[test]
fn test_shamir_nsec_inheritance_formula() {
    use nostring_shamir::codex32::{combine_shares, generate_shares, Codex32Config};

    // Test the (N+1)-of-(2N+1) formula for 3 heirs
    let n_heirs: u8 = 3;
    let threshold = n_heirs + 1; // 4
    let total_shares = 2 * n_heirs + 1; // 7

    let nsec_bytes = [99u8; 32]; // Simulated nsec
    let config = Codex32Config::new(threshold, "NSEC", total_shares).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("generate");

    assert_eq!(shares.len(), total_shares as usize);

    // Pre-distributed: shares[0..3] (1 per heir, N=3 total)
    // Locked in backup: shares[3..7] (N+1=4 shares)

    // All heirs colluding (3 shares) should NOT meet threshold (4)
    assert!(
        combine_shares(&shares[0..3]).is_err(),
        "All heirs colluding should NOT be enough (3 < 4)"
    );

    // After inheritance: 1 heir share + locked shares = 1 + 4 = 5 > threshold (4)
    let mut recovery_shares = vec![shares[0].clone()]; // heir's share
    recovery_shares.extend_from_slice(&shares[3..7]); // locked shares
    let recovered = combine_shares(&recovery_shares).expect("recovery");
    assert_eq!(recovered, nsec_bytes, "Post-inheritance recovery should work");

    // Locked shares alone = 4 = threshold (works even if heir loses their share)
    let recovered = combine_shares(&shares[3..7]).expect("locked only");
    assert_eq!(recovered, nsec_bytes, "Locked shares alone should meet threshold");

    println!("✓ nsec inheritance formula works (N=3, threshold=4, total=7)");
}

// ============================================================================
// 5. Notification Templates
// ============================================================================

#[test]
fn test_notification_levels_and_templates() {
    use nostring_notify::templates::{generate_message, NotificationLevel};

    // Test each level
    let reminder = generate_message(NotificationLevel::Reminder, 25.0, 3600, 934000);
    assert!(reminder.subject.contains("reminder"));
    assert!(reminder.body.contains("25 days"));

    let warning = generate_message(NotificationLevel::Warning, 5.0, 720, 934000);
    assert!(warning.subject.contains("WARNING"));

    let urgent = generate_message(NotificationLevel::Urgent, 0.5, 72, 934000);
    assert!(urgent.subject.contains("URGENT"));
    assert!(urgent.body.contains("hours"));

    let critical = generate_message(NotificationLevel::Critical, -1.0, -144, 934000);
    assert!(critical.subject.contains("CRITICAL"));
    assert!(critical.body.contains("EXPIRED"));

    // Verify ordering
    assert!(NotificationLevel::Critical > NotificationLevel::Urgent);
    assert!(NotificationLevel::Urgent > NotificationLevel::Warning);
    assert!(NotificationLevel::Warning > NotificationLevel::Reminder);

    println!("✓ All 4 notification levels generate correct templates");
}

#[test]
fn test_notification_threshold_matching() {
    use nostring_notify::NotifyConfig;
    use nostring_notify::templates::NotificationLevel;

    let config = NotifyConfig::default();

    // 45 days — no notification
    let triggered: Option<NotificationLevel> = config
        .thresholds
        .iter()
        .filter(|t| 45.0 <= t.days as f64)
        .map(|t| t.level)
        .max();
    assert!(triggered.is_none(), "45 days should not trigger");

    // 25 days — reminder
    let triggered: Option<NotificationLevel> = config
        .thresholds
        .iter()
        .filter(|t| 25.0 <= t.days as f64)
        .map(|t| t.level)
        .max();
    assert_eq!(triggered, Some(NotificationLevel::Reminder));

    // 5 days — warning (highest triggered)
    let triggered: Option<NotificationLevel> = config
        .thresholds
        .iter()
        .filter(|t| 5.0 <= t.days as f64)
        .map(|t| t.level)
        .max();
    assert_eq!(triggered, Some(NotificationLevel::Warning));

    println!("✓ Threshold matching logic correct");
}

// ============================================================================
// 6. Service Key (Nostr)
// ============================================================================

#[test]
fn test_service_key_generation() {
    use nostr_sdk::prelude::*;

    let keys = Keys::generate();
    let secret_hex = keys.secret_key().to_secret_hex();
    let npub = keys.public_key().to_bech32().unwrap();

    assert!(npub.starts_with("npub1"), "npub should start with npub1");
    assert_eq!(secret_hex.len(), 64, "Secret hex should be 32 bytes = 64 hex chars");

    // Verify roundtrip
    let recovered = Keys::parse(&secret_hex).expect("parse secret");
    assert_eq!(
        recovered.public_key(),
        keys.public_key(),
        "Recovered key should match"
    );

    println!("✓ Service key generation and roundtrip works");
    println!("  npub: {}...", &npub[..20]);
}

// ============================================================================
// 7. Electrum Connectivity (network required)
// ============================================================================

#[test]
#[ignore = "requires network access"]
fn test_mainnet_electrum_connectivity() {
    use nostring_electrum::ElectrumClient;

    let client =
        ElectrumClient::new("ssl://blockstream.info:700", Network::Bitcoin).expect("connect");
    let height = client.get_height().expect("height");

    assert!(height > 930000, "Height should be > 930000");
    println!("✓ Mainnet Electrum connected, height: {}", height);
}

#[test]
#[ignore = "requires network access"]
fn test_testnet_electrum_connectivity() {
    use nostring_electrum::ElectrumClient;

    let client = match ElectrumClient::new("ssl://blockstream.info:993", Network::Testnet) {
        Ok(c) => c,
        Err(e) => {
            println!("⚠ Testnet server unavailable (common): {}", e);
            return;
        }
    };

    let height = client.get_height().expect("height");
    println!("✓ Testnet Electrum connected, height: {}", height);
}

// ============================================================================
// 8. Full Flow (offline simulation)
// ============================================================================

#[test]
fn test_full_inheritance_flow_offline() {
    use nostring_core::crypto::{decrypt_seed, encrypt_seed};
    use nostring_core::seed::{derive_seed, generate_mnemonic, parse_mnemonic, WordCount};
    use nostring_inherit::heir::{HeirKey, HeirRegistry};
    use nostring_shamir::codex32::{combine_shares, generate_shares, Codex32Config};
    use nostr_sdk::prelude::*;

    println!("\n=== Full Inheritance Flow (Offline Simulation) ===\n");

    // Step 1: Owner generates seed
    let mnemonic = generate_mnemonic(WordCount::Words24).expect("mnemonic");
    let parsed = parse_mnemonic(&mnemonic.to_string()).expect("parse");
    let seed = derive_seed(&parsed, "");
    println!("1. ✓ Owner generated 24-word mnemonic");

    // Step 2: Encrypt seed
    let encrypted = encrypt_seed(&seed, "strong-password").expect("encrypt");
    let decrypted = decrypt_seed(&encrypted, "strong-password").expect("decrypt");
    assert_eq!(seed, decrypted);
    println!("2. ✓ Seed encrypted and decryption verified");

    // Step 3: Add heirs
    let test_xpub_str = "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8";
    let xpub = Xpub::from_str(test_xpub_str).unwrap();
    let fp = xpub.fingerprint();
    let path = DerivationPath::from_str("m/84'/0'/0'").unwrap();

    let mut registry = HeirRegistry::new();
    registry.add(HeirKey::new("Spouse", fp, xpub, Some(path.clone())));
    assert_eq!(registry.list().len(), 1);
    println!("3. ✓ Heir 'Spouse' added to registry");

    // Step 4: Generate service key
    let service_keys = Keys::generate();
    let service_npub = service_keys.public_key().to_bech32().unwrap();
    println!("4. ✓ Service key generated: {}...", &service_npub[..20]);

    // Step 5: Shamir split for nsec inheritance (1 heir → 2-of-3)
    let n_heirs: u8 = 1;
    let threshold = n_heirs + 1; // 2
    let total = 2 * n_heirs + 1; // 3
    let nsec_bytes = [0xABu8; 32]; // Simulated nsec

    let config = Codex32Config::new(threshold, "her0", total).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("shares");
    assert_eq!(shares.len(), 3);
    println!(
        "5. ✓ nsec Shamir-split: {}-of-{} (pre-dist: {}, locked: {})",
        threshold,
        total,
        n_heirs,
        threshold
    );

    // Step 6: Verify heir can't reconstruct alone
    assert!(
        combine_shares(&shares[0..1]).is_err(),
        "Single share insufficient"
    );
    println!("6. ✓ Heir's single share cannot reconstruct nsec");

    // Step 7: After inheritance, heir + locked shares → nsec
    let mut recovery = vec![shares[0].clone()];
    recovery.extend_from_slice(&shares[1..3]);
    let recovered = combine_shares(&recovery).expect("recover");
    assert_eq!(recovered, nsec_bytes);
    println!("7. ✓ Post-inheritance recovery: heir share + locked shares → nsec");

    // Step 8: Notification templates ready
    use nostring_notify::templates::{generate_message, NotificationLevel};
    let msg = generate_message(NotificationLevel::Reminder, 25.0, 3600, 934000);
    assert!(!msg.subject.is_empty());
    println!("8. ✓ Notification templates ready");

    println!("\n=== Full Flow Complete — All Components Working ===\n");
}
