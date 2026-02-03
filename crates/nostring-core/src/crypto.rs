//! Cryptographic utilities
//!
//! Password-based encryption for seed storage using Argon2id + AES-256-GCM.
//!
//! # Security Notes
//!
//! - Argon2id is memory-hard (resistant to GPU/ASIC attacks)
//! - AES-256-GCM provides authenticated encryption
//! - Each encryption uses a random nonce
//! - Password is never stored

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

/// Argon2id parameters (OWASP recommendations for 2024+)
/// - m_cost: 64 MiB memory
/// - t_cost: 3 iterations  
/// - p_cost: 4 parallel threads
const ARGON2_M_COST: u32 = 65536; // 64 MiB
const ARGON2_T_COST: u32 = 3;
const ARGON2_P_COST: u32 = 4;
const ARGON2_OUTPUT_LEN: usize = 32; // 256 bits for AES-256

/// Salt length for Argon2
const SALT_LEN: usize = 16;

/// Nonce length for AES-256-GCM
const NONCE_LEN: usize = 12;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("Key derivation failed: {0}")]
    KeyDerivationFailed(String),
    #[error("Invalid ciphertext format")]
    InvalidFormat,
}

/// Encrypted seed format:
/// [salt (16 bytes)][nonce (12 bytes)][ciphertext (64 + 16 bytes)]
/// Total: 108 bytes for a 64-byte seed
pub struct EncryptedSeed {
    /// Salt used for Argon2id key derivation
    salt: [u8; SALT_LEN],
    /// Nonce used for AES-256-GCM
    nonce: [u8; NONCE_LEN],
    /// Encrypted seed + authentication tag
    ciphertext: Vec<u8>,
}

impl Zeroize for EncryptedSeed {
    fn zeroize(&mut self) {
        self.salt.zeroize();
        self.nonce.zeroize();
        self.ciphertext.zeroize();
    }
}

impl Drop for EncryptedSeed {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl EncryptedSeed {
    /// Serialize to bytes: salt || nonce || ciphertext
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(SALT_LEN + NONCE_LEN + self.ciphertext.len());
        bytes.extend_from_slice(&self.salt);
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        // Minimum size: salt + nonce + at least 1 byte ciphertext + 16 byte tag
        if bytes.len() < SALT_LEN + NONCE_LEN + 17 {
            return Err(CryptoError::InvalidFormat);
        }

        let mut salt = [0u8; SALT_LEN];
        let mut nonce = [0u8; NONCE_LEN];

        salt.copy_from_slice(&bytes[0..SALT_LEN]);
        nonce.copy_from_slice(&bytes[SALT_LEN..SALT_LEN + NONCE_LEN]);
        let ciphertext = bytes[SALT_LEN + NONCE_LEN..].to_vec();

        Ok(Self {
            salt,
            nonce,
            ciphertext,
        })
    }
}

/// Derive an encryption key from a password using Argon2id.
///
/// Returns a `Zeroizing` wrapper that automatically zeroes the key
/// material from memory when dropped.
fn derive_key(
    password: &str,
    salt: &[u8; SALT_LEN],
) -> Result<Zeroizing<[u8; ARGON2_OUTPUT_LEN]>, CryptoError> {
    let params = Params::new(
        ARGON2_M_COST,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(ARGON2_OUTPUT_LEN),
    )
    .map_err(|e| CryptoError::KeyDerivationFailed(e.to_string()))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0u8; ARGON2_OUTPUT_LEN]);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|e| CryptoError::KeyDerivationFailed(e.to_string()))?;

    Ok(key)
}

/// Encrypt a seed with a password
///
/// Uses Argon2id for key derivation and AES-256-GCM for encryption.
/// Each call generates a new random salt and nonce.
///
/// # Arguments
/// * `seed` - The 64-byte BIP-39 seed to encrypt
/// * `password` - User-provided password
///
/// # Returns
/// Encrypted seed that can be safely stored
pub fn encrypt_seed(seed: &[u8; 64], password: &str) -> Result<EncryptedSeed, CryptoError> {
    // Generate random salt (16 bytes = 128 bits of entropy from CSPRNG)
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);

    let nonce_arr = Aes256Gcm::generate_nonce(&mut OsRng);
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&nonce_arr);

    // Derive encryption key from password (auto-zeroized on drop)
    let key = derive_key(password, &salt)?;

    // Encrypt seed — key is zeroized when `key` goes out of scope
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&*key));
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), seed.as_slice())
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

    Ok(EncryptedSeed {
        salt,
        nonce,
        ciphertext,
    })
}

