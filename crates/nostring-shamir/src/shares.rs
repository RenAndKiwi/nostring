//! Share management
//!
//! Common types and utilities for both SLIP-39 and Codex32 shares.

use serde::{Serialize, Deserialize};

/// A single Shamir share
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share {
    /// Share index (1-based)
    pub index: u8,
    /// Share data (format depends on scheme)
    pub data: ShareData,
    /// Human-readable label (e.g., "Spouse", "Safe Deposit Box")
    pub label: Option<String>,
}

/// Share data format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShareData {
    /// SLIP-39 word list
    Slip39(Vec<String>),
    /// Codex32 Bech32 string
    Codex32(String),
}
