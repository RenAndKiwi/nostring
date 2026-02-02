//! Codex32: Physical Shamir Secret Sharing
//!
//! Paper-based backup system using Bech32 encoding and offline volvelle computation.
//! https://github.com/roconnor-blockstream/SSS32
//!
//! # Features
//!
//! - Fully offline operation (no computer needed for reconstruction)
//! - Bech32 error detection (catches typos)
//! - Human-readable share format
//! - Compatible with BIP-39 seed reconstruction
//!
//! # Format
//!
//! Each share is encoded as: `ms1<threshold><identifier><share_index><payload><checksum>`
//!
//! Example: `ms13cashh...` (2-of-3, identifier "cash", share 1)

use crate::ShamirError;
use serde::{Deserialize, Serialize};

/// Codex32 HRP (Human Readable Part)
pub const CODEX32_HRP: &str = "ms";

/// A Codex32 share
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Codex32Share {
    /// Threshold (2-9)
    pub threshold: u8,
    /// 4-character identifier
    pub identifier: String,
    /// Share index (s, a, c, d, e, f, g, h, j, k, l, m, n, p, q, r, t, u, v, w, x, y, z, 2, 3, 4, 5, 6, 7, 8, 9)
    pub index: char,
    /// Payload (the share data)
    pub payload: Vec<u8>,
    /// Full bech32-encoded string
    pub encoded: String,
}

/// Configuration for Codex32 generation
#[derive(Debug, Clone)]
pub struct Codex32Config {
    /// Threshold (2-9)
    pub threshold: u8,
    /// 4-character identifier (lowercase letters and digits 0-9)
    pub identifier: String,
    /// Total shares to generate
    pub total_shares: u8,
}

impl Codex32Config {
    /// Create a 2-of-3 configuration
    pub fn two_of_three(identifier: &str) -> Result<Self, ShamirError> {
        Self::new(2, identifier, 3)
    }

    /// Create a new configuration
    pub fn new(threshold: u8, identifier: &str, total_shares: u8) -> Result<Self, ShamirError> {
        if threshold < 2 || threshold > 9 {
            return Err(ShamirError::InvalidThreshold);
        }
        if identifier.len() != 4 {
            return Err(ShamirError::InvalidShare(
                "Identifier must be exactly 4 characters".into(),
            ));
        }
        if threshold > total_shares {
            return Err(ShamirError::ThresholdExceedsShares);
        }
        Ok(Self {
            threshold,
            identifier: identifier.to_lowercase(),
            total_shares,
        })
    }
}

/// Generate Codex32 shares from a master seed
///
/// # Note
/// Full implementation requires the Codex32 polynomial arithmetic.
/// This is a placeholder that shows the API design.
pub fn generate_shares(
    _seed: &[u8],
    _config: &Codex32Config,
) -> Result<Vec<Codex32Share>, ShamirError> {
    // TODO: Implement full Codex32 generation
    // This requires:
    // 1. BCH polynomial math for share generation
    // 2. Bech32 encoding
    // 3. Checksum computation

    Err(ShamirError::InvalidShare(
        "Codex32 generation not yet implemented".into(),
    ))
}

/// Combine Codex32 shares to recover the master seed
pub fn combine_shares(_shares: &[Codex32Share]) -> Result<Vec<u8>, ShamirError> {
    // TODO: Implement full Codex32 reconstruction
    Err(ShamirError::InvalidShare(
        "Codex32 reconstruction not yet implemented".into(),
    ))
}

/// Parse a Codex32 string into a share
pub fn parse_share(encoded: &str) -> Result<Codex32Share, ShamirError> {
    if !encoded.starts_with(CODEX32_HRP) {
        return Err(ShamirError::InvalidShare(
            "Codex32 share must start with 'ms'".into(),
        ));
    }

    // Parse format: ms<threshold><identifier><index><payload><checksum>
    let data = &encoded[2..];
    if data.len() < 6 {
        return Err(ShamirError::InvalidShare("Codex32 share too short".into()));
    }

    let threshold = data
        .chars()
        .next()
        .and_then(|c| c.to_digit(10))
        .ok_or_else(|| ShamirError::InvalidShare("Invalid threshold".into()))?
        as u8;

    let identifier: String = data.chars().skip(1).take(4).collect();
    let index = data
        .chars()
        .nth(5)
        .ok_or_else(|| ShamirError::InvalidShare("Missing share index".into()))?;

    // Payload is everything after index until checksum (last 6 chars)
    // This is simplified - full implementation needs proper Bech32 decoding
    let payload = data[6..].as_bytes().to_vec();

    Ok(Codex32Share {
        threshold,
        identifier,
        index,
        payload,
        encoded: encoded.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        assert!(Codex32Config::new(2, "test", 3).is_ok());
        assert!(Codex32Config::new(1, "test", 3).is_err()); // threshold < 2
        assert!(Codex32Config::new(2, "tes", 3).is_err()); // identifier too short
        assert!(Codex32Config::new(5, "test", 3).is_err()); // threshold > total
    }

    #[test]
    fn test_parse_share_format() {
        // Codex32 format: ms<threshold><id:4><index><payload>
        // Example: ms2cashaXXXXXX = threshold 2, id "cash", index 'a'
        let share_str = "ms2cashaabcdefg123456";
        let result = parse_share(share_str);
        assert!(result.is_ok());

        let share = result.unwrap();
        assert_eq!(share.threshold, 2);
        assert_eq!(share.identifier, "cash");
        assert_eq!(share.index, 'a');
    }
}