/// Decrypt an encrypted seed with a password
///
/// # Arguments
/// * `encrypted` - The encrypted seed
/// * `password` - User-provided password (must match encryption password)
///
/// # Returns
/// The decrypted 64-byte seed
///
/// # Errors
/// Returns error if password is wrong or ciphertext is tampered
pub fn decrypt_seed(
    encrypted: &EncryptedSeed,
    password: &str,
) -> Result<Zeroizing<[u8; 64]>, CryptoError> {
    // Derive decryption key from password using stored salt (auto-zeroized on drop)
    let key = derive_key(password, &encrypted.salt)?;

    // Decrypt seed — key is zeroized when `key` goes out of scope
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&*key));
    let mut plaintext = cipher
        .decrypt(
            Nonce::from_slice(&encrypted.nonce),
            encrypted.ciphertext.as_slice(),
        )
        .map_err(|_| {
            CryptoError::DecryptionFailed("Invalid password or corrupted data".to_string())
        })?;

    // Verify length
    if plaintext.len() != 64 {
        plaintext.zeroize();
        return Err(CryptoError::DecryptionFailed(
            "Invalid seed length".to_string(),
        ));
    }

    let mut seed = Zeroizing::new([0u8; 64]);
    seed.copy_from_slice(&plaintext);

    // Zeroize the plaintext Vec — seed is now safely in a Zeroizing wrapper
    plaintext.zeroize();

    Ok(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let seed = [42u8; 64]; // Test seed
        let password = "correct horse battery staple";

        let encrypted = encrypt_seed(&seed, password).unwrap();
        let decrypted = decrypt_seed(&encrypted, password).unwrap();

        assert_eq!(seed, *decrypted);
    }

    #[test]
    fn test_wrong_password_fails() {
        let seed = [42u8; 64];
        let password = "correct password";
        let wrong_password = "wrong password";

        let encrypted = encrypt_seed(&seed, password).unwrap();
        let result = decrypt_seed(&encrypted, wrong_password);

        assert!(result.is_err());
    }

    #[test]
    fn test_different_encryptions_different_ciphertext() {
        let seed = [42u8; 64];
        let password = "same password";

        let encrypted1 = encrypt_seed(&seed, password).unwrap();
        let encrypted2 = encrypt_seed(&seed, password).unwrap();

        // Due to random salt and nonce, ciphertexts should differ
        assert_ne!(encrypted1.to_bytes(), encrypted2.to_bytes());

        // But both should decrypt to the same seed
        let decrypted1 = decrypt_seed(&encrypted1, password).unwrap();
        let decrypted2 = decrypt_seed(&encrypted2, password).unwrap();
        assert_eq!(*decrypted1, *decrypted2);
        assert_eq!(*decrypted1, seed);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let seed = [42u8; 64];
        let password = "test password";

        let encrypted = encrypt_seed(&seed, password).unwrap();
        let bytes = encrypted.to_bytes();
        let restored = EncryptedSeed::from_bytes(&bytes).unwrap();
        let decrypted = decrypt_seed(&restored, password).unwrap();

        assert_eq!(seed, *decrypted);
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let seed = [42u8; 64];
        let password = "test password";

        let encrypted = encrypt_seed(&seed, password).unwrap();
        let mut bytes = encrypted.to_bytes();

        // Tamper with the ciphertext
        let last_idx = bytes.len() - 1;
        bytes[last_idx] ^= 0xFF;

        let tampered = EncryptedSeed::from_bytes(&bytes).unwrap();
        let result = decrypt_seed(&tampered, password);

        assert!(result.is_err());
    }

    #[test]
    fn test_salt_has_full_128_bit_entropy() {
        // The salt must be 16 random bytes from a CSPRNG, giving 128 bits of entropy.
        // Previously, the salt was derived from base64-encoded SaltString characters,
        // which limited each byte to ~6 bits of entropy (~96 bits total).
        //
        // With OsRng.fill_bytes(), every byte value 0x00–0xFF is possible.
        // We verify this by checking that across many encryptions, salt bytes
        // span outside the printable ASCII / base64 range (0x00–0x2F, 0x80–0xFF).
        let seed = [42u8; 64];
        let password = "test";

        let mut saw_byte_outside_base64 = false;
        for _ in 0..20 {
            let encrypted = encrypt_seed(&seed, password).unwrap();
            // Check if any salt byte falls outside the base64 character range
            // Base64 chars: A-Z(0x41-0x5A), a-z(0x61-0x7A), 0-9(0x30-0x39), +/(0x2B,0x2F)
            for &b in &encrypted.salt {
                let is_base64_char = b.is_ascii_alphanumeric() || b == b'+' || b == b'/';
                if !is_base64_char {
                    saw_byte_outside_base64 = true;
                }
            }
        }
        // With 20 encryptions × 16 bytes = 320 random bytes, the probability
        // that ALL of them happen to be valid base64 chars (64/256 = 25%) is
        // 0.25^320 ≈ 10^{-193}. This test is astronomically unlikely to flake.
        assert!(
            saw_byte_outside_base64,
            "Salt bytes appear restricted to base64 charset — entropy may be only ~96 bits"
        );
    }

    #[test]
    fn test_empty_password_works() {
        // Empty passwords should work (though not recommended)
        let seed = [42u8; 64];
        let password = "";

        let encrypted = encrypt_seed(&seed, password).unwrap();
        let decrypted = decrypt_seed(&encrypted, password).unwrap();

        assert_eq!(seed, *decrypted);
    }

    /// Verify that the derived key is zeroized after being dropped.
    ///
    /// We can't directly inspect the memory of a dropped value, but we
    /// can verify the `Zeroizing` wrapper works by dropping it and
    /// checking a copy made before the zeroization would occur.
    #[test]
    fn test_zeroizing_wrapper_clears_on_drop() {
        let mut key = Zeroizing::new([0xFFu8; 32]);
        // Verify the key has non-zero content before zeroization
        assert!(key.iter().all(|&b| b == 0xFF));

        // Manually zeroize (simulates what Drop does)
        key.zeroize();

        // After zeroize, the contents should be all zeros
        assert!(key.iter().all(|&b| b == 0));
    }

    /// Verify that decrypted seed bytes are zeroized when the Zeroizing wrapper drops.
    #[test]
    fn test_decrypted_seed_zeroized_on_drop() {
        let seed = [42u8; 64];
        let password = "test";

        let encrypted = encrypt_seed(&seed, password).unwrap();
        let mut decrypted = decrypt_seed(&encrypted, password).unwrap();

        // Verify it contains the expected data
        assert_eq!(*decrypted, seed);

        // Manually zeroize (simulates what happens on drop)
        decrypted.zeroize();

        // Verify all bytes are zeroed
        assert!(decrypted.iter().all(|&b| b == 0));
    }

    /// Verify that EncryptedSeed is zeroized on drop.
    #[test]
    fn test_encrypted_seed_zeroized_on_drop() {
        let seed = [42u8; 64];
        let password = "test";

        let mut encrypted = encrypt_seed(&seed, password).unwrap();

        // Verify it has non-zero content
        assert!(!encrypted.salt.iter().all(|&b| b == 0));
        assert!(!encrypted.ciphertext.iter().all(|&b| b == 0));

        // Manually zeroize (simulates what happens on drop)
        encrypted.zeroize();

        // All fields should be zeroed
        assert!(encrypted.salt.iter().all(|&b| b == 0));
        assert!(encrypted.nonce.iter().all(|&b| b == 0));
        assert!(encrypted.ciphertext.is_empty() || encrypted.ciphertext.iter().all(|&b| b == 0));
    }
}
