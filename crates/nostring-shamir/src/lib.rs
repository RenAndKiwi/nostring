//! NoString Shamir Module
//!
//! Split and reconstruct BIP-39 seeds using Shamir's Secret Sharing.
//!
//! # Two Paths
//!
//! ## SLIP-39 (Digital)
//! - Standard implementation for digital backup
//! - Generate M-of-N shares as word lists
//! - Compatible with other SLIP-39 tools
//!
//! ## Codex32 (Physical)
//! - Paper-based Shamir using volvelles
//! - Fully offline, air-gapped operation
//! - Bech32-encoded shares for error detection
//! - **Reconstructs to BIP-39 compatible seed**
//!
//! # Example: Split a BIP-39 entropy
//!
//! ```
//! use nostring_shamir::slip39::{generate_shares, combine_shares, Slip39Config};
//!
//! // 128-bit entropy (12-word BIP-39)
//! let entropy = vec![0x42u8; 16];
//!
//! // Split into 2-of-3 shares
//! let config = Slip39Config::two_of_three();
//! let groups = generate_shares(&entropy, &config).unwrap();
//!
//! // Each share has mnemonic words
//! for share in &groups[0] {
//!     println!("Share {}: {}", share.member_index + 1, share.words.join(" "));
//! }
//!
//! // Recover with any 2 shares
//! let recovered = combine_shares(&groups[0][0..2]).unwrap();
//! assert_eq!(recovered, entropy);
//! ```

pub mod codex32;
pub mod gf256;
pub mod shamir;
pub mod shares;
pub mod slip39;
pub mod wordlist;

// Re-exports
pub use shamir::{reconstruct_secret, split_secret, Share};
pub use slip39::{combine_shares, generate_shares, Slip39Config, Slip39Share};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShamirError {
    #[error("Invalid threshold: need at least 2")]
    InvalidThreshold,
    #[error("Threshold exceeds share count")]
    ThresholdExceedsShares,
    #[error("Not enough shares to reconstruct")]
    InsufficientShares,
    #[error("Share verification failed")]
    VerificationFailed,
    #[error("Invalid share format: {0}")]
    InvalidShare(String),
}

/// Configuration for Shamir split
pub struct ShamirConfig {
    /// Minimum shares needed to reconstruct (M)
    pub threshold: u8,
    /// Total shares to generate (N)
    pub total_shares: u8,
}

impl ShamirConfig {
    /// Common 2-of-3 setup
    pub fn two_of_three() -> Self {
        Self {
            threshold: 2,
            total_shares: 3,
        }
    }

    /// Common 3-of-5 setup
    pub fn three_of_five() -> Self {
        Self {
            threshold: 3,
            total_shares: 5,
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ShamirError> {
        if self.threshold < 2 {
            return Err(ShamirError::InvalidThreshold);
        }
        if self.threshold > self.total_shares {
            return Err(ShamirError::ThresholdExceedsShares);
        }
        Ok(())
    }
}
