//! Codex32: BIP-93 Checksummed SSSS-aware BIP32 seeds
//!
//! Paper-based backup system using Bech32 encoding and BCH error correction.
//! Supports offline volvelle computation for share generation and verification.
//!
//! Reference: https://bips.dev/93/
//!
//! # Features
//!
//! - Fully offline operation (no computer needed for reconstruction)
//! - BCH error-correcting checksum (up to 4 substitution errors)
//! - Human-readable share format using bech32 alphabet
//! - Compatible with BIP-32 master seed encoding
//!
//! # Format
//!
//! Each share is encoded as: `ms1<threshold><identifier:4><share_index><payload><checksum:13>`
//!
//! Example: `ms12namea320zyxwvutsrqpnmlkjhgfedcaxrpp870hkkqrm` (2-of-N, identifier "name", share 'a')

use crate::ShamirError;
use serde::{Deserialize, Serialize};

/// Bech32 character set (same as BIP-173)
const CHARSET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";

/// Codex32 HRP (Human Readable Part)
pub const CODEX32_HRP: &str = "ms";

/// Constant for standard checksum verification
const MS32_CONST: u128 = 0x10ce0795c2fd1e62a;

/// BCH generator polynomial coefficients for 13-char checksum
const GEN: [u128; 5] = [
    0x19dc500ce73fde210,
    0x1bfae00def77fe529,
    0x1fbd920fffe7bee52,
    0x1739640bdeee3fdad,
    0x07729a039cfc75f5a,
];

/// Inverse table for bech32 multiplication
const BECH32_INV: [u8; 32] = [
    0, 1, 20, 24, 10, 8, 12, 29, 5, 11, 4, 9, 6, 28, 26, 31, 22, 18, 17, 23, 2, 25, 16, 19, 3, 21,
    14, 30, 13, 7, 27, 15,
];

/// A Codex32 share
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Codex32Share {
    /// Threshold (0, or 2-9). 0 indicates unshared secret.
    pub threshold: u8,
    /// 4-character identifier (bech32)
    pub identifier: String,
    /// Share index (bech32 character). 's' = unshared secret.
    pub index: char,
    /// Payload bytes (decoded from bech32)
    pub payload: Vec<u8>,
    /// Full bech32-encoded string
    pub encoded: String,
}

/// Configuration for Codex32 generation
#[derive(Debug, Clone)]
pub struct Codex32Config {
    /// Threshold (2-9)
    pub threshold: u8,
    /// 4-character identifier (bech32 characters only)
    pub identifier: String,
    /// Total shares to generate
    pub total_shares: u8,
}

impl Codex32Config {
    /// Create a 2-of-3 configuration
    pub fn two_of_three(identifier: &str) -> Result<Self, ShamirError> {
        Self::new(2, identifier, 3)
    }

    /// Create a 3-of-5 configuration
    pub fn three_of_five(identifier: &str) -> Result<Self, ShamirError> {
        Self::new(3, identifier, 5)
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
        // Validate identifier contains only bech32 characters
        let id_lower = identifier.to_lowercase();
        if !id_lower.chars().all(|c| CHARSET.contains(c)) {
            return Err(ShamirError::InvalidShare(
                "Identifier must use bech32 characters only".into(),
            ));
        }
        if threshold > total_shares {
            return Err(ShamirError::ThresholdExceedsShares);
        }
        Ok(Self {
            threshold,
            identifier: id_lower,
            total_shares,
        })
    }
}

/// Compute the BCH polymod checksum
fn ms32_polymod(values: &[u8]) -> u128 {
    let mut residue: u128 = 0x23181b3;
    for &v in values {
        let b = residue >> 60;
        residue = ((residue & 0x0fffffffffffffff) << 5) ^ (v as u128);
        for i in 0..5 {
            if (b >> i) & 1 != 0 {
                residue ^= GEN[i];
            }
        }
    }
    residue
}

/// Verify a codex32 checksum
///
/// # Arguments
/// * `data` - The data part as 5-bit values (including checksum)
///
/// # Returns
/// `true` if the checksum is valid
pub fn ms32_verify_checksum(data: &[u8]) -> bool {
    if data.len() >= 96 {
        // Long checksum not yet implemented
        return false;
    }
    if data.len() <= 93 {
        return ms32_polymod(data) == MS32_CONST;
    }
    false
}

