//! Key derivation from BIP-39 seed
//!
//! Derives both Nostr keys (NIP-06) and Bitcoin keys (BIP-84) from a single seed.

use bitcoin::bip32::{Xpriv, DerivationPath};
use bitcoin::Network;
use nostr_sdk::Keys as NostrKeys;
use thiserror::Error;

/// NIP-06 derivation path for Nostr keys
pub const NIP06_PATH: &str = "m/44'/1237'/0'/0/0";

/// BIP-84 derivation path for Bitcoin keys (native segwit)
pub const BIP84_PATH: &str = "m/84'/0'/0'";

#[derive(Error, Debug)]
pub enum KeyError {
    #[error("Derivation failed: {0}")]
    DerivationFailed(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

/// Derive Nostr keys from seed using NIP-06 path
pub fn derive_nostr_keys(seed: &[u8; 64]) -> Result<NostrKeys, KeyError> {
    let master = Xpriv::new_master(Network::Bitcoin, seed)
        .map_err(|e| KeyError::DerivationFailed(e.to_string()))?;
    
    let path: DerivationPath = NIP06_PATH.parse()
        .map_err(|e: bitcoin::bip32::Error| KeyError::InvalidPath(e.to_string()))?;
    
    let derived = master.derive_priv(&bitcoin::secp256k1::Secp256k1::new(), &path)
        .map_err(|e| KeyError::DerivationFailed(e.to_string()))?;
    
    // Convert to Nostr keys
    let secret_key = nostr_sdk::SecretKey::from_slice(&derived.private_key.secret_bytes())
        .map_err(|e| KeyError::DerivationFailed(e.to_string()))?;
    
    Ok(NostrKeys::new(secret_key))
}

/// Derive Bitcoin master key from seed using BIP-84 path
pub fn derive_bitcoin_master(seed: &[u8; 64]) -> Result<Xpriv, KeyError> {
    let master = Xpriv::new_master(Network::Bitcoin, seed)
        .map_err(|e| KeyError::DerivationFailed(e.to_string()))?;
    
    let path: DerivationPath = BIP84_PATH.parse()
        .map_err(|e: bitcoin::bip32::Error| KeyError::InvalidPath(e.to_string()))?;
    
    master.derive_priv(&bitcoin::secp256k1::Secp256k1::new(), &path)
        .map_err(|e| KeyError::DerivationFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::{generate_mnemonic, derive_seed};

    #[test]
    fn test_key_derivation() {
        let mnemonic = generate_mnemonic().unwrap();
        let seed = derive_seed(&mnemonic, "");
        
        let nostr_keys = derive_nostr_keys(&seed).unwrap();
        let btc_master = derive_bitcoin_master(&seed).unwrap();
        
        // Keys should be deterministic
        let nostr_keys2 = derive_nostr_keys(&seed).unwrap();
        assert_eq!(nostr_keys.public_key(), nostr_keys2.public_key());
        
        // Nostr and Bitcoin keys should be different
        assert_ne!(
            nostr_keys.secret_key().to_secret_bytes(),
            btc_master.private_key.secret_bytes()
        );
    }
}
