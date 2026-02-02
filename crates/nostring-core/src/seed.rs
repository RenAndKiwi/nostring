//! BIP-39 seed management
//!
//! Handles seed generation, import, encryption, and storage.
//!
//! # BIP-39 Overview
//!
//! BIP-39 defines a standard for mnemonic phrases:
//! - Word counts: 12, 15, 18, 21, or 24 words
//! - Language: English (we only support English for now)
//! - Checksum: Last bits of SHA256 hash of entropy
//! - Seed derivation: PBKDF2-HMAC-SHA512 with 2048 iterations
//!
//! # Security Notes
//!
//! - Mnemonics should never be logged or stored in plaintext
//! - Always use the encrypted storage functions for persistence
//! - Passphrases (optional 25th word) add an extra layer of security

use bip39::{Mnemonic, Language};
use thiserror::Error;

/// Supported word counts for BIP-39 mnemonics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordCount {
    Words12 = 12,
    Words15 = 15,
    Words18 = 18,
    Words21 = 21,
    Words24 = 24,
}

impl WordCount {
    /// Get the entropy bits for this word count
    pub fn entropy_bits(self) -> usize {
        match self {
            WordCount::Words12 => 128,
            WordCount::Words15 => 160,
            WordCount::Words18 => 192,
            WordCount::Words21 => 224,
            WordCount::Words24 => 256,
        }
    }
}

impl From<WordCount> for usize {
    fn from(wc: WordCount) -> usize {
        wc as usize
    }
}

#[derive(Error, Debug)]
pub enum SeedError {
    #[error("Invalid mnemonic: {0}")]
    InvalidMnemonic(String),
    #[error("Invalid word count: {0}")]
    InvalidWordCount(usize),
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
}

/// Generate a new BIP-39 mnemonic with the specified word count.
///
/// # Arguments
/// * `word_count` - Number of words (12, 15, 18, 21, or 24)
///
/// # Returns
/// A valid BIP-39 mnemonic phrase
///
/// # Example
/// ```
/// use nostring_core::seed::{generate_mnemonic, WordCount};
/// let mnemonic = generate_mnemonic(WordCount::Words24).unwrap();
/// assert_eq!(mnemonic.word_count(), 24);
/// ```
pub fn generate_mnemonic(word_count: WordCount) -> Result<Mnemonic, SeedError> {
    Mnemonic::generate_in(Language::English, word_count.into())
        .map_err(|e: bip39::Error| SeedError::InvalidMnemonic(e.to_string()))
}

/// Generate a new BIP-39 mnemonic with 24 words (256-bit entropy).
///
/// This is the recommended word count for maximum security.
pub fn generate_mnemonic_24() -> Result<Mnemonic, SeedError> {
    generate_mnemonic(WordCount::Words24)
}

/// Parse and validate a mnemonic from a space-separated word string.
///
/// # Arguments
/// * `words` - Space-separated mnemonic words
///
/// # Returns
/// A validated Mnemonic, or an error if invalid
///
/// # Errors
/// - Invalid word count
/// - Invalid words (not in BIP-39 wordlist)
/// - Invalid checksum
pub fn parse_mnemonic(words: &str) -> Result<Mnemonic, SeedError> {
    Mnemonic::parse_in(Language::English, words)
        .map_err(|e| SeedError::InvalidMnemonic(e.to_string()))
}

/// Derive a 64-byte seed from a mnemonic.
///
/// Uses PBKDF2-HMAC-SHA512 with 2048 iterations.
/// Salt is "mnemonic" + passphrase.
///
/// # Arguments
/// * `mnemonic` - A valid BIP-39 mnemonic
/// * `passphrase` - Optional passphrase (empty string if none)
///
/// # Returns
/// A 64-byte (512-bit) seed suitable for BIP-32 key derivation
pub fn derive_seed(mnemonic: &Mnemonic, passphrase: &str) -> [u8; 64] {
    mnemonic.to_seed(passphrase)
}

/// Validate a mnemonic string without parsing.
///
/// Useful for quick validation before attempting full parse.
pub fn is_valid_mnemonic(words: &str) -> bool {
    parse_mnemonic(words).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mnemonic_24() {
        let mnemonic = generate_mnemonic_24().unwrap();
        assert_eq!(mnemonic.word_count(), 24);
    }

    #[test]
    fn test_generate_mnemonic_12() {
        let mnemonic = generate_mnemonic(WordCount::Words12).unwrap();
        assert_eq!(mnemonic.word_count(), 12);
    }

    #[test]
    fn test_parse_valid_mnemonic() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mnemonic = parse_mnemonic(words).unwrap();
        assert_eq!(mnemonic.word_count(), 12);
    }

    #[test]
    fn test_parse_invalid_mnemonic() {
        // Invalid checksum
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon";
        assert!(parse_mnemonic(words).is_err());
    }

    #[test]
    fn test_parse_invalid_word() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon notaword";
        assert!(parse_mnemonic(words).is_err());
    }

    /// BIP-39 Test Vectors from https://github.com/trezor/python-mnemonic
    /// Format: (entropy_hex, mnemonic, seed_hex, xprv)
    #[test]
    fn test_bip39_vector_12_words() {
        // Vector: 00000000000000000000000000000000
        let mnemonic = parse_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
        ).unwrap();
        
        let seed = derive_seed(&mnemonic, "TREZOR");
        let expected_seed = hex::decode(
            "c55257c360c07c72029aebc1b53c05ed0362ada38ead3e3e9efa3708e53495531f09a6987599d18264c1e1c92f2cf141630c7a3c4ab7c81b2f001698e7463b04"
        ).unwrap();
        
        assert_eq!(seed.as_slice(), expected_seed.as_slice());
    }

    #[test]
    fn test_bip39_vector_24_words() {
        // Vector: 0000000000000000000000000000000000000000000000000000000000000000
        let mnemonic = parse_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art"
        ).unwrap();
        
        let seed = derive_seed(&mnemonic, "TREZOR");
        let expected_seed = hex::decode(
            "bda85446c68413707090a52022edd26a1c9462295029f2e60cd7c4f2bbd3097170af7a4d73245cafa9c3cca8d561a7c3de6f5d4a10be8ed2a5e608d68f92fcc8"
        ).unwrap();
        
        assert_eq!(seed.as_slice(), expected_seed.as_slice());
    }

    #[test]
    fn test_bip39_vector_no_passphrase() {
        // Without "TREZOR" passphrase, we get different seed
        let mnemonic = parse_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
        ).unwrap();
        
        // With empty passphrase
        let seed_no_pass = derive_seed(&mnemonic, "");
        
        // With "TREZOR" passphrase
        let seed_with_pass = derive_seed(&mnemonic, "TREZOR");
        
        // They should be different
        assert_ne!(seed_no_pass, seed_with_pass);
    }

    #[test]
    fn test_bip39_vector_zoo() {
        // Vector: ffffffffffffffffffffffffffffffff (12 words)
        let mnemonic = parse_mnemonic(
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong"
        ).unwrap();
        
        let seed = derive_seed(&mnemonic, "TREZOR");
        let expected_seed = hex::decode(
            "ac27495480225222079d7be181583751e86f571027b0497b5b5d11218e0a8a13332572917f0f8e5a589620c6f15b11c61dee327651a14c34e18231052e48c069"
        ).unwrap();
        
        assert_eq!(seed.as_slice(), expected_seed.as_slice());
    }
}

// TODO: Implement encrypted storage with Argon2id + AES-256-GCM