/// Create a codex32 checksum
///
/// # Arguments
/// * `data` - The data part without checksum (as 5-bit values)
///
/// # Returns
/// 13 checksum values (5-bit each)
pub fn ms32_create_checksum(data: &[u8]) -> Vec<u8> {
    let mut values = data.to_vec();
    values.extend_from_slice(&[0; 13]);
    let polymod = ms32_polymod(&values) ^ MS32_CONST;
    (0..13)
        .map(|i| ((polymod >> (5 * (12 - i))) & 31) as u8)
        .collect()
}

/// Multiply two elements in GF(32) (bech32 field)
fn bech32_mul(a: u8, b: u8) -> u8 {
    let mut res = 0u8;
    let mut aa = a;
    for i in 0..5 {
        if (b >> i) & 1 != 0 {
            res ^= aa;
        }
        aa <<= 1;
        if aa >= 32 {
            aa ^= 41; // x^5 + x^3 + 1 (the reducing polynomial for GF(32))
        }
    }
    res
}

/// Compute Lagrange coefficients for interpolation at point x
fn bech32_lagrange(indices: &[u8], x: u8) -> Vec<u8> {
    // Compute numerator product: ∏(x - x_j) for all j
    let mut n = 1u8;
    let mut c = Vec::new();

    for &i in indices {
        n = bech32_mul(n, i ^ x);
        // Compute denominator product for this index: ∏(x_i - x_j) for j ≠ i
        let mut m = 1u8;
        for &j in indices {
            m = bech32_mul(m, if i == j { x } else { i } ^ j);
        }
        c.push(m);
    }

    // Final Lagrange coefficients: n / m_i using multiplicative inverse
    c.iter()
        .map(|&m| bech32_mul(n, BECH32_INV[m as usize]))
        .collect()
}

/// Interpolate shares at a target index
///
/// # Arguments
/// * `shares` - List of shares, each as a vector of 5-bit values
/// * `target_index` - The x value to interpolate to (16 = 's' for secret recovery)
///
/// # Returns
/// Interpolated values at the target index
pub fn ms32_interpolate(shares: &[Vec<u8>], target_index: u8) -> Vec<u8> {
    if shares.is_empty() || shares[0].is_empty() {
        return Vec::new();
    }

    // Extract share indices (position 5 in the data)
    let indices: Vec<u8> = shares.iter().map(|s| s[5]).collect();
    let weights = bech32_lagrange(&indices, target_index);

    let mut result = vec![0u8; shares[0].len()];
    for i in 0..result.len() {
        let mut n = 0u8;
        for j in 0..shares.len() {
            n ^= bech32_mul(weights[j], shares[j][i]);
        }
        result[i] = n;
    }

    // Fix the share index in the result to be the target
    result[5] = target_index;

    result
}

/// Recover a codex32 secret from shares
///
/// # Arguments
/// * `shares` - List of codex32 shares (threshold shares required)
///
/// # Returns
/// The recovered codex32 secret
pub fn ms32_recover(shares: &[Codex32Share]) -> Result<Codex32Share, ShamirError> {
    if shares.is_empty() {
        return Err(ShamirError::InsufficientShares);
    }

    let threshold = shares[0].threshold as usize;
    if shares.len() < threshold {
        return Err(ShamirError::InsufficientShares);
    }

    // Convert shares to data arrays
    let share_data: Result<Vec<Vec<u8>>, _> = shares.iter().map(|s| decode_data(&s.encoded)).collect();
    let share_data = share_data?;

    // Interpolate to x=16 (which is 's' in bech32 - the secret index)
    let secret_index = char_to_value('s').unwrap();
    let secret_data = ms32_interpolate(&share_data[..threshold], secret_index);

    // Encode back to string
    let encoded = encode_data(&secret_data);
    parse_share(&encoded)
}

