//! SLIP-39: Shamir's Secret-Sharing for Mnemonic Codes
//!
//! Implementation of SLIP-0039 for encoding shares as mnemonic words.
//! https://github.com/satoshilabs/slips/blob/master/slip-0039.md
//!
//! SLIP-39 uses:
//! - 1024-word wordlist (10 bits per word)
//! - RS1024 checksum (Reed-Solomon)
//! - Groups and members for hierarchical splitting

use crate::rs1024::{rs1024_create_checksum, rs1024_verify_checksum, CS_SHAMIR_EXTENDABLE};
use crate::shamir::{reconstruct_secret, split_secret, Share};
use crate::wordlist::{index_to_word, word_to_index};
use crate::ShamirError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A SLIP-39 mnemonic share
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slip39Share {
    /// Share identifier (common across all shares)
    pub identifier: u16,
    /// Group index (for multi-group setups)
    pub group_index: u8,
    /// Group threshold
    pub group_threshold: u8,
    /// Group count
    pub group_count: u8,
    /// Member index within group
    pub member_index: u8,
    /// Member threshold within group
    pub member_threshold: u8,
    /// Share data (the actual secret fragment)
    pub share_value: Vec<u8>,
    /// Full mnemonic words
    pub words: Vec<String>,
}

/// Configuration for SLIP-39 generation
#[derive(Debug, Clone)]
pub struct Slip39Config {
    /// Random identifier (0-32767)
    pub identifier: Option<u16>,
    /// Passphrase for extra encryption (optional)
    pub passphrase: Option<String>,
    /// Groups: Vec<(threshold, count)>
    pub groups: Vec<(u8, u8)>,
    /// Overall threshold of groups needed
    pub group_threshold: u8,
}

impl Default for Slip39Config {
    fn default() -> Self {
        Self {
            identifier: None,
            passphrase: None,
            // Single group with 2-of-3
            groups: vec![(2, 3)],
            group_threshold: 1,
        }
    }
}

impl Slip39Config {
    /// Simple 2-of-3 setup (single group)
    pub fn two_of_three() -> Self {
        Self {
            groups: vec![(2, 3)],
            ..Default::default()
        }
    }

    /// Simple 3-of-5 setup (single group)
    pub fn three_of_five() -> Self {
        Self {
            groups: vec![(3, 5)],
            ..Default::default()
        }
    }

    /// Multi-group setup
    pub fn with_groups(group_threshold: u8, groups: Vec<(u8, u8)>) -> Self {
        Self {
            group_threshold,
            groups,
            ..Default::default()
        }
    }
}

/// Generate SLIP-39 shares from a master secret
///
/// # Arguments
/// * `master_secret` - The entropy to split (e.g., 16, 20, 24, 28, or 32 bytes for BIP-39)
/// * `config` - SLIP-39 configuration
///
/// # Returns
/// Vec of groups, each containing Vec of shares
pub fn generate_shares(
    master_secret: &[u8],
    config: &Slip39Config,
) -> Result<Vec<Vec<Slip39Share>>, ShamirError> {
    // Validate secret length (SLIP-39 requires 128-256 bits in 16-bit increments)
    if master_secret.len() < 16 || master_secret.len() > 32 {
        return Err(ShamirError::InvalidShare(
            "Master secret must be 16-32 bytes".into(),
        ));
    }

    let identifier = config.identifier.unwrap_or_else(|| {
        let mut rng = rand::thread_rng();
        (rand::RngCore::next_u32(&mut rng) & 0x7FFF) as u16
    });

    let group_count = config.groups.len() as u8;
    let mut all_groups = Vec::new();

    // For each group, split the master secret
    for (group_idx, &(member_threshold, member_count)) in config.groups.iter().enumerate() {
        let raw_shares = split_secret(master_secret, member_threshold, member_count)?;

        let group_shares: Vec<Slip39Share> = raw_shares
            .into_iter()
            .map(|share| {
                let words = encode_share_to_words(
                    identifier,
                    group_idx as u8,
                    config.group_threshold,
                    group_count,
                    share.index - 1, // SLIP-39 uses 0-indexed members
                    member_threshold,
                    &share.data,
                );

                Slip39Share {
                    identifier,
                    group_index: group_idx as u8,
                    group_threshold: config.group_threshold,
                    group_count,
                    member_index: share.index - 1,
                    member_threshold,
                    share_value: share.data,
                    words,
                }
            })
            .collect();

        all_groups.push(group_shares);
    }

    Ok(all_groups)
}

