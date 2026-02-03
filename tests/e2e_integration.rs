//! End-to-end integration test for NoString.
//!
//! Tests the complete inheritance flow without requiring a live Bitcoin
//! network or real funds. Exercises:
//!
//! 1. Seed generation / xpub derivation
//! 2. Heir key management
//! 3. Policy creation (miniscript descriptor)
//! 4. Shamir secret sharing (Codex32) with real nsec
//! 5. Notification template generation
//! 6. Service key creation
//! 7. Full end-to-end flow tying all components together
//!
//! Run with: cargo test --test e2e_integration
//! For network tests: cargo test --test e2e_integration -- --ignored

use bitcoin::bip32::{DerivationPath, Xpub};
use bitcoin::Network;
use miniscript::descriptor::DescriptorPublicKey;
use std::str::FromStr;

// ============================================================================
// GAP 1: Real miniscript witness tests for spend detection
// ============================================================================

/// Build a real WSH inheritance descriptor using actual secp256k1 keypairs,
/// construct correct witnesses for owner and heir spending paths,
/// and verify that `analyze_witness()` correctly detects each path.
#[test]
fn test_real_miniscript_owner_witness_detection() {
    use bitcoin::Witness;
    use miniscript::policy::Concrete;
    use miniscript::{Descriptor, Miniscript, Segwitv0};
    use nostring_watch::{analyze_witness, DetectionMethod, SpendType};
    use secp256k1::{Message, Secp256k1, SecretKey};
    use std::sync::Arc;

    let secp = Secp256k1::new();

    // Generate real keypairs
    let owner_sk = SecretKey::new(&mut rand::thread_rng());
    let heir_sk = SecretKey::new(&mut rand::thread_rng());
    let owner_pk = bitcoin::PublicKey::new(secp256k1::PublicKey::from_secret_key(&secp, &owner_sk));
    let heir_pk = bitcoin::PublicKey::new(secp256k1::PublicKey::from_secret_key(&secp, &heir_sk));

    // Build the policy: or_d(pk(owner), and_v(v:pk(heir), older(26280)))
    let policy: Concrete<bitcoin::PublicKey> = Concrete::Or(vec![
        (1, Arc::new(Concrete::Key(owner_pk))),
        (
            1,
            Arc::new(Concrete::And(vec![
                Arc::new(Concrete::Key(heir_pk)),
                Arc::new(Concrete::Older(miniscript::RelLockTime::from_height(26280))),
            ])),
        ),
    ]);

    // Compile to miniscript and wrap in WSH
    let ms: Miniscript<bitcoin::PublicKey, Segwitv0> = policy.compile().expect("compile policy");
    let descriptor = Descriptor::new_wsh(ms.clone()).expect("wsh descriptor");
    let desc_str = descriptor.to_string();
    assert!(
        desc_str.starts_with("wsh("),
        "Should be WSH descriptor: {}",
        desc_str
    );

    // Extract the witness script (the raw script inside the WSH)
    let witness_script = ms.encode();

    // Create a dummy message to sign (simulates a sighash)
    let dummy_sighash = [0x42u8; 32];
    let msg = Message::from_digest(dummy_sighash);

    // Sign with owner key (ECDSA, DER-encoded + SIGHASH_ALL byte)
    let owner_sig_raw = secp.sign_ecdsa(&msg, &owner_sk);
    let mut owner_sig = owner_sig_raw.serialize_der().to_vec();
    owner_sig.push(0x01); // SIGHASH_ALL

    // Owner spending path witness: [<sig_owner>, <witness_script>]
    // The compiled script is: <OWNER> CHECKSIG IFDUP NOTIF <HEIR> CHECKSIGVERIFY <N> CSV ENDIF
    // Owner path: CHECKSIG succeeds with real sig → IFDUP duplicates → NOTIF skips heir block
    let mut owner_witness = Witness::new();
    owner_witness.push(&owner_sig);
    owner_witness.push(witness_script.as_bytes());

    let analysis = analyze_witness(&owner_witness);
    assert_eq!(
        analysis.spend_type,
        SpendType::OwnerCheckin,
        "Real owner witness should be detected as OwnerCheckin"
    );
    assert_eq!(analysis.method, DetectionMethod::WitnessAnalysis);
    assert_eq!(
        analysis.witness_stack_size, 1,
        "Owner path has 1 stack item (signature only, excluding witness script)"
    );
    assert!(
        analysis.confidence >= 0.9,
        "Confidence should be high for real DER signature: {}",
        analysis.confidence
    );

    println!(
        "✓ Gap 1: Real miniscript owner witness correctly detected (confidence: {:.2})",
        analysis.confidence
    );
}

#[test]
fn test_real_miniscript_heir_witness_detection() {
    use bitcoin::Witness;
    use miniscript::policy::Concrete;
    use miniscript::{Descriptor, Miniscript, Segwitv0};
    use nostring_watch::{analyze_witness, DetectionMethod, SpendType};
    use secp256k1::{Message, Secp256k1, SecretKey};
    use std::sync::Arc;

    let secp = Secp256k1::new();

    // Generate real keypairs
    let owner_sk = SecretKey::new(&mut rand::thread_rng());
    let heir_sk = SecretKey::new(&mut rand::thread_rng());
    let owner_pk = bitcoin::PublicKey::new(secp256k1::PublicKey::from_secret_key(&secp, &owner_sk));
    let heir_pk = bitcoin::PublicKey::new(secp256k1::PublicKey::from_secret_key(&secp, &heir_sk));

    // Build the same policy
    let policy: Concrete<bitcoin::PublicKey> = Concrete::Or(vec![
        (1, Arc::new(Concrete::Key(owner_pk))),
        (
            1,
            Arc::new(Concrete::And(vec![
                Arc::new(Concrete::Key(heir_pk)),
                Arc::new(Concrete::Older(miniscript::RelLockTime::from_height(26280))),
            ])),
        ),
    ]);

    let ms: Miniscript<bitcoin::PublicKey, Segwitv0> = policy.compile().expect("compile policy");
    let descriptor = Descriptor::new_wsh(ms.clone()).expect("wsh descriptor");
    let witness_script = ms.encode();

    // Sign with heir key
    let dummy_sighash = [0x42u8; 32];
    let msg = Message::from_digest(dummy_sighash);
    let heir_sig_raw = secp.sign_ecdsa(&msg, &heir_sk);
    let mut heir_sig = heir_sig_raw.serialize_der().to_vec();
    heir_sig.push(0x01); // SIGHASH_ALL

    // Heir spending path witness: [<sig_heir>, <empty_dummy>, <witness_script>]
    // The heir path requires an empty dummy for the owner's CHECKSIG (which produces 0),
    // then heir's CHECKSIGVERIFY succeeds, then CSV checks the timelock.
    let mut heir_witness = Witness::new();
    heir_witness.push(&heir_sig);
    heir_witness.push(&[] as &[u8]); // empty dummy for owner CHECKSIG
    heir_witness.push(witness_script.as_bytes());

    let analysis = analyze_witness(&heir_witness);
    assert_eq!(
        analysis.spend_type,
        SpendType::HeirClaim,
        "Real heir witness should be detected as HeirClaim"
    );
    assert_eq!(analysis.method, DetectionMethod::WitnessAnalysis);
    assert_eq!(
        analysis.witness_stack_size, 2,
        "Heir path has 2 stack items (sig + empty dummy, excluding witness script)"
    );
    assert!(
        analysis.confidence >= 0.85,
        "Confidence should be good for heir path: {}",
        analysis.confidence
    );

    // Verify the descriptor string looks correct
    let desc_str = descriptor.to_string();
    println!("  Compiled descriptor: {}", desc_str);
    println!("  Witness script: {} bytes", witness_script.len());

    println!(
        "✓ Gap 1: Real miniscript heir witness correctly detected (confidence: {:.2})",
        analysis.confidence
    );
}