/// Generate Codex32 shares from a master seed
///
/// # Arguments
/// * `seed` - The BIP-32 master seed (16-64 bytes)
/// * `config` - Configuration for share generation
///
/// # Returns
/// Vector of Codex32 shares
pub fn generate_shares(seed: &[u8], config: &Codex32Config) -> Result<Vec<Codex32Share>, ShamirError> {
    if seed.len() < 16 || seed.len() > 64 {
        return Err(ShamirError::InvalidShare(
            "Seed must be 16-64 bytes".into(),
        ));
    }

    // First, create the secret share (index 's')
    let secret = create_codex32_secret(seed, &config.identifier, config.threshold)?;
    let secret_data = decode_data(&secret.encoded)?;

    // Generate k-1 random shares
    let mut shares = Vec::with_capacity(config.total_shares as usize);
    let mut share_data = vec![secret_data];

    // Share indices: a, c, d, e, f, g, h, j, k, l, m, n, p, r, t, u, v, w, x, y, z, 2, 3, 4, 5, 6, 7, 8, 9
    // (skipping 'q' which is 0, 's' which is secret, and some others for clarity)
    let available_indices: Vec<char> = "acdeghjklmnprtuvwxyz234567890"
        .chars()
        .take((config.threshold - 1) as usize)
        .collect();

    for &idx_char in &available_indices {
        // Generate random payload of same length
        let random_share = create_random_share(
            &config.identifier,
            config.threshold,
            idx_char,
            seed.len(),
        )?;
        share_data.push(decode_data(&random_share.encoded)?);
        shares.push(random_share);
    }

    // Now derive additional shares using interpolation
    let remaining_indices: Vec<char> = "acdeghjklmnprtuvwxyz234567890"
        .chars()
        .skip((config.threshold - 1) as usize)
        .take((config.total_shares - config.threshold + 1) as usize)
        .collect();

    // Add the random shares to the output
    for derived_idx in remaining_indices {
        let target = char_to_value(derived_idx).unwrap();
        let derived_data = ms32_interpolate(&share_data[..config.threshold as usize], target);
        let encoded = encode_data(&derived_data);
        shares.push(parse_share(&encoded)?);
    }

    // Include the initial random shares
    Ok(shares)
}

/// Create a codex32 secret from a seed
fn create_codex32_secret(
    seed: &[u8],
    identifier: &str,
    threshold: u8,
) -> Result<Codex32Share, ShamirError> {
    // Convert seed to bech32 5-bit values
    let payload = bytes_to_bech32(seed);

    // Build data array for checksum: threshold_char_bech32_value + identifier + 's' + payload
    // The threshold is a digit character '0'-'9', we need its bech32 value
    let threshold_char = char::from_digit(threshold as u32, 10).unwrap_or('0');
    let threshold_bech32_value = char_to_value(threshold_char).ok_or_else(|| {
        ShamirError::InvalidShare(format!("Invalid threshold digit: {}", threshold))
    })?;

    let mut data = Vec::new();
    data.push(threshold_bech32_value);

    for c in identifier.chars() {
        data.push(char_to_value(c).ok_or_else(|| {
            ShamirError::InvalidShare(format!("Invalid identifier character: {}", c))
        })?);
    }

    data.push(char_to_value('s').unwrap()); // Secret index
    data.extend_from_slice(&payload);

    // Add checksum
    let checksum = ms32_create_checksum(&data);
    data.extend_from_slice(&checksum);

    // Build the encoded string with the digit character (not bech32)
    let mut encoded = String::from("ms1");
    encoded.push(threshold_char);
    for &v in &data[1..] {
        encoded.push(value_to_char(v).unwrap());
    }

    parse_share(&encoded)
}

/// Create a random share
fn create_random_share(
    identifier: &str,
    threshold: u8,
    index: char,
    seed_len: usize,
) -> Result<Codex32Share, ShamirError> {
    use rand::RngCore;

    // Calculate payload length in 5-bit values
    let payload_len = (seed_len * 8 + 4) / 5; // Ceiling division

    // Generate random payload
    let mut rng = rand::thread_rng();
    let mut payload = vec![0u8; payload_len];
    for p in payload.iter_mut() {
        let mut byte = [0u8; 1];
        rng.fill_bytes(&mut byte);
        *p = byte[0] & 31; // 5-bit value
    }

    // The threshold is a digit character '0'-'9', we need its bech32 value for checksum
    let threshold_char = char::from_digit(threshold as u32, 10).unwrap_or('0');
    let threshold_bech32_value = char_to_value(threshold_char).ok_or_else(|| {
        ShamirError::InvalidShare(format!("Invalid threshold digit: {}", threshold))
    })?;

    // Build data array for checksum
    let mut data = Vec::new();
    data.push(threshold_bech32_value);

    for c in identifier.chars() {
        data.push(char_to_value(c).ok_or_else(|| {
            ShamirError::InvalidShare(format!("Invalid identifier character: {}", c))
        })?);
    }

    data.push(char_to_value(index).ok_or_else(|| {
        ShamirError::InvalidShare(format!("Invalid share index: {}", index))
    })?);

    data.extend_from_slice(&payload);

    // Add checksum
    let checksum = ms32_create_checksum(&data);
    data.extend_from_slice(&checksum);

    // Build encoded string with digit character for threshold
    let mut encoded = String::from("ms1");
    encoded.push(threshold_char);
    for &v in &data[1..] {
        encoded.push(value_to_char(v).unwrap());
    }

    parse_share(&encoded)
}