/// Combine SLIP-39 shares to recover the master secret
pub fn combine_shares(shares: &[Slip39Share]) -> Result<Vec<u8>, ShamirError> {
    if shares.is_empty() {
        return Err(ShamirError::InsufficientShares);
    }

    // Group shares by group_index
    let mut groups: HashMap<u8, Vec<&Slip39Share>> = HashMap::new();
    for share in shares {
        groups.entry(share.group_index).or_default().push(share);
    }

    // For now, support single-group reconstruction
    // Full implementation would handle multi-group hierarchical reconstruction
    if groups.len() > 1 {
        return Err(ShamirError::InvalidShare(
            "Multi-group reconstruction not yet implemented".into(),
        ));
    }

    let (_, group_shares) = groups.into_iter().next().unwrap();

    // Check we have enough shares
    if group_shares.len() < group_shares[0].member_threshold as usize {
        return Err(ShamirError::InsufficientShares);
    }

    // Convert to raw shares for reconstruction
    let raw_shares: Vec<Share> = group_shares
        .iter()
        .map(|s| Share {
            index: s.member_index + 1, // Convert back to 1-indexed
            data: s.share_value.clone(),
        })
        .collect();

    reconstruct_secret(&raw_shares)
}

/// Push `num_bits` bits of a value to the bit vector (MSB first)
fn push_bits(bits: &mut Vec<bool>, value: u16, num_bits: usize) {
    for i in (0..num_bits).rev() {
        bits.push((value >> i) & 1 != 0);
    }
}

/// Encode share data to mnemonic words
fn encode_share_to_words(
    identifier: u16,
    group_index: u8,
    group_threshold: u8,
    group_count: u8,
    member_index: u8,
    member_threshold: u8,
    share_data: &[u8],
) -> Vec<String> {
    // Build the share data structure per SLIP-39 format:
    // - ID: 15 bits
    // - Extendable flag: 1 bit (ext=1 for modern shares)
    // - Iteration exponent: 4 bits
    // - Group index: 4 bits
    // - Group threshold - 1: 4 bits
    // - Group count - 1: 4 bits
    // - Member index: 4 bits
    // - Member threshold - 1: 4 bits
    // - Share value: variable (8 bits per byte)
    // - Checksum: 30 bits

    let mut bits = Vec::new();

    push_bits(&mut bits, identifier, 15);
    push_bits(&mut bits, 1, 1); // Extendable flag (1 = extendable)
    push_bits(&mut bits, 0, 4); // Iteration exponent (0 = no PBKDF2)
    push_bits(&mut bits, group_index as u16, 4);
    push_bits(&mut bits, (group_threshold - 1) as u16, 4);
    push_bits(&mut bits, (group_count - 1) as u16, 4);
    push_bits(&mut bits, member_index as u16, 4);
    push_bits(&mut bits, (member_threshold - 1) as u16, 4);

    // Share value (8 bits per byte)
    for &byte in share_data {
        push_bits(&mut bits, byte as u16, 8);
    }

    // Pad to 10-bit boundary
    while bits.len() % 10 != 0 {
        bits.push(false);
    }

    // Convert bits to 10-bit values (data only, no checksum yet)
    let mut data_values: Vec<u16> = Vec::new();
    for chunk in bits.chunks(10) {
        let mut word_index = 0u16;
        for (i, &bit) in chunk.iter().enumerate() {
            if bit {
                word_index |= 1 << (9 - i);
            }
        }
        data_values.push(word_index);
    }

    // Create RS1024 checksum (3 words = 30 bits)
    let checksum = rs1024_create_checksum(CS_SHAMIR_EXTENDABLE, &data_values);
    data_values.extend_from_slice(&checksum);

    // Convert all values (including checksum) to words
    let mut words = Vec::new();
    for word_index in data_values {
        let word = index_to_word(word_index).expect("Index always valid (10 bits = 0-1023)");
        words.push(word.to_string());
    }

    words
}

