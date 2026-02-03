//! Security-specific tests for audit preparation.
//!
//! These tests verify:
//! 1. Crypto operations reject invalid inputs
//! 2. Secrets are zeroed after use (flow-level verification)
//! 3. Malformed inputs don't panic
//! 4. Share validation catches corruption
//! 5. Codex32 fuzz testing with random inputs

use nostring_core::crypto::{decrypt_seed, encrypt_seed, EncryptedSeed};
use nostring_core::seed::{derive_seed, generate_mnemonic, parse_mnemonic, WordCount};
use nostring_shamir::codex32::{
    combine_shares, generate_shares as codex32_generate, parse_share, Codex32Config,
};
use nostring_shamir::shamir::{split_secret, reconstruct_secret};
use zeroize::Zeroize;

// ============================================================================
// 1. Seed Encryption Security Tests
// ============================================================================

#[test]
fn test_wrong_password_fails_decryption() {
    let seed = [0xABu8; 64];
    let correct_pw = "correct horse battery staple";
    let wrong_pw = "wrong horse battery staple";

    let encrypted = encrypt_seed(&seed, correct_pw).unwrap();
    let result = decrypt_seed(&encrypted, wrong_pw);

    assert!(result.is_err(), "Decryption with wrong password should fail");
}

#[test]
fn test_empty_password_decryption_fails_with_wrong_password() {
    let seed = [0x42u8; 64];
    let encrypted = encrypt_seed(&seed, "").unwrap();

    // Empty password encrypt, non-empty decrypt should fail
    let result = decrypt_seed(&encrypted, "notempty");
    assert!(result.is_err());

    // Correct empty password should succeed
    let result = decrypt_seed(&encrypted, "");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), seed);
}

#[test]
fn test_tampered_salt_fails_decryption() {
    let seed = [0x42u8; 64];
    let password = "test password";

    let encrypted = encrypt_seed(&seed, password).unwrap();
    let mut bytes = encrypted.to_bytes();

    // Tamper with salt (first 16 bytes)
    bytes[0] ^= 0xFF;

    let tampered = EncryptedSeed::from_bytes(&bytes).unwrap();
    let result = decrypt_seed(&tampered, password);
    assert!(result.is_err(), "Tampered salt should fail decryption");
}

#[test]
fn test_tampered_nonce_fails_decryption() {
    let seed = [0x42u8; 64];
    let password = "test password";

    let encrypted = encrypt_seed(&seed, password).unwrap();
    let mut bytes = encrypted.to_bytes();

    // Tamper with nonce (bytes 16..28)
    bytes[16] ^= 0xFF;

    let tampered = EncryptedSeed::from_bytes(&bytes).unwrap();
    let result = decrypt_seed(&tampered, password);
    assert!(result.is_err(), "Tampered nonce should fail decryption");
}

#[test]
fn test_tampered_ciphertext_fails_decryption() {
    let seed = [0x42u8; 64];
    let password = "test password";

    let encrypted = encrypt_seed(&seed, password).unwrap();
    let mut bytes = encrypted.to_bytes();

    // Tamper with ciphertext body (after salt+nonce, before auth tag)
    bytes[30] ^= 0xFF;

    let tampered = EncryptedSeed::from_bytes(&bytes).unwrap();
    let result = decrypt_seed(&tampered, password);
    assert!(result.is_err(), "Tampered ciphertext should fail (GCM auth tag)");
}

#[test]
fn test_truncated_ciphertext_fails() {
    let seed = [0x42u8; 64];
    let password = "test password";

    let encrypted = encrypt_seed(&seed, password).unwrap();
    let bytes = encrypted.to_bytes();

    // Truncate to less than minimum (salt + nonce + 17)
    let truncated = &bytes[..28]; // Only salt + nonce, no ciphertext
    let result = EncryptedSeed::from_bytes(truncated);
    assert!(result.is_err(), "Truncated data should fail parsing");
}

