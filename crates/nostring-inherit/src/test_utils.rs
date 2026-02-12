//! Shared test utilities for nostring-inherit tests.
//!
//! Provides deterministic keypair generation, chain codes, and vault construction
//! helpers used across multiple test modules.

use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey};
use bitcoin::Network;
use miniscript::descriptor::DescriptorPublicKey;
use nostring_ccd::register_cosigner_with_chain_code;
use nostring_ccd::types::ChainCode;
use std::str::FromStr;

use crate::policy::{PathInfo, Timelock};
use crate::taproot::{create_inheritable_vault, InheritableVault};

/// Generate a deterministic keypair from a seed byte.
///
/// The secret key is `[0x01, 0x00, ..., 0x00, seed]` (32 bytes).
/// Different seed bytes produce different keys.
pub fn test_keypair(seed_byte: u8) -> (SecretKey, PublicKey) {
    let secp = Secp256k1::new();
    let mut secret_bytes = [0u8; 32];
    secret_bytes[31] = seed_byte;
    secret_bytes[0] = 0x01;
    let sk = SecretKey::from_slice(&secret_bytes).unwrap();
    let pk = sk.public_key(&secp);
    (sk, pk)
}

/// Deterministic chain code for tests.
pub fn test_chain_code() -> ChainCode {
    ChainCode([0xAB; 32])
}

/// Standard test xpub string (BIP-32 test vector).
pub fn test_xpub_str() -> &'static str {
    "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8"
}

/// Create a test vault with a single heir and the given timelock.
pub fn make_test_vault(timelock_blocks: u16) -> InheritableVault {
    let (_owner_sk, owner_pk) = test_keypair(1);
    let (_cosigner_sk, cosigner_pk) = test_keypair(2);
    let (_heir_sk, heir_pk) = test_keypair(3);
    let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

    let heir_xonly = heir_pk.x_only_public_key().0;
    let heir_desc = DescriptorPublicKey::from_str(&format!("{}", heir_xonly)).unwrap();

    create_inheritable_vault(
        &owner_pk,
        &delegated,
        0,
        PathInfo::Single(heir_desc),
        Timelock::from_blocks(timelock_blocks).unwrap(),
        0,
        Network::Testnet,
    )
    .unwrap()
}

/// Create a test vault with a 6-month timelock (default for most tests).
pub fn make_default_test_vault() -> InheritableVault {
    make_test_vault(26_280)
}