/// Convert bytes to bech32 5-bit values
fn bytes_to_bech32(bytes: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;

    for &byte in bytes {
        acc = (acc << 8) | (byte as u32);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            result.push(((acc >> bits) & 31) as u8);
        }
    }

    // Pad remaining bits
    if bits > 0 {
        result.push(((acc << (5 - bits)) & 31) as u8);
    }

    result
}

/// Convert bech32 5-bit values to bytes
fn bech32_to_bytes(values: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;

    for &val in values {
        acc = (acc << 5) | (val as u32);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            result.push(((acc >> bits) & 255) as u8);
        }
    }

    // Discard incomplete byte (must be 4 bits or less per spec)
    result
}

/// Convert a bech32 character to its 5-bit value
fn char_to_value(c: char) -> Option<u8> {
    CHARSET.find(c.to_ascii_lowercase()).map(|i| i as u8)
}

/// Convert a 5-bit value to bech32 character
fn value_to_char(v: u8) -> Option<char> {
    if v < 32 {
        Some(CHARSET.chars().nth(v as usize).unwrap())
    } else {
        None
    }
}

/// Encode data array to codex32 string (all bytes as bech32 characters)
fn encode_data(data: &[u8]) -> String {
    let mut result = String::from("ms1");
    for &v in data {
        result.push(value_to_char(v).unwrap_or('?'));
    }
    result
}

/// Decode codex32 string to data array (all as bech32 values for checksum)
fn decode_data(s: &str) -> Result<Vec<u8>, ShamirError> {
    let s_lower = s.to_lowercase();
    if !s_lower.starts_with("ms1") {
        return Err(ShamirError::InvalidShare(
            "Codex32 string must start with 'ms1'".into(),
        ));
    }

    let data_part = &s_lower[3..];
    let mut result = Vec::new();

    for c in data_part.chars() {
        let v = char_to_value(c).ok_or_else(|| {
            ShamirError::InvalidShare(format!("Invalid bech32 character: {}", c))
        })?;
        result.push(v);
    }

    Ok(result)
}

/// Parse a codex32 string into a share
pub fn parse_share(encoded: &str) -> Result<Codex32Share, ShamirError> {
    let encoded_lower = encoded.to_lowercase();

    if !encoded_lower.starts_with("ms1") {
        return Err(ShamirError::InvalidShare(
            "Codex32 share must start with 'ms1'".into(),
        ));
    }

    let data = decode_data(&encoded_lower)?;

    // Verify checksum
    if !ms32_verify_checksum(&data) {
        return Err(ShamirError::InvalidShare("Invalid checksum".into()));
    }

    if data.len() < 19 {
        // 1 + 4 + 1 + 0 + 13 minimum
        return Err(ShamirError::InvalidShare("Codex32 share too short".into()));
    }

    // Get threshold from the ORIGINAL STRING character (it's a digit '0'-'9')
    let data_part = &encoded_lower[3..];
    let threshold_char = data_part.chars().next().unwrap();
    let threshold = threshold_char.to_digit(10).ok_or_else(|| {
        ShamirError::InvalidShare("Threshold must be a digit 0-9".into())
    })? as u8;

    if threshold != 0 && (threshold < 2 || threshold > 9) {
        return Err(ShamirError::InvalidShare(
            "Threshold must be 0 or 2-9".into(),
        ));
    }

    let identifier: String = (1..5).map(|i| value_to_char(data[i]).unwrap()).collect();

    let index = value_to_char(data[5]).unwrap();

    // Validate: threshold 0 requires index 's'
    if threshold == 0 && index != 's' {
        return Err(ShamirError::InvalidShare(
            "Threshold 0 requires share index 's'".into(),
        ));
    }

    // Payload is everything between index and checksum
    let payload_end = data.len() - 13;
    let payload_values = &data[6..payload_end];
    let payload = bech32_to_bytes(payload_values);

    Ok(Codex32Share {
        threshold,
        identifier,
        index,
        payload,
        encoded: encoded_lower,
    })
}