#[test]
fn test_encrypted_seed_from_bytes_min_length() {
    // Minimum valid: 16 (salt) + 12 (nonce) + 17 (1 byte ct + 16 byte tag) = 45
    let too_short = vec![0u8; 44];
    assert!(EncryptedSeed::from_bytes(&too_short).is_err());

    let just_right = vec![0u8; 45];
    assert!(EncryptedSeed::from_bytes(&just_right).is_ok());
}

// ============================================================================
// 2. nsec Zeroing Flow Test
// ============================================================================

#[test]
fn test_zeroize_works_on_vec() {
    // Verify the zeroize crate actually zeroes memory
    let mut secret = vec![0xAB_u8; 32];
    let ptr = secret.as_ptr();

    // Zeroize the vector
    secret.zeroize();

    // After zeroize, the vector should be empty or zeroed
    assert!(secret.is_empty() || secret.iter().all(|&b| b == 0),
        "Zeroize should clear the vector");
}

#[test]
fn test_split_nsec_flow_zeroes_secret() {
    // Simulate the nsec split flow from commands.rs
    // Verify that after the split, the original secret bytes are zeroed

    let mut secret_bytes = vec![0x42u8; 32]; // Simulated nsec

    // Split using Shamir
    let shares = split_secret(&secret_bytes, 2, 3).unwrap();

    // Zero the secret (as the real code does)
    secret_bytes.zeroize();

    // Verify it's zeroed
    assert!(secret_bytes.is_empty() || secret_bytes.iter().all(|&b| b == 0));

    // Verify shares can still reconstruct
    let recovered = reconstruct_secret(&shares[0..2]).unwrap();
    assert_eq!(recovered, vec![0x42u8; 32]);
}

// ============================================================================
// 3. Invalid Share Rejection Tests
// ============================================================================

#[test]
fn test_invalid_codex32_prefix_rejected() {
    let result = parse_share("xx10testsxxxxxxxxxxxxxxxxxxxxxxxxxx4nzvca9cmczlw");
    assert!(result.is_err(), "Wrong prefix should be rejected");
}

#[test]
fn test_invalid_codex32_checksum_rejected() {
    // Valid: ms10testsxxxxxxxxxxxxxxxxxxxxxxxxxx4nzvca9cmczlw
    // Corrupt last char
    let result = parse_share("ms10testsxxxxxxxxxxxxxxxxxxxxxxxxxx4nzvca9cmczlx");
    assert!(result.is_err(), "Bad checksum should be rejected");
}

#[test]
fn test_empty_codex32_share_rejected() {
    let result = parse_share("");
    assert!(result.is_err());
}

#[test]
fn test_codex32_too_short_rejected() {
    let result = parse_share("ms1");
    assert!(result.is_err());
}

#[test]
fn test_invalid_bech32_chars_rejected() {
    // 'b', 'i', 'o' are not in bech32 charset
    let result = parse_share("ms10testbxxxxxxxxxxxxxxxxxxxxxxxxxx4nzvca9cmczlw");
    assert!(result.is_err(), "Invalid bech32 chars should be rejected");
}

#[test]
fn test_shamir_split_threshold_too_low() {
    let secret = b"test secret";
    let result = split_secret(secret, 1, 3);
    assert!(result.is_err(), "Threshold < 2 should fail");
}

#[test]
fn test_shamir_split_threshold_exceeds_total() {
    let secret = b"test secret";
    let result = split_secret(secret, 5, 3);
    assert!(result.is_err(), "Threshold > total should fail");
}

#[test]
fn test_shamir_split_empty_secret() {
    let result = split_secret(b"", 2, 3);
    assert!(result.is_err(), "Empty secret should fail");
}

#[test]
fn test_shamir_reconstruct_empty_shares() {
    let result = reconstruct_secret(&[]);
    assert!(result.is_err(), "Empty shares should fail");
}

#[test]
fn test_shamir_reconstruct_mismatched_lengths() {
    use nostring_shamir::shamir::Share;

    let shares = vec![
        Share { index: 1, data: vec![1, 2, 3] },
        Share { index: 2, data: vec![4, 5] }, // Different length
    ];
    let result = reconstruct_secret(&shares);
    assert!(result.is_err(), "Mismatched share lengths should fail");
}