#[test]
fn test_real_miniscript_combined_witness_with_timing() {
    use bitcoin::Witness;
    use miniscript::policy::Concrete;
    use miniscript::{Miniscript, Segwitv0};
    use nostring_watch::{analyze_spend, DetectionMethod, SpendType};
    use secp256k1::{Message, Secp256k1, SecretKey};
    use std::sync::Arc;

    let secp = Secp256k1::new();

    let owner_sk = SecretKey::new(&mut rand::thread_rng());
    let heir_sk = SecretKey::new(&mut rand::thread_rng());
    let owner_pk = bitcoin::PublicKey::new(secp256k1::PublicKey::from_secret_key(&secp, &owner_sk));
    let heir_pk = bitcoin::PublicKey::new(secp256k1::PublicKey::from_secret_key(&secp, &heir_sk));

    let policy: Concrete<bitcoin::PublicKey> = Concrete::Or(vec![
        (1, Arc::new(Concrete::Key(owner_pk))),
        (
            1,
            Arc::new(Concrete::And(vec![
                Arc::new(Concrete::Key(heir_pk)),
                Arc::new(Concrete::Older(miniscript::RelLockTime::from_height(26280))),
            ])),
        ),
    ]);

    let ms: Miniscript<bitcoin::PublicKey, Segwitv0> = policy.compile().expect("compile");
    let witness_script = ms.encode();

    let dummy_sighash = [0x42u8; 32];
    let msg = Message::from_digest(dummy_sighash);
    let owner_sig_raw = secp.sign_ecdsa(&msg, &owner_sk);
    let mut owner_sig = owner_sig_raw.serialize_der().to_vec();
    owner_sig.push(0x01);

    // Owner witness with timing confirmation (spend before timelock expiry)
    let mut owner_witness = Witness::new();
    owner_witness.push(&owner_sig);
    owner_witness.push(witness_script.as_bytes());

    let analysis = analyze_spend(&owner_witness, 810_000, 800_000, 26_280);
    assert_eq!(analysis.spend_type, SpendType::OwnerCheckin);
    // Both witness and timing agree → very high confidence
    assert!(
        analysis.confidence >= 0.95,
        "Owner with timing confirmation should be very high confidence: {}",
        analysis.confidence
    );

    // Heir witness post-expiry
    let heir_sig_raw = secp.sign_ecdsa(&msg, &heir_sk);
    let mut heir_sig = heir_sig_raw.serialize_der().to_vec();
    heir_sig.push(0x01);

    let mut heir_witness = Witness::new();
    heir_witness.push(&heir_sig);
    heir_witness.push(&[] as &[u8]);
    heir_witness.push(witness_script.as_bytes());

    let analysis = analyze_spend(&heir_witness, 830_000, 800_000, 26_280);
    assert_eq!(analysis.spend_type, SpendType::HeirClaim);
    assert_eq!(analysis.method, DetectionMethod::WitnessAnalysis);

    println!("✓ Gap 1: Combined witness + timing analysis with real miniscript witnesses verified");
}

// ============================================================================
// GAP 2: Command-level revocation tests (real crypto flow)
// ============================================================================

