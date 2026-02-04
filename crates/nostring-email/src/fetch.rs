//! Email fetching via IMAP
//!
//! Allows heirs to retrieve Shamir shares or descriptor backups
//! that were delivered via email. Searches by subject/sender
//! and extracts the relevant content.

use crate::EmailError;
use serde::{Deserialize, Serialize};

/// IMAP configuration for fetching emails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
    /// IMAP server hostname
    pub host: String,
    /// IMAP port (993 for TLS, 143 for STARTTLS)
    pub port: u16,
    /// IMAP username
    pub username: String,
    /// IMAP password
    pub password: String,
    /// Use TLS (recommended)
    #[serde(default = "default_true")]
    pub tls: bool,
}

fn default_true() -> bool {
    true
}

/// A fetched email message.
#[derive(Debug, Clone)]
pub struct FetchedEmail {
    /// Message sequence number
    pub seq: u32,
    /// Subject line
    pub subject: String,
    /// Sender address
    pub from: String,
    /// Plain text body
    pub body: String,
}

/// Search for NoString share emails in the inbox.
///
/// Looks for emails with subjects matching the NoString share delivery pattern.
pub fn fetch_share_emails(config: &ImapConfig) -> Result<Vec<FetchedEmail>, EmailError> {
    fetch_by_subject(config, "NoString: Your inheritance share")
}

/// Search for NoString descriptor backup emails.
pub fn fetch_descriptor_emails(config: &ImapConfig) -> Result<Vec<FetchedEmail>, EmailError> {
    fetch_by_subject(config, "NoString: Inheritance descriptor backup")
}

/// Search for emails matching a subject pattern.
pub fn fetch_by_subject(
    config: &ImapConfig,
    subject_pattern: &str,
) -> Result<Vec<FetchedEmail>, EmailError> {
    let tls_kind = if config.tls {
        imap::TlsKind::Native
    } else {
        imap::TlsKind::Any
    };

    let client = imap::ClientBuilder::new(&config.host, config.port)
        .tls_kind(tls_kind)
        .connect()
        .map_err(|e| EmailError::Connection(format!("IMAP connect failed: {}", e)))?;

    let mut session = client
        .login(&config.username, &config.password)
        .map_err(|(e, _)| EmailError::Auth(format!("IMAP login failed: {}", e)))?;

    session
        .select("INBOX")
        .map_err(|e| EmailError::Imap(format!("Failed to select INBOX: {}", e)))?;

    // Search by subject
    let search_query = format!("SUBJECT \"{}\"", subject_pattern);
    let message_seqs = session
        .search(&search_query)
        .map_err(|e| EmailError::Imap(format!("Search failed: {}", e)))?;

    let mut emails = Vec::new();

    if message_seqs.is_empty() {
        let _ = session.logout();
        return Ok(emails);
    }

    // Fetch matching messages
    let seq_list: Vec<String> = message_seqs.iter().map(|s| s.to_string()).collect();
    let seq_set = seq_list.join(",");

    let messages = session
        .fetch(&seq_set, "RFC822")
        .map_err(|e| EmailError::Imap(format!("Fetch failed: {}", e)))?;

    for message in messages.iter() {
        if let Some(body_bytes) = message.body() {
            match parse_email(message.message, body_bytes) {
                Ok(email) => emails.push(email),
                Err(e) => {
                    log::warn!("Failed to parse email: {}", e);
                }
            }
        }
    }

    let _ = session.logout();
    Ok(emails)
}

/// Extract a Shamir share from an email body.
///
/// Looks for Codex32-formatted shares (start with "ms1" or "MS1").
pub fn extract_share_from_body(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("ms1") || trimmed.starts_with("MS1") {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Extract a descriptor backup from an email body.
///
/// Looks for content between === DESCRIPTOR BACKUP === markers.
pub fn extract_descriptor_from_body(body: &str) -> Option<String> {
    let start_marker = "=== DESCRIPTOR BACKUP ===";
    let end_marker = "=== END BACKUP ===";

    let start = body.find(start_marker)?;
    let end = body.find(end_marker)?;

    if start < end {
        let content = &body[start + start_marker.len()..end];
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn parse_email(seq: u32, raw: &[u8]) -> Result<FetchedEmail, EmailError> {
    let raw_str = String::from_utf8_lossy(raw);

    let subject =
        extract_header(&raw_str, "Subject").unwrap_or_else(|| "(no subject)".to_string());
    let from = extract_header(&raw_str, "From").unwrap_or_else(|| "(unknown)".to_string());

    // Extract body (after the first blank line)
    let body = if let Some(pos) = raw_str.find("\r\n\r\n") {
        raw_str[pos + 4..].to_string()
    } else if let Some(pos) = raw_str.find("\n\n") {
        raw_str[pos + 2..].to_string()
    } else {
        raw_str.to_string()
    };

    Ok(FetchedEmail {
        seq,
        subject,
        from,
        body,
    })
}

fn extract_header(raw: &str, header_name: &str) -> Option<String> {
    let prefix = format!("{}: ", header_name);
    for line in raw.lines() {
        if line.starts_with(&prefix) {
            return Some(line[prefix.len()..].trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_share_from_body() {
        let body = r#"Hello Alice,

YOUR SHARE (keep this secret):
ms12nsecaxxxxxxxxxxxxxxxxxxxxxxxxxxx

OWNER'S IDENTITY:
npub1test...
"#;
        let share = extract_share_from_body(body);
        assert!(share.is_some());
        assert!(share.unwrap().starts_with("ms12nsecax"));
    }

    #[test]
    fn test_extract_descriptor_from_body() {
        let body = r#"Hello Bob,

=== DESCRIPTOR BACKUP ===
{"descriptor":"wsh(or_d(pk(...)))","network":"testnet"}
=== END BACKUP ===

WHAT TO DO:
..."#;
        let descriptor = extract_descriptor_from_body(body);
        assert!(descriptor.is_some());
        assert!(descriptor.unwrap().contains("descriptor"));
    }

    #[test]
    fn test_extract_share_no_match() {
        let body = "Just a regular email with no shares.";
        assert!(extract_share_from_body(body).is_none());
    }

    #[test]
    fn test_extract_descriptor_no_markers() {
        let body = "No descriptor backup here.";
        assert!(extract_descriptor_from_body(body).is_none());
    }

    #[test]
    fn test_parse_email() {
        let raw = b"From: sender@test.com\r\nSubject: Test Email\r\n\r\nHello world!";
        let email = parse_email(1, raw).unwrap();
        assert_eq!(email.subject, "Test Email");
        assert_eq!(email.from, "sender@test.com");
        assert!(email.body.contains("Hello world!"));
    }
}