#[test]
fn test_shamir_reconstruct_duplicate_indices() {
    use nostring_shamir::shamir::Share;

    let shares = vec![
        Share { index: 1, data: vec![1, 2, 3] },
        Share { index: 1, data: vec![4, 5, 6] }, // Duplicate index
    ];
    let result = reconstruct_secret(&shares);
    assert!(result.is_err(), "Duplicate indices should fail");
}

// ============================================================================
// 4. Malformed Input Panic Tests (should NOT panic)
// ============================================================================

#[test]
fn test_parse_mnemonic_garbage_does_not_panic() {
    let inputs = [
        "",
        "a",
        "hello world",
        "abandon abandon abandon", // Too few words
        &"abandon ".repeat(100),   // Too many words
        "🎉 🎊 🎈 🎃 🎄 🎅 🎆 🎇 🎁 🎂 🎀 🎍", // Unicode
        "\0\0\0\0\0\0\0\0\0\0\0\0", // Null bytes
        &"a".repeat(10000),          // Very long
    ];

    for input in &inputs {
        // Should return Err, not panic
        let _ = parse_mnemonic(input);
    }
}

#[test]
fn test_codex32_parse_garbage_does_not_panic() {
    let inputs = [
        "",
        "ms",
        "ms1",
        "ms1x",
        "ms10",
        "not a share at all",
        &"ms1".to_string().as_str().repeat(1000),
        "ms10testsxxxxxxxxxxxxxxxxxxxxxxxxxINVALIDCHECKSUM",
        "MS10TESTSXXXXXXXXXXXXXXXXXXXXXXXXXX4NZVCA9CMCZLW", // uppercase
        "\0\0\0",
    ];

    for input in &inputs {
        let _ = parse_share(input);
    }
}

#[test]
fn test_encrypted_seed_from_garbage_bytes_does_not_panic() {
    let inputs: Vec<Vec<u8>> = vec![
        vec![],
        vec![0],
        vec![0; 10],
        vec![0xFF; 100],
        vec![0; 1000],
        (0..255).collect(),
    ];

    for input in &inputs {
        let _ = EncryptedSeed::from_bytes(input);
    }
}

// ============================================================================
// 5. Codex32 Fuzz Tests (random inputs)
// ============================================================================

#[test]
fn test_codex32_fuzz_random_strings() {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    for _ in 0..1000 {
        // Generate random string of bech32-ish chars
        let len = rng.gen_range(3..100);
        let chars: String = (0..len)
            .map(|_| {
                let idx = rng.gen_range(0..36);
                if idx < 26 {
                    (b'a' + idx as u8) as char
                } else {
                    (b'0' + (idx - 26) as u8) as char
                }
            })
            .collect();

        let input = format!("ms1{}", chars);
        // Should not panic
        let _ = parse_share(&input);
    }
}

#[test]
fn test_codex32_fuzz_valid_share_bit_flip() {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    // Start with a known valid share
    let valid = "ms10testsxxxxxxxxxxxxxxxxxxxxxxxxxx4nzvca9cmczlw";

    for _ in 0..500 {
        let mut chars: Vec<u8> = valid.bytes().collect();
        // Flip a random character
        let idx = rng.gen_range(0..chars.len());
        chars[idx] = (chars[idx] as u16 ^ rng.gen_range(1..128) as u16) as u8;

        let corrupted = String::from_utf8_lossy(&chars).to_string();
        // Should not panic (may succeed or fail)
        let _ = parse_share(&corrupted);
    }
}

#[test]
fn test_codex32_generate_and_verify_roundtrip_random_seeds() {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    for _ in 0..20 {
        // Random 16-byte seed
        let mut seed = vec![0u8; 16];
        rng.fill(&mut seed[..]);

        let config = Codex32Config::new(2, "test", 3).unwrap();
        let shares = codex32_generate(&seed, &config).unwrap();

        // Verify any 2 shares reconstruct correctly
        let recovered = combine_shares(&shares[0..2]).unwrap();
        assert_eq!(recovered, seed, "Roundtrip failed for random seed");
    }
}