/// Test the full revocation flow with real Nostr keys and Shamir splitting.
/// This exercises the actual cryptographic operations, not just DB upserts.
#[test]
fn test_revocation_full_crypto_flow() {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::{combine_shares, generate_shares, parse_share, Codex32Config};

    // === Step 1: Generate original Nostr identity ===
    let original_keys = Keys::generate();
    let original_npub = original_keys.public_key().to_bech32().unwrap();
    let original_nsec_bytes = original_keys.secret_key().as_secret_bytes().to_vec();

    // === Step 2: Initial Shamir split (2 heirs → 3-of-5) ===
    let n_heirs: u8 = 2;
    let threshold = n_heirs + 1; // 3
    let total = 2 * n_heirs + 1; // 5

    let config_v1 = Codex32Config::new(threshold, "rev0", total).expect("config");
    let shares_v1 = generate_shares(&original_nsec_bytes, &config_v1).expect("shares");
    assert_eq!(shares_v1.len(), 5);

    // Pre-distributed to heirs: shares[0], shares[1]
    let pre_dist_v1: Vec<String> = shares_v1[..2].iter().map(|s| s.encoded.clone()).collect();
    // Locked in backup: shares[2], shares[3], shares[4]
    let _locked_v1: Vec<String> = shares_v1[2..].iter().map(|s| s.encoded.clone()).collect();

    // Verify recovery works with heir 0 + all locked shares
    let mut recovery_v1 = vec![parse_share(&pre_dist_v1[0]).expect("parse")];
    for s in &_locked_v1 {
        recovery_v1.push(parse_share(s).expect("parse"));
    }
    let recovered_v1 = combine_shares(&recovery_v1).expect("combine");
    let recovered_keys_v1 = Keys::parse(&hex::encode(&recovered_v1)).expect("key");
    assert_eq!(
        recovered_keys_v1.public_key().to_bech32().unwrap(),
        original_npub,
        "Pre-revocation: recovery should work"
    );

    // === Step 3: Simulate revocation — clear locked shares ===
    // In the real app, this is config_delete("nsec_locked_shares") + config_delete("nsec_owner_npub")
    // After revocation, the locked shares vector is empty (deleted from DB).
    // We drop `locked_v1` to simulate this — heirs only have their pre-distributed shares.

    // === Step 4: Verify old shares can't reconstruct without locked shares ===
    // Heirs only have their pre-distributed shares (2 shares, but threshold is 3)
    let heir_only: Vec<_> = pre_dist_v1
        .iter()
        .map(|s| parse_share(s).expect("parse"))
        .collect();
    assert!(
        combine_shares(&heir_only).is_err(),
        "Post-revocation: heirs' shares alone (2 < threshold 3) should fail"
    );

    // === Step 5: Re-split with NEW Nostr identity ===
    let new_keys = Keys::generate();
    let new_npub = new_keys.public_key().to_bech32().unwrap();
    let new_nsec_bytes = new_keys.secret_key().as_secret_bytes().to_vec();
    assert_ne!(
        original_npub, new_npub,
        "New identity should differ from original"
    );

    let config_v2 = Codex32Config::new(threshold, "rev2", total).expect("config");
    let shares_v2 = generate_shares(&new_nsec_bytes, &config_v2).expect("shares");

    let pre_dist_v2: Vec<String> = shares_v2[..2].iter().map(|s| s.encoded.clone()).collect();
    let locked_v2: Vec<String> = shares_v2[2..].iter().map(|s| s.encoded.clone()).collect();

    // === Step 6: Old pre-distributed shares DON'T work with new locked shares ===
    // Old shares were from a different Shamir split (different polynomial + different secret)
    // Mixing shares from different splits produces garbage, not a valid key.
    let mut cross_mix = vec![parse_share(&pre_dist_v1[0]).expect("parse")];
    for s in &locked_v2 {
        cross_mix.push(parse_share(s).expect("parse"));
    }
    // This will either error (different identifiers) or produce wrong bytes
    let cross_result = combine_shares(&cross_mix);
    match cross_result {
        Err(_) => {
            // Different identifiers prevent combination — this is the expected secure behavior
            println!("  Cross-split combination correctly rejected (different identifiers)");
        }
        Ok(wrong_bytes) => {
            // If it somehow combines, the bytes should NOT match either key
            assert_ne!(
                wrong_bytes, original_nsec_bytes,
                "Cross-split must NOT recover original identity"
            );
            assert_ne!(
                wrong_bytes, new_nsec_bytes,
                "Cross-split must NOT recover new identity"
            );
            println!("  Cross-split produced garbage bytes (expected)");
        }
    }

    // === Step 7: New pre-distributed shares DO work with new locked shares ===
    let mut recovery_v2 = vec![parse_share(&pre_dist_v2[0]).expect("parse")];
    for s in &locked_v2 {
        recovery_v2.push(parse_share(s).expect("parse"));
    }
    let recovered_v2 = combine_shares(&recovery_v2).expect("combine");
    let recovered_keys_v2 = Keys::parse(&hex::encode(&recovered_v2)).expect("key");
    assert_eq!(
        recovered_keys_v2.public_key().to_bech32().unwrap(),
        new_npub,
        "Post-resplit: new shares should recover new identity"
    );

    // Verify all heirs can recover the new identity
    for (heir_idx, pre_share) in pre_dist_v2.iter().enumerate().take(n_heirs as usize) {
        let mut heir_recovery = vec![parse_share(pre_share).expect("parse")];
        for s in &locked_v2 {
            heir_recovery.push(parse_share(s).expect("parse"));
        }
        let rec = combine_shares(&heir_recovery).expect("combine");
        let rec_keys = Keys::parse(&hex::encode(&rec)).expect("key");
        assert_eq!(
            rec_keys.public_key().to_bech32().unwrap(),
            new_npub,
            "Heir {} should recover new identity",
            heir_idx
        );
    }

    println!("✓ Gap 2: Full revocation crypto flow verified:");
    println!("  - Original identity split and recoverable");
    println!("  - Revocation clears locked shares");
    println!("  - Old heirs' shares alone insufficient (below threshold)");
    println!("  - Cross-split mixing doesn't recover either identity");
    println!("  - Re-split with new identity fully functional");
}

/// Test revocation with same key re-split (key rotation of shares, not identity).
#[test]
fn test_revocation_same_key_resplit() {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::{combine_shares, generate_shares, parse_share, Codex32Config};

    let keys = Keys::generate();
    let npub = keys.public_key().to_bech32().unwrap();
    let nsec_bytes = keys.secret_key().as_secret_bytes().to_vec();

    // First split
    let config_v1 = Codex32Config::new(2, "sp0a", 3).expect("config");
    let shares_v1 = generate_shares(&nsec_bytes, &config_v1).expect("shares");
    let pre_dist_v1 = shares_v1[0].encoded.clone();
    let _locked_v1: Vec<String> = shares_v1[1..].iter().map(|s| s.encoded.clone()).collect();
    // locked_v1 is intentionally unused — simulates revocation (cleared from DB)

    // Revoke and re-split SAME key with different polynomial
    let config_v2 = Codex32Config::new(2, "sp0c", 3).expect("config");
    let shares_v2 = generate_shares(&nsec_bytes, &config_v2).expect("shares");
    let pre_dist_v2 = shares_v2[0].encoded.clone();
    let locked_v2: Vec<String> = shares_v2[1..].iter().map(|s| s.encoded.clone()).collect();

    // Old pre-distributed share + new locked shares should NOT work
    // (different identifier → different split)
    let mut cross_mix = vec![parse_share(&pre_dist_v1).expect("parse")];
    for s in &locked_v2 {
        cross_mix.push(parse_share(s).expect("parse"));
    }
    let cross_result = combine_shares(&cross_mix);
    match cross_result {
        Err(_) => {
            // Expected: different identifiers reject combination
        }
        Ok(bytes) => {
            // If combined, verify it's NOT the right key
            // (different polynomials produce different interpolation)
            // With same secret but different random shares, interpolation from
            // mixed sets should produce wrong result
            if bytes == nsec_bytes {
                // This is mathematically possible only if the shares happen to
                // lie on compatible polynomials, which is astronomically unlikely
                panic!("Cross-split should not recover the secret");
            }
        }
    }

    // New pre-distributed + new locked shares SHOULD work
    let mut recovery = vec![parse_share(&pre_dist_v2).expect("parse")];
    for s in &locked_v2 {
        recovery.push(parse_share(s).expect("parse"));
    }
    let recovered = combine_shares(&recovery).expect("combine");
    let rec_keys = Keys::parse(&hex::encode(&recovered)).expect("key");
    assert_eq!(rec_keys.public_key().to_bech32().unwrap(), npub);

    println!("✓ Gap 2: Same-key re-split revocation verified");
}

