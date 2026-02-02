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
    use crate::seed::{generate_mnemonic_24, derive_seed, parse_mnemonic};
    use nostr_sdk::ToBech32;

    #[test]
    fn test_key_derivation_random() {
        let mnemonic = generate_mnemonic_24().unwrap();
        let seed = derive_seed(&mnemonic, "");
        
        let nostr_keys = derive_nostr_keys(&seed).unwrap();
        let btc_master = derive_bitcoin_master(&seed).unwrap();
        
        // Keys should be deterministic
        let nostr_keys2 = derive_nostr_keys(&seed).unwrap();
        assert_eq!(nostr_keys.public_key(), nostr_keys2.public_key());
        
        // Nostr and Bitcoin keys should be different (different derivation paths)
        assert_ne!(
            nostr_keys.secret_key().to_secret_bytes(),
            btc_master.private_key.secret_bytes()
        );
    }

    /// Official NIP-06 test vector from https://github.com/nostr-protocol/nips/blob/master/06.md
    ///
    /// Mnemonic: leader monkey parrot ring guide accident before fence cannon height naive bean
    /// Path: m/44'/1237'/0'/0/0
    /// Private key (hex): 7f7ff03d123792d6ac594bfa67bf6d0c0ab55b6b1fdb6249303fe861f1ccba9a
    /// nsec: nsec10allq0gjx7fddtzef0ax00mdps9t2kmtrldkyjfs8l5xruwvh2dq0lhhkp
    /// Public key (hex): 17162c921dc4d2518f9a101db33695df1afb56ab82f5ff3e5da6eec3ca5cd917
    /// npub: npub1zutzeysacnf9rru6zqwmxd54mud0k44tst6l70ja5mhv8jjumytsd2x7nu
    #[test]
    fn test_nip06_official_vector() {
        let mnemonic = parse_mnemonic(
            "leader monkey parrot ring guide accident before fence cannon height naive bean"
        ).unwrap();
        
        // NIP-06 uses empty passphrase
        let seed = derive_seed(&mnemonic, "");
        
        let nostr_keys = derive_nostr_keys(&seed).unwrap();
        
        // Check private key (hex)
        let privkey_hex = hex::encode(nostr_keys.secret_key().to_secret_bytes());
        assert_eq!(
            privkey_hex,
            "7f7ff03d123792d6ac594bfa67bf6d0c0ab55b6b1fdb6249303fe861f1ccba9a"
        );
        
        // Check nsec (bech32)
        let nsec = nostr_keys.secret_key().to_bech32().unwrap();
        assert_eq!(
            nsec,
            "nsec10allq0gjx7fddtzef0ax00mdps9t2kmtrldkyjfs8l5xruwvh2dq0lhhkp"
        );
        
        // Check public key (hex)
        let pubkey_hex = nostr_keys.public_key().to_hex();
        assert_eq!(
            pubkey_hex,
            "17162c921dc4d2518f9a101db33695df1afb56ab82f5ff3e5da6eec3ca5cd917"
        );
        
        // Check npub (bech32)
        let npub = nostr_keys.public_key().to_bech32().unwrap();
        assert_eq!(
            npub,
            "npub1zutzeysacnf9rru6zqwmxd54mud0k44tst6l70ja5mhv8jjumytsd2x7nu"
        );
    }

    /// Test that different mnemonics produce different keys
    #[test]
    fn test_different_mnemonics_different_keys() {
        let mnemonic1 = parse_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
        ).unwrap();
        let mnemonic2 = parse_mnemonic(
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong"
        ).unwrap();
        
        let seed1 = derive_seed(&mnemonic1, "");
        let seed2 = derive_seed(&mnemonic2, "");
        
        let keys1 = derive_nostr_keys(&seed1).unwrap();
        let keys2 = derive_nostr_keys(&seed2).unwrap();
        
        assert_ne!(keys1.public_key(), keys2.public_key());
    }

    /// Test that passphrase changes the derived keys
    #[test]
    fn test_passphrase_changes_keys() {
        let mnemonic = parse_mnemonic(
            "leader monkey parrot ring guide accident before fence cannon height naive bean"
        ).unwrap();
        
        let seed_no_pass = derive_seed(&mnemonic, "");
        let seed_with_pass = derive_seed(&mnemonic, "secret passphrase");
        
        let keys_no_pass = derive_nostr_keys(&seed_no_pass).unwrap();
        let keys_with_pass = derive_nostr_keys(&seed_with_pass).unwrap();
        
        // Same mnemonic, different passphrase → different keys
        assert_ne!(keys_no_pass.public_key(), keys_with_pass.public_key());
    }
}
