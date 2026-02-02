//! NoString Email Module
//!
//! Encrypted email using Nostr identity, adapted from nostr-mail.
//!
//! # Features
//!
//! - SMTP sending with NIP-44 encryption
//! - IMAP fetching with automatic decryption
//! - Contact discovery via Nostr relays
//! - Email archival

// TODO: Port from nostr-mail:
// - email.rs (SMTP/IMAP operations)
// - nostr.rs (DM sync, profile lookup)
// - crypto.rs (NIP-44 encryption)

pub mod contacts;
pub mod fetch;
pub mod send;
