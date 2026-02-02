//! Share management utilities
//!
//! Common operations for working with Shamir shares regardless of encoding.

use crate::ShamirError;
use serde::{Deserialize, Serialize};

/// A generic share that can represent SLIP-39 or Codex32
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnyShare {
    /// SLIP-39 encoded share
    Slip39(crate::slip39::Slip39Share),
    /// Codex32 encoded share
    Codex32(crate::codex32::Codex32Share),
    /// Raw share (for internal use)
    Raw(crate::shamir::Share),
}

/// Share metadata for display purposes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareMetadata {
    /// Share index (1-indexed for display)
    pub index: u8,
    /// Threshold needed for reconstruction
    pub threshold: u8,
    /// Total shares in the set
    pub total: u8,
    /// Encoding format
    pub format: ShareFormat,
    /// Human-readable label (optional)
    pub label: Option<String>,
}

/// Share encoding format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareFormat {
    /// SLIP-39 mnemonic words
    Slip39,
    /// Codex32 bech32 string
    Codex32,
    /// Raw bytes (hex encoded for display)
    Raw,
}

/// QR code export options
#[derive(Debug, Clone)]
pub struct QrExportOptions {
    /// Include metadata in QR
    pub include_metadata: bool,
    /// Error correction level
    pub error_correction: QrErrorCorrection,
    /// Module size in pixels
    pub module_size: u8,
}

impl Default for QrExportOptions {
    fn default() -> Self {
        Self {
            include_metadata: true,
            error_correction: QrErrorCorrection::Medium,
            module_size: 4,
        }
    }
}

/// QR error correction levels
#[derive(Debug, Clone, Copy)]
pub enum QrErrorCorrection {
    Low,      // ~7% recovery
    Medium,   // ~15% recovery
    Quartile, // ~25% recovery
    High,     // ~30% recovery
}

/// Export a share as a string suitable for backup
pub fn export_share_string(share: &AnyShare) -> String {
    match share {
        AnyShare::Slip39(s) => s.words.join(" "),
        AnyShare::Codex32(s) => s.encoded.clone(),
        AnyShare::Raw(s) => {
            // Export as hex
            s.data.iter().map(|b| format!("{:02x}", b)).collect()
        }
    }
}

/// Parse a share from a string (auto-detect format)
pub fn parse_share_string(input: &str) -> Result<AnyShare, ShamirError> {
    let trimmed = input.trim();

    // Try Codex32 first (starts with "ms")
    if trimmed.starts_with("ms") {
        let share = crate::codex32::parse_share(trimmed)?;
        return Ok(AnyShare::Codex32(share));
    }

    // Try SLIP-39 (space-separated words)
    if trimmed.contains(' ') {
        let words: Vec<String> = trimmed.split_whitespace().map(|s| s.to_string()).collect();
        let share = crate::slip39::parse_mnemonic(&words)?;
        return Ok(AnyShare::Slip39(share));
    }

    // Try hex (raw)
    if trimmed.chars().all(|c| c.is_ascii_hexdigit()) && trimmed.len().is_multiple_of(2) {
        let data: Result<Vec<u8>, _> = (0..trimmed.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&trimmed[i..i + 2], 16))
            .collect();

        if let Ok(data) = data {
            return Ok(AnyShare::Raw(crate::shamir::Share { index: 0, data }));
        }
    }

    Err(ShamirError::InvalidShare(
        "Could not detect share format".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_format_detection() {
        // Codex32 format (valid test vector 1 from BIP-93)
        let codex32 = "ms10testsxxxxxxxxxxxxxxxxxxxxxxxxxx4nzvca9cmczlw";
        let result = parse_share_string(codex32);
        assert!(
            matches!(result, Ok(AnyShare::Codex32(_))),
            "Failed to parse codex32: {:?}",
            result
        );

        // Hex format
        let hex = "deadbeef";
        let result = parse_share_string(hex);
        assert!(matches!(result, Ok(AnyShare::Raw(_))));
    }
}