// ============================================================================
// GAP 3: Full delivery flow integration test
// ============================================================================

/// Test the complete heir delivery flow:
/// setup → Shamir split → descriptor backup → delivery message → parse → recover.
#[test]
fn test_full_heir_delivery_flow() {
    use nostr_sdk::prelude::*;
    use nostring_notify::templates::generate_heir_delivery_message;
    use nostring_shamir::codex32::{combine_shares, generate_shares, parse_share, Codex32Config};

    // === Step 1: Setup — generate owner Nostr key ===
    let owner_keys = Keys::generate();
    let original_npub = owner_keys.public_key().to_bech32().unwrap();
    let original_nsec = owner_keys.secret_key().to_bech32().unwrap();
    let nsec_bytes = owner_keys.secret_key().as_secret_bytes().to_vec();

    // === Step 2: Create Shamir split (1 heir → 2-of-3) ===
    let config = Codex32Config::new(2, "dlvr", 3).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("shares");

    let pre_distributed_share = shares[0].encoded.clone(); // given to heir
    let locked_shares: Vec<String> = shares[1..].iter().map(|s| s.encoded.clone()).collect();

    // === Step 3: Generate descriptor backup data as JSON ===
    let descriptor_str = "wsh(or_d(pk([owner/84h/0h/0h]xpub6ABC.../0/*),and_v(v:pk([heir/84h/0h/0h]xpub6DEF.../0/*),older(26280))))";

    let backup_data = serde_json::json!({
        "version": 1,
        "descriptor": descriptor_str,
        "network": "bitcoin",
        "timelock_blocks": 26280,
        "owner_npub": original_npub,
        "heirs": [
            {
                "label": "Spouse",
                "xpub": "xpub6DEF...",
                "timelock_months": 6
            }
        ],
        "locked_shares": locked_shares
    });
    let backup_json = serde_json::to_string_pretty(&backup_data).expect("serialize backup");

    // === Step 4: Generate heir delivery message ===
    let delivery_msg = generate_heir_delivery_message("Spouse", &backup_json);

    // Verify message structure
    assert_eq!(
        delivery_msg.level,
        nostring_notify::templates::NotificationLevel::Critical
    );
    assert!(delivery_msg.subject.contains("Inheritance"));
    assert!(delivery_msg.body.contains("Spouse"));
    assert!(delivery_msg
        .body
        .contains("BEGIN NOSTRING DESCRIPTOR BACKUP"));
    assert!(delivery_msg.body.contains("END NOSTRING DESCRIPTOR BACKUP"));
    assert!(delivery_msg.body.contains(&backup_json));

    // === Step 5: Parse the delivery message to extract backup ===
    let body = &delivery_msg.body;
    let begin_marker = "=== BEGIN NOSTRING DESCRIPTOR BACKUP ===";
    let end_marker = "=== END NOSTRING DESCRIPTOR BACKUP ===";

    let begin_pos = body.find(begin_marker).expect("should find begin marker");
    let end_pos = body.find(end_marker).expect("should find end marker");
    let extracted_json = body[begin_pos + begin_marker.len()..end_pos].trim();

    // Parse the extracted JSON
    let parsed_backup: serde_json::Value =
        serde_json::from_str(extracted_json).expect("extracted JSON should parse");

    // Verify backup contents
    assert_eq!(
        parsed_backup["descriptor"].as_str().unwrap(),
        descriptor_str
    );
    assert_eq!(parsed_backup["network"].as_str().unwrap(), "bitcoin");
    assert_eq!(parsed_backup["timelock_blocks"].as_u64().unwrap(), 26280);
    assert_eq!(parsed_backup["owner_npub"].as_str().unwrap(), original_npub);

    // === Step 6: Extract locked shares from parsed backup ===
    let extracted_locked: Vec<String> = parsed_backup["locked_shares"]
        .as_array()
        .expect("locked_shares should be array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert_eq!(
        extracted_locked.len(),
        locked_shares.len(),
        "Should extract correct number of locked shares"
    );
    assert_eq!(
        extracted_locked, locked_shares,
        "Extracted locked shares should match originals"
    );

    // === Step 7: Combine pre-distributed share + locked shares → recover nsec ===
    let mut recovery_shares = vec![parse_share(&pre_distributed_share).expect("parse heir share")];
    for s in &extracted_locked {
        recovery_shares.push(parse_share(s).expect("parse locked share"));
    }

    let recovered_bytes = combine_shares(&recovery_shares).expect("combine shares");
    assert_eq!(
        recovered_bytes, nsec_bytes,
        "Recovered bytes must match original"
    );

    // Verify recovered key matches original identity
    let recovered_keys = Keys::parse(&hex::encode(&recovered_bytes)).expect("parse recovered key");
    let recovered_npub = recovered_keys.public_key().to_bech32().unwrap();
    let recovered_nsec = recovered_keys.secret_key().to_bech32().unwrap();

    assert_eq!(recovered_npub, original_npub, "Recovered npub must match");
    assert_eq!(recovered_nsec, original_nsec, "Recovered nsec must match");

    println!("✓ Gap 3: Full heir delivery flow verified:");
    println!("  1. Owner key generated: {}...", &original_npub[..25]);
    println!("  2. Shamir split: 2-of-3 (1 pre-distributed, 2 locked)");
    println!(
        "  3. Descriptor backup JSON generated ({} bytes)",
        backup_json.len()
    );
    println!(
        "  4. Delivery message generated ({} chars)",
        delivery_msg.body.len()
    );
    println!("  5. Backup extracted from delivery message body");
    println!(
        "  6. {} locked shares extracted from backup",
        extracted_locked.len()
    );
    println!("  7. nsec recovered and identity verified ✓");
}

