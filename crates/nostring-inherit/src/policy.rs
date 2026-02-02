//! Inheritance policy construction
//!
//! Build miniscript policies for timelock inheritance.

use bitcoin::secp256k1::PublicKey;

// Will use miniscript::policy::Concrete when implementing policy compilation
#[allow(unused_imports)]
use miniscript as _;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PolicyError {
    #[error("Invalid policy: {0}")]
    InvalidPolicy(String),
    #[error("No heirs configured")]
    NoHeirs,
}

/// Timelock duration in blocks (~10 min each)
pub struct TimelockBlocks(pub u32);

impl TimelockBlocks {
    /// 6 months (~26,280 blocks)
    pub fn six_months() -> Self {
        Self(26_280)
    }
    
    /// 1 year (~52,560 blocks)
    pub fn one_year() -> Self {
        Self(52_560)
    }
    
    /// Custom duration in days
    pub fn days(days: u32) -> Self {
        Self(days * 144) // ~144 blocks per day
    }
}

/// Inheritance policy configuration
pub struct InheritancePolicy {
    /// Owner's public key (primary path)
    pub owner: PublicKey,
    /// Heirs with their timelocks
    pub recovery_paths: Vec<RecoveryPath>,
}

/// A recovery path (heirs + timelock)
pub struct RecoveryPath {
    /// Required keys (M-of-N if multiple)
    pub keys: Vec<PublicKey>,
    /// Threshold (how many keys required)
    pub threshold: usize,
    /// Timelock duration
    pub timelock: TimelockBlocks,
}

// TODO: Implement policy -> miniscript -> descriptor conversion
