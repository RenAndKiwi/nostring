//! Core Shamir's Secret Sharing implementation
//!
//! Split a secret into N shares where any M can reconstruct it.

use crate::gf256::{lagrange_interpolate, poly_eval};
use crate::ShamirError;
use rand::RngCore;

use serde::{Deserialize, Serialize};

/// A single share of a secret
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    /// Share index (1..=N, never 0)
    pub index: u8,
    /// Share data (same length as original secret)
    pub data: Vec<u8>,
}

/// Split a secret into shares using Shamir's Secret Sharing
///
/// # Arguments
/// * `secret` - The secret bytes to split
/// * `threshold` - Minimum shares needed to reconstruct (M)
/// * `total` - Total shares to generate (N)
///
/// # Returns
/// Vector of N shares, any M of which can reconstruct the secret
pub fn split_secret(secret: &[u8], threshold: u8, total: u8) -> Result<Vec<Share>, ShamirError> {
    if threshold < 2 {
        return Err(ShamirError::InvalidThreshold);
    }
    if threshold > total {
        return Err(ShamirError::ThresholdExceedsShares);
    }
    if secret.is_empty() {
        return Err(ShamirError::InvalidShare("Empty secret".into()));
    }

    let mut rng = rand::thread_rng();
    let mut shares: Vec<Share> = (1..=total)
        .map(|i| Share {
            index: i,
            data: Vec::with_capacity(secret.len()),
        })
        .collect();

    // For each byte of the secret, create a random polynomial and evaluate at each x
    for &secret_byte in secret {
        // Generate random coefficients for polynomial
        // p(x) = secret + c1*x + c2*x^2 + ... + c_{t-1}*x^{t-1}
        let mut coefficients = vec![secret_byte];
        for _ in 1..threshold {
            let mut random_byte = [0u8];
            rng.fill_bytes(&mut random_byte);
            coefficients.push(random_byte[0]);
        }

        // Evaluate polynomial at x = 1, 2, 3, ..., N
        for share in &mut shares {
            let y = poly_eval(&coefficients, share.index);
            share.data.push(y);
        }
    }

    Ok(shares)
}

/// Reconstruct a secret from shares
///
/// # Arguments
/// * `shares` - At least threshold shares
///
/// # Returns
/// The original secret bytes
pub fn reconstruct_secret(shares: &[Share]) -> Result<Vec<u8>, ShamirError> {
    if shares.is_empty() {
        return Err(ShamirError::InsufficientShares);
    }

    // All shares must have the same length
    let secret_len = shares[0].data.len();
    if shares.iter().any(|s| s.data.len() != secret_len) {
        return Err(ShamirError::InvalidShare(
            "Shares have different lengths".into(),
        ));
    }

    // Check for duplicate indices
    let mut indices: Vec<u8> = shares.iter().map(|s| s.index).collect();
    indices.sort();
    indices.dedup();
    if indices.len() != shares.len() {
        return Err(ShamirError::InvalidShare("Duplicate share indices".into()));
    }

    // Reconstruct each byte using Lagrange interpolation
    let mut secret = Vec::with_capacity(secret_len);
    for byte_idx in 0..secret_len {
        let points: Vec<(u8, u8)> = shares.iter().map(|s| (s.index, s.data[byte_idx])).collect();

        let byte = lagrange_interpolate(&points);
        secret.push(byte);
    }

    Ok(secret)
}

/// Verify that shares are consistent (reconstruct to the same secret)
///
/// This checks if any threshold-sized subset reconstructs to the same value.
/// Useful for detecting corrupted shares.
pub fn verify_shares(shares: &[Share], threshold: usize) -> Result<bool, ShamirError> {
    if shares.len() < threshold {
        return Err(ShamirError::InsufficientShares);
    }

    // Try multiple subsets and verify they all give the same result
    let first_subset: Vec<Share> = shares[0..threshold].to_vec();
    let expected = reconstruct_secret(&first_subset)?;

    // Try other combinations
    if shares.len() > threshold {
        // Try with the last `threshold` shares
        let last_subset: Vec<Share> = shares[shares.len() - threshold..].to_vec();
        let result = reconstruct_secret(&last_subset)?;
        if result != expected {
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_and_reconstruct_2_of_3() {
        let secret = b"Hello, Shamir!";
        let shares = split_secret(secret, 2, 3).unwrap();

        assert_eq!(shares.len(), 3);

        // Reconstruct with shares 1 and 2
        let recovered = reconstruct_secret(&shares[0..2]).unwrap();
        assert_eq!(recovered, secret);

        // Reconstruct with shares 2 and 3
        let recovered = reconstruct_secret(&shares[1..3]).unwrap();
        assert_eq!(recovered, secret);

        // Reconstruct with shares 1 and 3
        let recovered = reconstruct_secret(&[shares[0].clone(), shares[2].clone()]).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn test_split_and_reconstruct_3_of_5() {
        let secret = b"A longer secret message for testing 3-of-5 Shamir";
        let shares = split_secret(secret, 3, 5).unwrap();

        assert_eq!(shares.len(), 5);

        // Reconstruct with first 3 shares
        let recovered = reconstruct_secret(&shares[0..3]).unwrap();
        assert_eq!(recovered, secret);

        // Reconstruct with last 3 shares
        let recovered = reconstruct_secret(&shares[2..5]).unwrap();
        assert_eq!(recovered, secret);

        // Reconstruct with non-consecutive shares
        let recovered =
            reconstruct_secret(&[shares[0].clone(), shares[2].clone(), shares[4].clone()]).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn test_split_256_bit_seed() {
        // Test with a 256-bit (32 byte) seed like BIP-39 entropy
        let seed: Vec<u8> = (0..32).collect();
        let shares = split_secret(&seed, 2, 3).unwrap();

        let recovered = reconstruct_secret(&shares[0..2]).unwrap();
        assert_eq!(recovered, seed);
    }

    #[test]
    fn test_insufficient_shares() {
        let secret = b"test";
        let shares = split_secret(secret, 3, 5).unwrap();

        // Try to reconstruct with only 2 shares (need 3)
        let result = reconstruct_secret(&shares[0..2]);
        // This will "succeed" but give wrong answer
        // The caller must know the threshold and provide enough shares
        assert!(result.is_ok());
        assert_ne!(result.unwrap(), secret.to_vec());
    }

    #[test]
    fn test_invalid_threshold() {
        let secret = b"test";

        // Threshold < 2
        assert!(split_secret(secret, 1, 3).is_err());

        // Threshold > total
        assert!(split_secret(secret, 5, 3).is_err());
    }

    #[test]
    fn test_verify_shares() {
        let secret = b"verify me";
        let shares = split_secret(secret, 2, 3).unwrap();

        assert!(verify_shares(&shares, 2).unwrap());
    }

    #[test]
    fn test_share_indices() {
        let secret = b"test";
        let shares = split_secret(secret, 2, 5).unwrap();

        // Indices should be 1, 2, 3, 4, 5
        for (i, share) in shares.iter().enumerate() {
            assert_eq!(share.index, (i + 1) as u8);
        }
    }
}