/// Test delivery flow with multiple heirs (each gets their own share).
#[test]
fn test_delivery_flow_multiple_heirs() {
    use nostr_sdk::prelude::*;
    use nostring_notify::templates::generate_heir_delivery_message;
    use nostring_shamir::codex32::{combine_shares, generate_shares, parse_share, Codex32Config};

    let owner_keys = Keys::generate();
    let original_npub = owner_keys.public_key().to_bech32().unwrap();
    let nsec_bytes = owner_keys.secret_key().as_secret_bytes().to_vec();

    // 3 heirs → 4-of-7 split
    let n_heirs: u8 = 3;
    let threshold = n_heirs + 1; // 4
    let total = 2 * n_heirs + 1; // 7

    let config = Codex32Config::new(threshold, "mhrs", total).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("shares");

    let heir_shares: Vec<String> = shares[..n_heirs as usize]
        .iter()
        .map(|s| s.encoded.clone())
        .collect();
    let locked_shares: Vec<String> = shares[n_heirs as usize..]
        .iter()
        .map(|s| s.encoded.clone())
        .collect();

    let heir_labels = ["Spouse", "Child-1", "Child-2"];

    // Generate a delivery message for each heir
    for (heir_idx, label) in heir_labels.iter().enumerate() {
        let backup_data = serde_json::json!({
            "version": 1,
            "descriptor": "wsh(...)",
            "owner_npub": original_npub,
            "locked_shares": locked_shares
        });
        let backup_json = serde_json::to_string(&backup_data).unwrap();
        let msg = generate_heir_delivery_message(label, &backup_json);

        // Parse and extract
        let begin = msg.body.find("=== BEGIN").unwrap();
        let end = msg.body.find("=== END").unwrap();
        let json_str =
            msg.body[begin + "=== BEGIN NOSTRING DESCRIPTOR BACKUP ===".len()..end].trim();
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        let extracted_locked: Vec<String> = parsed["locked_shares"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();

        // Each heir can recover using their unique share + locked shares
        let mut recovery = vec![parse_share(&heir_shares[heir_idx]).expect("parse")];
        for s in &extracted_locked {
            recovery.push(parse_share(s).expect("parse"));
        }

        let recovered = combine_shares(&recovery).expect("combine");
        let rec_keys = Keys::parse(&hex::encode(&recovered)).expect("key");
        assert_eq!(
            rec_keys.public_key().to_bech32().unwrap(),
            original_npub,
            "Heir {} ({}) should recover owner identity",
            heir_idx,
            label
        );
    }

    // Verify heirs colluding WITHOUT locked shares cannot recover
    let colluding: Vec<_> = heir_shares
        .iter()
        .map(|s| parse_share(s).expect("parse"))
        .collect();
    assert!(
        combine_shares(&colluding).is_err(),
        "All heirs colluding (3 < threshold 4) should fail"
    );

    println!("✓ Gap 3: Multi-heir delivery flow verified (3 heirs, each recovers independently)");
}

// ============================================================================
// 9. Descriptor Backup Round-Trip
// ============================================================================

/// Test that a descriptor backup file can be generated, serialized to text,
/// parsed back, and the locked shares extracted and combined with a pre-distributed
/// share to recover the nsec — end-to-end file round-trip.
#[test]
fn test_descriptor_backup_roundtrip() {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::{combine_shares, generate_shares, parse_share, Codex32Config};

    // Setup: owner with 2 heirs
    let owner_keys = Keys::generate();
    let original_npub = owner_keys.public_key().to_bech32().unwrap();
    let nsec_bytes = owner_keys.secret_key().as_secret_bytes().to_vec();

    let n_heirs: u8 = 2;
    let threshold = n_heirs + 1; // 3
    let total = 2 * n_heirs + 1; // 5

    let config = Codex32Config::new(threshold, "rcvy", total).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("shares");
    assert_eq!(shares.len(), 5);

    // Pre-distributed: shares[0], shares[1] (one per heir)
    // Locked: shares[2], shares[3], shares[4]
    let pre_distributed: Vec<String> = shares[..2].iter().map(|s| s.encoded.clone()).collect();
    let locked: Vec<String> = shares[2..].iter().map(|s| s.encoded.clone()).collect();

    // === Simulate descriptor backup file generation ===
    let descriptor_str = "wsh(or_d(pk([73c5da0a/84h/0h/0h]xpub6ABC.../0/*),and_v(v:pk([b2e5c4d1/84h/0h/0h]xpub6DEF.../0/*),older(26280))))";
    let backup_content = format!(
        r#"# NoString Descriptor Backup
# Generated: 2026-02-03T09:00:00Z

## Descriptor
{descriptor}

## Details
Network: bitcoin
Timelock: 26280 blocks (~182 days)

## Heirs
- Spouse: xpub6DEF... (6 months)
- Sibling: xpub6GHI... (6 months)

## Nostr Identity Inheritance
Owner npub: {npub}

### Locked Shares
{locked_shares}

### Heir Recovery Instructions
1. Download NoString
2. Choose "Recover a Loved One's Identity"
3. Enter YOUR pre-distributed share + ALL locked shares above
"#,
        descriptor = descriptor_str,
        npub = original_npub,
        locked_shares = locked
            .iter()
            .enumerate()
            .map(|(i, s)| format!("Share {}: {}", i + 1, s))
            .collect::<Vec<_>>()
            .join("\n"),
    );

    // === Simulate heir parsing the backup file ===
    // Extract locked shares from the text
    let mut extracted_locked: Vec<String> = Vec::new();
    for line in backup_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Share ") && trimmed.contains(": ms1") {
            // Parse "Share N: ms1..." format
            if let Some(share_str) = trimmed.split(": ").nth(1) {
                extracted_locked.push(share_str.to_string());
            }
        }
    }
    assert_eq!(
        extracted_locked.len(),
        locked.len(),
        "Should extract all locked shares from backup text"
    );
    assert_eq!(
        extracted_locked, locked,
        "Extracted shares should match originals"
    );

    // Extract owner npub from backup
    let mut extracted_npub: Option<String> = None;
    for line in backup_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Owner npub: ") {
            extracted_npub = Some(trimmed.trim_start_matches("Owner npub: ").to_string());
        }
    }
    assert_eq!(
        extracted_npub.as_deref(),
        Some(original_npub.as_str()),
        "Should extract owner npub"
    );

    // === Heir recovery: combine their share + locked shares ===
    // Heir 0 recovers
    let mut recovery_strings = vec![pre_distributed[0].clone()];
    recovery_strings.extend(extracted_locked.iter().cloned());

    let parsed_recovery: Vec<_> = recovery_strings
        .iter()
        .map(|s| parse_share(s).expect("parse share"))
        .collect();

    let recovered = combine_shares(&parsed_recovery).expect("combine");
    assert_eq!(recovered, nsec_bytes, "Recovered bytes must match");

    // Verify identity
    let recovered_keys = Keys::parse(&hex::encode(&recovered)).expect("parse key");
    assert_eq!(
        recovered_keys.public_key().to_bech32().unwrap(),
        original_npub,
        "Recovered identity must match owner"
    );

    println!("✓ Descriptor backup round-trip: generate → serialize → parse → recover → verified");
}

