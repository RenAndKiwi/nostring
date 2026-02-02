//! NoString Core
//!
//! Shared types and key derivation for NoString.
//!
//! # Key Derivation
//!
//! From a single BIP-39 seed:
//! - Nostr keys via NIP-06: m/44'/1237'/0'/0/0
//! - Bitcoin keys via BIP-84: m/84'/0'/0'
//!
//! # Encrypted Storage
//!
//! Seeds are encrypted at rest using Argon2id + AES-256-GCM.

pub mod keys;
pub mod seed;
pub mod crypto;

pub use keys::*;
pub use seed::*;
pub use crypto::{encrypt_seed, decrypt_seed, EncryptedSeed, CryptoError};