/// Parse mnemonic words back to a share
pub fn parse_mnemonic(words: &[String]) -> Result<Slip39Share, ShamirError> {
    // Convert words to 10-bit values
    let mut values: Vec<u16> = Vec::new();
    for word in words {
        let idx = word_to_index(word.as_str())
            .ok_or_else(|| ShamirError::InvalidShare(format!("Unknown word: {}", word)))?;
        values.push(idx);
    }

    // Verify RS1024 checksum
    if !rs1024_verify_checksum(CS_SHAMIR_EXTENDABLE, &values) {
        return Err(ShamirError::InvalidShare("Invalid RS1024 checksum".into()));
    }

    // Convert values to bits for parsing
    let mut bits = Vec::new();
    for &val in &values {
        for i in (0..10).rev() {
            bits.push((val >> i) & 1 != 0);
        }
    }

    // Parse header (39 bits)
    if bits.len() < 39 {
        return Err(ShamirError::InvalidShare("Mnemonic too short".into()));
    }

    let identifier = bits_to_u16(&bits[0..15]);
    // Skip iteration exponent (bits 15..19) and extendable flag (bit 15)
    let group_index = bits_to_u8(&bits[20..24]);
    let group_threshold = bits_to_u8(&bits[24..28]) + 1;
    let group_count = bits_to_u8(&bits[28..32]) + 1;
    let member_index = bits_to_u8(&bits[32..36]);
    let member_threshold = bits_to_u8(&bits[36..40]) + 1;

    // Share value starts at bit 40, ends 30 bits before end (checksum)
    let share_bits_end = bits.len() - 30;
    let mut share_value = Vec::new();
    for chunk in bits[40..share_bits_end].chunks(8) {
        if chunk.len() == 8 {
            share_value.push(bits_to_u8(chunk));
        }
    }

    Ok(Slip39Share {
        identifier,
        group_index,
        group_threshold,
        group_count,
        member_index,
        member_threshold,
        share_value,
        words: words.to_vec(),
    })
}

/// Convert a slice of bits to an integer (generic over output type)
fn bits_to_int<T>(bits: &[bool]) -> T
where
    T: From<u8> + std::ops::BitOrAssign + std::ops::Shl<usize, Output = T> + Default + Copy,
{
    let mut val = T::default();
    for (i, &bit) in bits.iter().enumerate() {
        if bit {
            val |= T::from(1) << (bits.len() - 1 - i);
        }
    }
    val
}

#[inline]
fn bits_to_u16(bits: &[bool]) -> u16 {
    bits_to_int(bits)
}

#[inline]
fn bits_to_u8(bits: &[bool]) -> u8 {
    bits_to_int(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_combine_2_of_3() {
        let master_secret = vec![0x42u8; 16]; // 128-bit secret
        let config = Slip39Config::two_of_three();

        let groups = generate_shares(&master_secret, &config).unwrap();
        assert_eq!(groups.len(), 1); // Single group
        assert_eq!(groups[0].len(), 3); // 3 shares

        // Combine with first 2 shares
        let recovered = combine_shares(&groups[0][0..2]).unwrap();
        assert_eq!(recovered, master_secret);

        // Combine with last 2 shares
        let recovered = combine_shares(&groups[0][1..3]).unwrap();
        assert_eq!(recovered, master_secret);
    }

    #[test]
    fn test_generate_and_combine_3_of_5() {
        let master_secret = vec![0xABu8; 32]; // 256-bit secret
        let config = Slip39Config::three_of_five();

        let groups = generate_shares(&master_secret, &config).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 5);

        // Combine with first 3 shares
        let recovered = combine_shares(&groups[0][0..3]).unwrap();
        assert_eq!(recovered, master_secret);
    }

    #[test]
    fn test_shares_have_words() {
        let master_secret = vec![0x42u8; 16];
        let config = Slip39Config::two_of_three();

        let groups = generate_shares(&master_secret, &config).unwrap();

        for share in &groups[0] {
            assert!(!share.words.is_empty());
            println!("Share {}: {} words", share.member_index, share.words.len());
            println!("  {}", share.words.join(" "));
        }
    }

    #[test]
    fn test_mnemonic_roundtrip() {
        // This test verifies that encoding a share to mnemonic words and parsing
        // it back recovers the original share data.
        
        let master_secret = vec![0x42u8; 16];
        let config = Slip39Config::two_of_three();

        let groups = generate_shares(&master_secret, &config).unwrap();
        let original_share = &groups[0][0];

        // Parse the mnemonic back
        let parsed = parse_mnemonic(&original_share.words).unwrap();

        // With full 1024-word wordlist, these should all match
        assert_eq!(parsed.identifier, original_share.identifier);
        assert_eq!(parsed.group_index, original_share.group_index);
        assert_eq!(parsed.member_index, original_share.member_index);
        assert_eq!(parsed.share_value, original_share.share_value);
    }
}