/// Test that locked shares in JSON format (as stored in SQLite) roundtrip correctly.
#[test]
fn test_locked_shares_json_roundtrip() {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::{combine_shares, generate_shares, parse_share, Codex32Config};

    let keys = Keys::generate();
    let nsec_bytes = keys.secret_key().as_secret_bytes().to_vec();
    let original_npub = keys.public_key().to_bech32().unwrap();

    // 1 heir: 2-of-3
    let config = Codex32Config::new(2, "jsnr", 3).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("shares");

    let pre_dist = shares[0].encoded.clone();
    let locked: Vec<String> = shares[1..].iter().map(|s| s.encoded.clone()).collect();

    // Serialize to JSON (as SQLite would store it)
    let json = serde_json::to_string(&locked).expect("json encode");

    // Deserialize back
    let recovered_locked: Vec<String> = serde_json::from_str(&json).expect("json decode");
    assert_eq!(recovered_locked, locked);

    // Combine pre-distributed + deserialized locked → nsec
    let mut all_shares = vec![pre_dist];
    all_shares.extend(recovered_locked);

    let parsed: Vec<_> = all_shares
        .iter()
        .map(|s| parse_share(s).expect("parse"))
        .collect();
    let recovered = combine_shares(&parsed).expect("combine");

    let recovered_keys = Keys::parse(&hex::encode(&recovered)).expect("parse key");
    assert_eq!(
        recovered_keys.public_key().to_bech32().unwrap(),
        original_npub
    );

    println!("✓ Locked shares JSON roundtrip works");
}

// ============================================================================
// 10. Heir Recovery Flow Tests (simulating recover_nsec command)
// ============================================================================

/// Test the complete heir recovery flow as the recover_nsec command would do it:
/// parse share strings → combine → validate as Nostr key → return nsec + npub.
#[test]
fn test_heir_recovery_command_flow() {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::{combine_shares, generate_shares, parse_share, Codex32Config};
    use zeroize::Zeroize;

    let owner_keys = Keys::generate();
    let original_npub = owner_keys.public_key().to_bech32().unwrap();
    let original_nsec = owner_keys.secret_key().to_bech32().unwrap();
    let nsec_bytes = owner_keys.secret_key().as_secret_bytes().to_vec();

    // 3 heirs: 4-of-7
    let config = Codex32Config::new(4, "rcvr", 7).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("shares");

    // Simulate: heir 2 has their share + receives locked shares from descriptor backup
    let heir_share = shares[2].encoded.clone(); // heir #3's pre-distributed share
    let locked_shares: Vec<String> = shares[3..7].iter().map(|s| s.encoded.clone()).collect();

    // Simulate the recover_nsec command logic
    let mut input_shares: Vec<String> = vec![heir_share];
    input_shares.extend(locked_shares);

    // Step 1: Parse all share strings
    let parsed: Vec<_> = input_shares
        .iter()
        .map(|s| parse_share(s).expect("parse share"))
        .collect();
    assert_eq!(parsed.len(), 5, "1 heir + 4 locked = 5 shares");

    // Step 2: Combine
    let mut recovered_bytes = combine_shares(&parsed).expect("combine");

    // Step 3: Validate it's a real Nostr key
    let recovered_hex = hex::encode(&recovered_bytes);
    let recovered_keys = Keys::parse(&recovered_hex).expect("valid Nostr key");

    // Step 4: Produce nsec and npub
    let recovered_nsec = recovered_keys.secret_key().to_bech32().unwrap();
    let recovered_npub = recovered_keys.public_key().to_bech32().unwrap();

    assert_eq!(recovered_npub, original_npub, "npub must match");
    assert_eq!(recovered_nsec, original_nsec, "nsec must match");

    // Step 5: Zero intermediate bytes
    recovered_bytes.zeroize();

    println!("✓ Heir recovery command flow: parse → combine → validate → nsec+npub verified");
}

/// Test recovery fails gracefully with insufficient shares.
#[test]
fn test_heir_recovery_insufficient_shares() {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::{combine_shares, generate_shares, parse_share, Codex32Config};

    let keys = Keys::generate();
    let nsec_bytes = keys.secret_key().as_secret_bytes().to_vec();

    // 2 heirs: 3-of-5
    let config = Codex32Config::new(3, "fa9l", 5).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("shares");

    // Only 2 shares (below threshold of 3)
    let two_shares: Vec<_> = shares[0..2]
        .iter()
        .map(|s| parse_share(&s.encoded).expect("parse"))
        .collect();

    let result = combine_shares(&two_shares);
    assert!(
        result.is_err(),
        "Should fail with insufficient shares (2 < 3 threshold)"
    );

    println!("✓ Recovery correctly rejects insufficient shares");
}

/// Test recovery with only a single share fails.
#[test]
fn test_heir_recovery_single_share_rejected() {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::{combine_shares, generate_shares, parse_share, Codex32Config};

    let keys = Keys::generate();
    let nsec_bytes = keys.secret_key().as_secret_bytes().to_vec();

    let config = Codex32Config::new(2, "sng0", 3).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("shares");

    let single = vec![parse_share(&shares[0].encoded).expect("parse")];
    let result = combine_shares(&single);
    assert!(result.is_err(), "Single share must not reconstruct");

    println!("✓ Single share correctly rejected");
}

/// Test recovery works for all heir counts from 1 to 6 (covering the formula).
#[test]
fn test_heir_recovery_all_heir_counts() {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::{combine_shares, generate_shares, parse_share, Codex32Config};

    for n_heirs in 1u8..=6 {
        let keys = Keys::generate();
        let original_npub = keys.public_key().to_bech32().unwrap();
        let nsec_bytes = keys.secret_key().as_secret_bytes().to_vec();

        let threshold = n_heirs + 1;
        let total = 2 * n_heirs + 1;

        // Use different identifier per count (4 lowercase bech32 chars)
        // bech32 charset has no 'b','i','o','1' — use safe chars
        let safe_chars = ['q', 'p', 'z', 'r', 'y', 'x'];
        let id = format!(
            "n{}{}{}",
            safe_chars[n_heirs as usize % 6],
            safe_chars[(n_heirs as usize + 1) % 6],
            safe_chars[(n_heirs as usize + 2) % 6]
        );
        let config = Codex32Config::new(threshold, &id, total).expect("config");
        let shares = generate_shares(&nsec_bytes, &config).expect("shares");

        // Verify: all heirs colluding can't recover
        if n_heirs > 1 {
            let collusion: Vec<_> = shares[..n_heirs as usize]
                .iter()
                .map(|s| parse_share(&s.encoded).expect("parse"))
                .collect();
            assert!(
                combine_shares(&collusion).is_err(),
                "N={}: All {} heirs colluding must fail (need {})",
                n_heirs,
                n_heirs,
                threshold
            );
        }

        // Verify: any single heir + locked shares recovers
        for heir_idx in 0..n_heirs as usize {
            let mut recovery = vec![parse_share(&shares[heir_idx].encoded).expect("parse")];
            for locked in &shares[n_heirs as usize..] {
                recovery.push(parse_share(&locked.encoded).expect("parse"));
            }

            let recovered = combine_shares(&recovery).expect("combine");
            let recovered_keys = Keys::parse(&hex::encode(&recovered)).expect("parse key");
            assert_eq!(
                recovered_keys.public_key().to_bech32().unwrap(),
                original_npub,
                "N={}: Heir {} recovery must produce correct npub",
                n_heirs,
                heir_idx
            );
        }

        // Verify: locked shares alone recover (resilience)
        let locked_only: Vec<_> = shares[n_heirs as usize..]
            .iter()
            .map(|s| parse_share(&s.encoded).expect("parse"))
            .collect();
        let recovered = combine_shares(&locked_only).expect("locked-only combine");
        let recovered_keys = Keys::parse(&hex::encode(&recovered)).expect("parse key");
        assert_eq!(
            recovered_keys.public_key().to_bech32().unwrap(),
            original_npub,
            "N={}: Locked shares alone must recover (resilience)",
            n_heirs
        );

        println!(
            "  ✓ N={}: {}-of-{} — collusion blocked, all heirs recover, locked-only resilient",
            n_heirs, threshold, total
        );
    }

    println!("✓ All heir counts 1-6 verified end-to-end");
}

