//! BIP-39 seed management
//!
//! Handles seed generation, import, encryption, and storage.

use bip39::{Mnemonic, Language};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SeedError {
    #[error("Invalid mnemonic: {0}")]
    InvalidMnemonic(String),
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
}

/// Generate a new BIP-39 mnemonic (24 words)
pub fn generate_mnemonic() -> Result<Mnemonic, SeedError> {
    let mnemonic = Mnemonic::generate_in(Language::English, 24)
        .map_err(|e| SeedError::InvalidMnemonic(e.to_string()))?;
    Ok(mnemonic)
}

/// Parse a mnemonic from words
pub fn parse_mnemonic(words: &str) -> Result<Mnemonic, SeedError> {
    Mnemonic::parse_in(Language::English, words)
        .map_err(|e| SeedError::InvalidMnemonic(e.to_string()))
}

/// Derive seed bytes from mnemonic (with optional passphrase)
pub fn derive_seed(mnemonic: &Mnemonic, passphrase: &str) -> [u8; 64] {
    mnemonic.to_seed(passphrase)
}

// TODO: Implement encrypted storage with Argon2id + AES-256-GCM