/// Combine Codex32 shares to recover the master seed
pub fn combine_shares(shares: &[Codex32Share]) -> Result<Vec<u8>, ShamirError> {
    let secret = ms32_recover(shares)?;
    Ok(secret.payload)
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
        assert!(Codex32Config::new(2, "TEST", 3).is_ok()); // uppercase OK (gets lowercased)
        assert!(Codex32Config::new(2, "ab!d", 3).is_err()); // invalid char in identifier
    }

    #[test]
    fn test_bech32_mul() {
        // Basic multiplication tests
        assert_eq!(bech32_mul(0, 5), 0);
        assert_eq!(bech32_mul(1, 5), 5);
        assert_eq!(bech32_mul(5, 1), 5);
    }

    #[test]
    fn test_bytes_to_bech32_roundtrip() {
        let original = vec![0x31, 0x8c, 0x63, 0x18, 0xc6, 0x31, 0x8c, 0x63];
        let bech32_vals = bytes_to_bech32(&original);
        let recovered = bech32_to_bytes(&bech32_vals);
        assert_eq!(recovered, original);
    }

    #[test]
    fn test_vector_1_parse() {
        // Test vector 1 from BIP-93
        let codex32_secret = "ms10testsxxxxxxxxxxxxxxxxxxxxxxxxxx4nzvca9cmczlw";
        let result = parse_share(codex32_secret);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

        let share = result.unwrap();
        assert_eq!(share.threshold, 0);
        assert_eq!(share.identifier, "test");
        assert_eq!(share.index, 's');
        assert_eq!(
            hex::encode(&share.payload),
            "318c6318c6318c6318c6318c6318c631"
        );
    }

    #[test]
    fn test_checksum_verification() {
        // Valid checksum from test vector 1
        let valid = "ms10testsxxxxxxxxxxxxxxxxxxxxxxxxxx4nzvca9cmczlw";
        let data = decode_data(valid).unwrap();
        assert!(ms32_verify_checksum(&data));

        // Invalid checksum (modified last character)
        let invalid = "ms10testsxxxxxxxxxxxxxxxxxxxxxxxxxx4nzvca9cmczlx";
        let data = decode_data(invalid).unwrap();
        assert!(!ms32_verify_checksum(&data));
    }

    #[test]
    fn test_create_checksum() {
        // Parse a known valid string and verify its checksum
        let valid = "ms10testsxxxxxxxxxxxxxxxxxxxxxxxxxx4nzvca9cmczlw";
        let data = decode_data(valid).unwrap();

        // Split into data and checksum
        let data_only = &data[..data.len() - 13];
        let existing_checksum = &data[data.len() - 13..];

        // Create checksum should produce the same result
        let computed_checksum = ms32_create_checksum(data_only);
        assert_eq!(computed_checksum.len(), 13);
        assert_eq!(computed_checksum, existing_checksum);

        // Full data should verify
        assert!(ms32_verify_checksum(&data));
    }

    #[test]
    fn test_vector_2_recovery() {
        // Test vector 2: 2-of-N scheme with identifier "name"
        let share_a = "ms12namea320zyxwvutsrqpnmlkjhgfedcaxrpp870hkkqrm";
        let share_c = "ms12namecacdefghjklmnpqrstuvwxyz023ftr2gdzmpy6pn";

        let parsed_a = parse_share(share_a).unwrap();
        let parsed_c = parse_share(share_c).unwrap();

        // Both should have threshold 2
        assert_eq!(parsed_a.threshold, 2);
        assert_eq!(parsed_c.threshold, 2);

        // Recover secret
        let shares = vec![parsed_a, parsed_c];
        let secret = ms32_recover(&shares).unwrap();

        assert_eq!(secret.index, 's');
        assert_eq!(
            hex::encode(&secret.payload),
            "d1808e096b35b209ca12132b264662a5"
        );
    }

    #[test]
    fn test_invalid_vectors() {
        // These should all fail checksum verification
        let invalid_checksums = [
            "ms10fauxsxxxxxxxxxxxxxxxxxxxxxxxxxxve740yyge2ghq",
            "ms10fauxsxxxxxxxxxxxxxxxxxxxxxxxxxxve740yyge2ghp",
        ];

        for invalid in invalid_checksums {
            let result = parse_share(invalid);
            assert!(result.is_err(), "Should have failed: {}", invalid);
        }
    }

    #[test]
    fn test_generate_and_recover() {
        let seed = vec![0x42u8; 16]; // 128-bit seed
        let config = Codex32Config::new(2, "cash", 3).unwrap();

        let shares = generate_shares(&seed, &config).unwrap();
        assert_eq!(shares.len(), 3);

        // All shares should have valid checksums
        for share in &shares {
            let data = decode_data(&share.encoded).unwrap();
            assert!(ms32_verify_checksum(&data), "Invalid checksum for share");
        }

        // Recover with any 2 shares
        let recovered = combine_shares(&shares[0..2]).unwrap();
        assert_eq!(recovered, seed);
    }
}