// ============================================================================
// 1. Seed + Key Derivation
// ============================================================================

#[test]
fn test_seed_generation_and_derivation() {
    use nostring_core::seed::{derive_seed, generate_mnemonic, parse_mnemonic, WordCount};

    let mnemonic = generate_mnemonic(WordCount::Words24).expect("mnemonic generation");
    let mnemonic_str = mnemonic.to_string();
    let words: Vec<&str> = mnemonic_str.split_whitespace().collect();
    assert_eq!(words.len(), 24, "Expected 24-word mnemonic");

    let parsed = parse_mnemonic(&mnemonic_str).expect("mnemonic parsing");
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

    let test_xpub_str = "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8";
    let xpub = Xpub::from_str(test_xpub_str).expect("parse xpub");
    let fp = xpub.fingerprint();
    let path = DerivationPath::from_str("m/84'/0'/0'").unwrap();

    let heir1 = HeirKey::new("Spouse", fp, xpub, Some(path.clone()));
    registry.add(heir1);

    assert_eq!(registry.list().len(), 1);
    assert!(registry.get(&fp).is_some());
    assert_eq!(registry.get(&fp).unwrap().label, "Spouse");

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

    let owner_desc = "[73c5da0a/84'/0'/0']xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8/0/*";
    let heir_desc = "[b2e5c4d1/84'/0'/0']xpub661MyMwAqRbcFW31YEwpkMuc5THy2PSt5bDMsktWQcFF8syAmRUapSCGu8ED9W6oDMSgv6Zz8idoc4a6mr8BDzTJY47LJhkJ8UB7WEGuduB/0/*";

    let owner_key: DescriptorPublicKey = owner_desc.parse().expect("parse owner key");
    let heir_key: DescriptorPublicKey = heir_desc.parse().expect("parse heir key");

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

    println!(
        "✓ Inheritance policy descriptor: {}...",
        &desc_str[..80.min(desc_str.len())]
    );
    println!("✓ Miniscript policy creation works");
}

// ============================================================================
// 4. Shamir Secret Sharing — with REAL Nostr keys
// ============================================================================

#[test]
fn test_codex32_roundtrip_real_key() {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::{combine_shares, generate_shares, Codex32Config};

    // Generate a real Nostr keypair
    let keys = Keys::generate();
    let original_npub = keys.public_key().to_bech32().unwrap();
    let secret_bytes = keys.secret_key().as_secret_bytes().to_vec();

    assert_eq!(secret_bytes.len(), 32, "Nostr secret key is 32 bytes");

    // Split the real secret key
    let config = Codex32Config::new(2, "test", 3).expect("config");
    let shares = generate_shares(&secret_bytes, &config).expect("generate");
    assert_eq!(shares.len(), 3, "Should generate 3 shares");

    // Combine any 2 shares
    let recovered = combine_shares(&shares[0..2]).expect("combine");
    assert_eq!(
        recovered, secret_bytes,
        "Recovered bytes should match original secret key"
    );

    // Verify the recovered key produces the same npub
    let recovered_hex = hex::encode(&recovered);
    let recovered_keys = Keys::parse(&recovered_hex).expect("parse recovered key");
    let recovered_npub = recovered_keys.public_key().to_bech32().unwrap();

    assert_eq!(
        original_npub, recovered_npub,
        "Recovered nsec must produce the same npub"
    );

    // Single share should NOT be enough
    assert!(
        combine_shares(&shares[0..1]).is_err(),
        "Single share should not reconstruct"
    );

    println!("✓ Codex32 Shamir roundtrip with real Nostr key works");
    println!("  Original npub:  {}...", &original_npub[..25]);
    println!("  Recovered npub: {}...", &recovered_npub[..25]);
}

#[test]
fn test_shamir_nsec_inheritance_formula_real_key() {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::{combine_shares, generate_shares, Codex32Config};

    // Generate a real Nostr identity (the owner's nsec)
    let owner_keys = Keys::generate();
    let original_npub = owner_keys.public_key().to_bech32().unwrap();
    let nsec_bytes = owner_keys.secret_key().as_secret_bytes().to_vec();

    // Test the (N+1)-of-(2N+1) formula for 3 heirs
    let n_heirs: u8 = 3;
    let threshold = n_heirs + 1; // 4
    let total_shares = 2 * n_heirs + 1; // 7

    // Use lowercase bech32 chars for Codex32 identifier
    let config = Codex32Config::new(threshold, "nsec", total_shares).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("generate");

    assert_eq!(shares.len(), total_shares as usize);

    // Pre-distributed: shares[0..3] (1 per heir, N=3 total)
    // Locked in backup: shares[3..7] (N+1=4 shares)

    // ATTACK: All heirs collude (3 shares) — must NOT meet threshold (4)
    assert!(
        combine_shares(&shares[0..3]).is_err(),
        "All heirs colluding should NOT be enough (3 < 4)"
    );

    // RECOVERY: 1 heir share + locked shares = 1 + 4 = 5 > threshold (4)
    let mut recovery_shares = vec![shares[0].clone()]; // heir's share
    recovery_shares.extend_from_slice(&shares[3..7]); // locked shares
    let recovered = combine_shares(&recovery_shares).expect("recovery");
    assert_eq!(
        recovered, nsec_bytes,
        "Post-inheritance recovery should work"
    );

    // Verify recovered nsec → same npub
    let recovered_keys = Keys::parse(&hex::encode(&recovered)).expect("parse recovered");
    assert_eq!(
        recovered_keys.public_key().to_bech32().unwrap(),
        original_npub,
        "Recovered nsec must produce original npub"
    );

    // RESILIENCE: Locked shares alone = 4 = threshold (heir lost their share)
    let recovered = combine_shares(&shares[3..7]).expect("locked only");
    let recovered_keys = Keys::parse(&hex::encode(&recovered)).expect("parse locked-only");
    assert_eq!(
        recovered_keys.public_key().to_bech32().unwrap(),
        original_npub,
        "Locked shares alone should recover correct identity"
    );

    println!("✓ nsec inheritance formula works with real Nostr key (N=3, threshold=4, total=7)");
    println!("  Owner npub: {}...", &original_npub[..25]);
}