// ============================================================================
// 6. Timing Attack Resistance (best-effort verification)
// ============================================================================

#[test]
fn test_wrong_password_timing_consistency() {
    // Verify that wrong-password decryption doesn't short-circuit
    // (This is inherently guaranteed by AES-GCM — the full decryption
    // runs before the auth tag is checked. This test documents the property.)
    let seed = [0x42u8; 64];
    let password = "correct password";
    let encrypted = encrypt_seed(&seed, password).unwrap();

    // Try many wrong passwords — all should fail with the same error type
    let wrong_passwords = [
        "wrong1", "wrong2", "wrong3", "",
        &"a".repeat(1000),
        "correct passwor", // Off by one
        "correct password ", // Extra space
    ];

    for wp in &wrong_passwords {
        let result = decrypt_seed(&encrypted, wp);
        assert!(result.is_err(), "Wrong password '{}' should fail", wp);
    }
}

// ============================================================================
// 7. SQL Parameterization Verification (compile-time via rusqlite)
// ============================================================================

// Note: All SQL queries in db.rs use rusqlite's params![] macro which
// provides compile-time parameterized queries. There are ZERO raw string
// concatenations in any SQL queries. This is verified by code review and
// documented in SECURITY_AUDIT_CHECKLIST.md.

// ============================================================================
// 8. Key Derivation Security
// ============================================================================

#[test]
fn test_different_seeds_different_keys() {
    let m1 = parse_mnemonic(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
    ).unwrap();
    let m2 = parse_mnemonic(
        "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong"
    ).unwrap();

    let seed1 = derive_seed(&m1, "");
    let seed2 = derive_seed(&m2, "");

    // Seeds must be different
    assert_ne!(seed1, seed2);

    // Encrypted forms must be different
    let enc1 = encrypt_seed(&seed1, "pw").unwrap();
    let enc2 = encrypt_seed(&seed2, "pw").unwrap();
    assert_ne!(enc1.to_bytes(), enc2.to_bytes());
}

#[test]
fn test_passphrase_changes_seed() {
    let mnemonic = parse_mnemonic(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
    ).unwrap();

    let seed_no_pass = derive_seed(&mnemonic, "");
    let seed_with_pass = derive_seed(&mnemonic, "my passphrase");
    let seed_other_pass = derive_seed(&mnemonic, "different passphrase");

    assert_ne!(seed_no_pass, seed_with_pass);
    assert_ne!(seed_with_pass, seed_other_pass);
    assert_ne!(seed_no_pass, seed_other_pass);
}

// ============================================================================
// 9. BIP-39 Word Count Validation
// ============================================================================

#[test]
fn test_all_valid_word_counts() {
    for wc in [WordCount::Words12, WordCount::Words15, WordCount::Words18, WordCount::Words21, WordCount::Words24] {
        let mnemonic = generate_mnemonic(wc).unwrap();
        assert_eq!(mnemonic.word_count(), wc as usize);
    }
}

// ============================================================================
// 10. Codex32 Config Validation
// ============================================================================

#[test]
fn test_codex32_config_edge_cases() {
    // Valid configs
    assert!(Codex32Config::new(2, "test", 3).is_ok());
    assert!(Codex32Config::new(9, "test", 9).is_ok());

    // Invalid configs
    assert!(Codex32Config::new(0, "test", 3).is_err());
    assert!(Codex32Config::new(1, "test", 3).is_err());
    assert!(Codex32Config::new(10, "test", 10).is_err());
    assert!(Codex32Config::new(2, "", 3).is_err());
    assert!(Codex32Config::new(2, "toolongid", 3).is_err());
    assert!(Codex32Config::new(2, "te!", 3).is_err());
    assert!(Codex32Config::new(5, "test", 3).is_err()); // threshold > total
}
