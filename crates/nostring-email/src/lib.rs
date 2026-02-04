//! NoString Email Module
//!
//! Email operations for inheritance notification and share delivery.
//!
//! # Architecture
//!
//! This crate provides:
//! - **Sending**: SMTP email delivery for share distribution and notifications
//! - **Fetching**: IMAP email retrieval for heirs recovering shares from email
//! - **Contacts**: Nostr-based contact discovery (NIP-05 → email mapping)
//!
//! For simple notification emails, see `nostring-notify` which handles
//! templated messages. This crate handles the lower-level email operations
//! needed for inheritance workflows.

pub mod contacts;
pub mod fetch;
pub mod send;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmailError {
    #[error("SMTP error: {0}")]
    Smtp(String),

    #[error("IMAP error: {0}")]
    Imap(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Message not found: {0}")]
    NotFound(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("NIP-05 lookup failed: {0}")]
    Nip05(String),
}