// ============================================================================
// 5. Notification Templates
// ============================================================================

#[test]
fn test_notification_levels_and_templates() {
    use nostring_notify::templates::{generate_message, NotificationLevel};

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

    assert!(NotificationLevel::Critical > NotificationLevel::Urgent);
    assert!(NotificationLevel::Urgent > NotificationLevel::Warning);
    assert!(NotificationLevel::Warning > NotificationLevel::Reminder);

    println!("✓ All 4 notification levels generate correct templates");
}

#[test]
fn test_notification_threshold_matching() {
    use nostring_notify::templates::NotificationLevel;
    use nostring_notify::NotifyConfig;

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
    assert_eq!(
        secret_hex.len(),
        64,
        "Secret hex should be 32 bytes = 64 hex chars"
    );

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
#[ignore = "requires network access — connects to Blockstream mainnet Electrum"]
fn test_mainnet_electrum_connectivity() {
    use nostring_electrum::ElectrumClient;

    let client =
        ElectrumClient::new("ssl://blockstream.info:700", Network::Bitcoin).expect("connect");
    let height = client.get_height().expect("height");

    assert!(height > 930000, "Height should be > 930000");
    println!("✓ Mainnet Electrum connected, height: {}", height);
}

#[test]
#[ignore = "requires network access — Blockstream testnet server is frequently down"]
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
    use nostr_sdk::prelude::*;
    use nostring_core::crypto::{decrypt_seed, encrypt_seed};
    use nostring_core::seed::{derive_seed, generate_mnemonic, parse_mnemonic, WordCount};
    use nostring_inherit::heir::{HeirKey, HeirRegistry};
    use nostring_inherit::policy::{InheritancePolicy, Timelock};
    use nostring_shamir::codex32::{combine_shares, generate_shares, Codex32Config};

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

    // Step 3: Add heirs to registry
    let test_xpub_str = "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8";
    let xpub = Xpub::from_str(test_xpub_str).expect("parse xpub");
    let fp = xpub.fingerprint();
    let path = DerivationPath::from_str("m/84'/0'/0'").unwrap();

    let mut registry = HeirRegistry::new();
    registry.add(HeirKey::new("Spouse", fp, xpub, Some(path.clone())));
    assert_eq!(registry.list().len(), 1);
    println!("3. ✓ Heir 'Spouse' added to registry");

    // Step 4: Create miniscript inheritance policy
    let owner_desc = "[73c5da0a/84'/0'/0']xpub661MyMwAqRbcFW31YEwpkMuc5THy2PSt5bDMsktWQcFF8syAmRUapSCGu8ED9W6oDMSgv6Zz8idoc4a6mr8BDzTJY47LJhkJ8UB7WEGuduB/0/*";
    let heir_desc = format!("[{}/84'/0'/0']{}/0/*", fp, test_xpub_str);

    let owner_key: DescriptorPublicKey = owner_desc.parse().expect("parse owner descriptor key");
    let heir_key: DescriptorPublicKey = heir_desc.parse().expect("parse heir descriptor key");

    let policy =
        InheritancePolicy::simple(owner_key, heir_key, Timelock::six_months()).expect("policy");
    let descriptor = policy.to_wsh_descriptor().expect("compile to wsh");
    let desc_str = format!("{}", descriptor);
    assert!(desc_str.contains("wsh("));
    println!(
        "4. ✓ Inheritance policy created: {}...",
        &desc_str[..60.min(desc_str.len())]
    );

    // Step 5: Generate service key
    let service_keys = Keys::generate();
    let service_npub = service_keys.public_key().to_bech32().unwrap();
    println!("5. ✓ Service key generated: {}...", &service_npub[..20]);

    // Step 6: Shamir split for nsec inheritance with REAL nsec
    let owner_nostr_keys = Keys::generate();
    let original_npub = owner_nostr_keys.public_key().to_bech32().unwrap();
    let nsec_bytes = owner_nostr_keys.secret_key().as_secret_bytes().to_vec();

    let n_heirs: u8 = 1;
    let threshold = n_heirs + 1; // 2
    let total = 2 * n_heirs + 1; // 3

    let config = Codex32Config::new(threshold, "her0", total).expect("config");
    let shares = generate_shares(&nsec_bytes, &config).expect("shares");
    assert_eq!(shares.len(), 3);
    println!(
        "6. ✓ nsec Shamir-split: {}-of-{} (pre-dist: {}, locked: {})",
        threshold, total, n_heirs, threshold
    );

    // Step 7: Verify heir can't reconstruct alone
    assert!(
        combine_shares(&shares[0..1]).is_err(),
        "Single share insufficient"
    );
    println!("7. ✓ Heir's single share cannot reconstruct nsec");

    // Step 8: After inheritance, heir + locked shares → nsec, verify npub matches
    let mut recovery = vec![shares[0].clone()];
    recovery.extend_from_slice(&shares[1..3]);
    let recovered = combine_shares(&recovery).expect("recover");
    assert_eq!(recovered, nsec_bytes);

    let recovered_keys = Keys::parse(&hex::encode(&recovered)).expect("parse recovered nsec");
    let recovered_npub = recovered_keys.public_key().to_bech32().unwrap();
    assert_eq!(
        original_npub, recovered_npub,
        "Recovered nsec must produce same npub"
    );
    println!("8. ✓ Post-inheritance recovery: nsec → npub verified");

    // Step 9: Notification templates ready
    use nostring_notify::templates::{generate_message, NotificationLevel};
    let msg = generate_message(NotificationLevel::Warning, 5.0, 720, 934000);
    assert!(msg.subject.contains("WARNING"));
    assert!(msg.body.contains("5 days"));
    println!("9. ✓ Notification templates ready");

    println!("\n=== Full Flow Complete — All Components Verified ===\n");
}
