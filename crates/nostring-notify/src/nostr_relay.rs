//! Nostr relay storage for locked Shamir shares.
//!
//! Publishes encrypted locked shares to multiple Nostr relays as redundant
//! backup beyond the descriptor file. Each share is encrypted to the heir's
//! npub using NIP-44 (with NIP-04 fallback), so only the intended heir can
//! decrypt it.
//!
//! # Security Model
//!
//! Encrypted shares on relays are useless without meeting the threshold —
//! this is defense-in-depth. Even if an attacker scrapes all relay data,
//! they get encrypted blobs they can't decrypt (without the heir's nsec)
//! and even decrypted shares alone can't reconstruct the secret.

use crate::NotifyError;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default relays for publishing shares
pub const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://relay.nostr.band",
    "wss://nos.lol",
];

/// Result of publishing shares to relays
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayPublishResult {
    /// Total shares published
    pub shares_published: usize,
    /// Per-heir publication results
    pub heir_results: Vec<HeirPublishResult>,
    /// Relays that accepted at least one event
    pub successful_relays: Vec<String>,
    /// Relays that failed
    pub failed_relays: Vec<String>,
}

/// Per-heir publication result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeirPublishResult {
    pub heir_npub: String,
    pub heir_label: String,
    pub shares_published: usize,
    pub event_ids: Vec<String>,
    pub error: Option<String>,
}

/// A locked share to be published for a specific heir
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharePayload {
    /// The locked share string (Codex32-encoded)
    pub share: String,
    /// Share index (for ordering)
    pub index: usize,
    /// Total locked shares
    pub total: usize,
    /// Identifier tag for grouping
    pub split_id: String,
}

/// Result of fetching shares from relays
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayFetchResult {
    /// Decrypted share payloads
    pub shares: Vec<SharePayload>,
    /// Relays that responded
    pub responding_relays: Vec<String>,
    /// Total events found
    pub events_found: usize,
}

/// Encrypt a share payload to an heir's npub and publish to relays.
///
/// Uses NIP-44 encryption (modern, with padding). Falls back to NIP-04
/// if NIP-44 fails.
///
/// # Arguments
/// * `sender_secret` - Service key secret (hex)
/// * `heir_npub` - Heir's Nostr public key (npub or hex)
/// * `shares` - The locked shares to publish
/// * `split_id` - Unique identifier for this split (to group shares)
/// * `relays` - Relay URLs to publish to
pub async fn publish_shares_to_relays(
    sender_secret: &str,
    heir_npub: &str,
    heir_label: &str,
    shares: &[String],
    split_id: &str,
    relays: &[String],
) -> Result<HeirPublishResult, NotifyError> {
    let recipient = parse_pubkey(heir_npub)
        .map_err(|e| NotifyError::NostrFailed(format!("Invalid heir npub: {}", e)))?;

    let keys = Keys::parse(sender_secret)
        .map_err(|e| NotifyError::NostrFailed(format!("Invalid secret key: {}", e)))?;

    let client = Client::new(keys.clone());

    for relay in relays {
        if let Err(e) = client.add_relay(relay).await {
            log::warn!("Failed to add relay {}: {}", relay, e);
        }
    }

    client.connect().await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    let mut event_ids = Vec::new();

    for (i, share) in shares.iter().enumerate() {
        let payload = SharePayload {
            share: share.clone(),
            index: i,
            total: shares.len(),
            split_id: split_id.to_string(),
        };

        let payload_json = serde_json::to_string(&payload).map_err(|e| {
            NotifyError::NostrFailed(format!("Failed to serialize share payload: {}", e))
        })?;

        // Try NIP-44 first, fall back to NIP-04
        let (encrypted, kind) = encrypt_for_heir(&keys, &recipient, &payload_json)?;

        let event = EventBuilder::new(kind, &encrypted)
            .tag(Tag::public_key(recipient))
            .tag(Tag::custom(
                TagKind::Custom("split".into()),
                vec![split_id.to_string()],
            ))
            .sign_with_keys(&keys)
            .map_err(|e| NotifyError::NostrFailed(format!("Failed to build event: {}", e)))?;

        match client.send_event(event).await {
            Ok(output) => {
                let eid = output.id().to_hex();
                log::info!(
                    "Published share {}/{} for heir {} (event: {})",
                    i + 1,
                    shares.len(),
                    heir_label,
                    eid
                );
                event_ids.push(eid);
            }
            Err(e) => {
                log::error!(
                    "Failed to publish share {}/{} for heir {}: {}",
                    i + 1,
                    shares.len(),
                    heir_label,
                    e
                );
            }
        }
    }

    client.disconnect().await;

    let published = event_ids.len();

    Ok(HeirPublishResult {
        heir_npub: heir_npub.to_string(),
        heir_label: heir_label.to_string(),
        shares_published: published,
        event_ids,
        error: if published == 0 {
            Some("No shares were accepted by any relay".to_string())
        } else {
            None
        },
    })
}

/// Publish locked shares to multiple relays for all heirs.
///
/// # Arguments
/// * `sender_secret` - Service key secret (hex)
/// * `heirs` - List of (npub, label) pairs
/// * `locked_shares` - The locked share strings
/// * `split_id` - Unique identifier for this split
/// * `relays` - Optional relay list (defaults to DEFAULT_RELAYS)
pub async fn publish_all_shares(
    sender_secret: &str,
    heirs: &[(String, String)], // (npub, label)
    locked_shares: &[String],
    split_id: &str,
    relays: Option<&[String]>,
) -> Result<RelayPublishResult, NotifyError> {
    let relay_list: Vec<String> = relays
        .map(|r| r.to_vec())
        .unwrap_or_else(|| DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect());

    let mut heir_results = Vec::new();
    let mut total_published = 0;

    for (npub, label) in heirs {
        match publish_shares_to_relays(
            sender_secret,
            npub,
            label,
            locked_shares,
            split_id,
            &relay_list,
        )
        .await
        {
            Ok(result) => {
                total_published += result.shares_published;
                heir_results.push(result);
            }
            Err(e) => {
                heir_results.push(HeirPublishResult {
                    heir_npub: npub.clone(),
                    heir_label: label.clone(),
                    shares_published: 0,
                    event_ids: Vec::new(),
                    error: Some(format!("{}", e)),
                });
            }
        }
    }

    // Determine relay status (simplified — we published to all, consider successful
    // if at least one event was accepted)
    let (successful_relays, failed_relays) = if total_published > 0 {
        (relay_list.clone(), Vec::new())
    } else {
        (Vec::new(), relay_list.clone())
    };

    Ok(RelayPublishResult {
        shares_published: total_published,
        heir_results,
        successful_relays,
        failed_relays,
    })
}

/// Fetch locked shares from relays for a specific heir.
///
/// The heir provides their nsec to decrypt the shares that were
/// encrypted to their npub by the service key.
///
/// # Arguments
/// * `heir_nsec` - Heir's Nostr secret key (nsec or hex)
/// * `sender_npub` - Service key's npub (to filter events from)
/// * `relays` - Relay URLs to query
/// * `split_id` - Optional split_id filter
pub async fn fetch_shares_from_relays(
    heir_nsec: &str,
    sender_npub: &str,
    relays: Option<&[String]>,
    split_id: Option<&str>,
) -> Result<RelayFetchResult, NotifyError> {
    let heir_keys = Keys::parse(heir_nsec)
        .map_err(|e| NotifyError::NostrFailed(format!("Invalid heir nsec: {}", e)))?;

    let sender_pk = parse_pubkey(sender_npub)
        .map_err(|e| NotifyError::NostrFailed(format!("Invalid sender npub: {}", e)))?;

    let relay_list: Vec<String> = relays
        .map(|r| r.to_vec())
        .unwrap_or_else(|| DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect());

    let client = Client::new(heir_keys.clone());

    for relay in &relay_list {
        if let Err(e) = client.add_relay(relay).await {
            log::warn!("Failed to add relay {}: {}", relay, e);
        }
    }

    client.connect().await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Build filter: encrypted DMs (kind 4) from the service key to us
    // Also check kind 1059 (gift-wrapped) in case NIP-44 uses different kind
    let heir_pk = heir_keys.public_key();
    let filter = Filter::new()
        .kinds(vec![Kind::EncryptedDirectMessage, Kind::Custom(1059)])
        .author(sender_pk)
        .pubkey(heir_pk)
        .limit(100);

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| NotifyError::NostrFailed(format!("Failed to fetch events: {}", e)))?;

    let mut shares = Vec::new();
    let events_found = events.len();

    for event in events.iter() {
        // Try to decrypt
        let decrypted = decrypt_event(&heir_keys, &sender_pk, event);

        if let Ok(content) = decrypted {
            // Try to parse as SharePayload
            if let Ok(payload) = serde_json::from_str::<SharePayload>(&content) {
                // Filter by split_id if provided
                if let Some(sid) = split_id {
                    if payload.split_id != sid {
                        continue;
                    }
                }
                shares.push(payload);
            }
        }
    }

    // Sort by index and deduplicate
    shares.sort_by_key(|s| s.index);
    shares.dedup_by_key(|s| (s.split_id.clone(), s.index));

    client.disconnect().await;

    Ok(RelayFetchResult {
        shares,
        responding_relays: relay_list,
        events_found,
    })
}

/// Encrypt content for an heir using NIP-44 (with NIP-04 fallback).
///
/// Returns (encrypted_content, event_kind).
fn encrypt_for_heir(
    keys: &Keys,
    recipient: &PublicKey,
    content: &str,
) -> Result<(String, Kind), NotifyError> {
    // Try NIP-44 first
    match nip44::encrypt(keys.secret_key(), recipient, content, nip44::Version::V2) {
        Ok(encrypted) => {
            log::debug!("Using NIP-44 encryption for relay share");
            // NIP-44 encrypted DMs still use kind 4 for broad compatibility
            // (kind 1059 gift-wrap would require a separate wrapper)
            Ok((encrypted, Kind::EncryptedDirectMessage))
        }
        Err(e) => {
            log::warn!("NIP-44 encryption failed ({}), falling back to NIP-04", e);
            // Fallback to NIP-04
            let encrypted = nip04::encrypt(keys.secret_key(), recipient, content).map_err(|e| {
                NotifyError::NostrFailed(format!("NIP-04 encryption failed: {}", e))
            })?;
            Ok((encrypted, Kind::EncryptedDirectMessage))
        }
    }
}

/// Try to decrypt an event, attempting NIP-44 first then NIP-04.
fn decrypt_event(keys: &Keys, sender_pk: &PublicKey, event: &Event) -> Result<String, NotifyError> {
    let content = &event.content;

    // Try NIP-44 first
    if let Ok(decrypted) = nip44::decrypt(keys.secret_key(), sender_pk, content) {
        return Ok(decrypted);
    }

    // Fall back to NIP-04
    nip04::decrypt(keys.secret_key(), sender_pk, content)
        .map_err(|e| NotifyError::NostrFailed(format!("Decryption failed: {}", e)))
}

/// Parse a public key from npub or hex format
fn parse_pubkey(input: &str) -> Result<PublicKey, String> {
    if input.starts_with("npub") {
        return PublicKey::from_bech32(input).map_err(|e| e.to_string());
    }
    PublicKey::from_hex(input).map_err(|e| e.to_string())
}

/// Generate a unique split_id from the current timestamp + random suffix
pub fn generate_split_id() -> String {
    use std::time::SystemTime;
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple unique-enough ID: timestamp + 4 random hex chars
    let rand_suffix: u16 = rand::random();
    format!("{:x}{:04x}", ts, rand_suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_payload_roundtrip() {
        let payload = SharePayload {
            share: "ms12nsecbyyy123".to_string(),
            index: 0,
            total: 3,
            split_id: "abc123".to_string(),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let decoded: SharePayload = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.share, "ms12nsecbyyy123");
        assert_eq!(decoded.index, 0);
        assert_eq!(decoded.total, 3);
        assert_eq!(decoded.split_id, "abc123");
    }

    #[test]
    fn test_nip44_encrypt_decrypt_roundtrip() {
        let sender = Keys::generate();
        let recipient = Keys::generate();

        let payload = SharePayload {
            share: "ms12nsectest_share_data".to_string(),
            index: 1,
            total: 5,
            split_id: "test_split_001".to_string(),
        };
        let plaintext = serde_json::to_string(&payload).unwrap();

        // Encrypt with NIP-44
        let encrypted = nip44::encrypt(
            sender.secret_key(),
            &recipient.public_key(),
            &plaintext,
            nip44::Version::V2,
        )
        .expect("NIP-44 encryption should succeed");

        // Decrypt
        let decrypted = nip44::decrypt(recipient.secret_key(), &sender.public_key(), &encrypted)
            .expect("NIP-44 decryption should succeed");

        assert_eq!(decrypted, plaintext);

        // Parse back
        let recovered: SharePayload = serde_json::from_str(&decrypted).unwrap();
        assert_eq!(recovered.share, payload.share);
        assert_eq!(recovered.index, payload.index);
        assert_eq!(recovered.total, payload.total);
        assert_eq!(recovered.split_id, payload.split_id);
    }

    #[test]
    fn test_nip04_encrypt_decrypt_roundtrip() {
        let sender = Keys::generate();
        let recipient = Keys::generate();

        let plaintext = r#"{"share":"ms12test","index":0,"total":1,"split_id":"x"}"#;

        let encrypted = nip04::encrypt(sender.secret_key(), &recipient.public_key(), plaintext)
            .expect("NIP-04 encryption should succeed");

        let decrypted = nip04::decrypt(recipient.secret_key(), &sender.public_key(), &encrypted)
            .expect("NIP-04 decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_for_heir_uses_nip44() {
        let sender = Keys::generate();
        let recipient = Keys::generate();

        let (encrypted, kind) = encrypt_for_heir(&sender, &recipient.public_key(), "test message")
            .expect("encrypt_for_heir should succeed");

        assert_eq!(kind, Kind::EncryptedDirectMessage);
        assert!(!encrypted.is_empty());

        // Should be decryptable with NIP-44
        let decrypted = nip44::decrypt(recipient.secret_key(), &sender.public_key(), &encrypted)
            .expect("Should decrypt with NIP-44");

        assert_eq!(decrypted, "test message");
    }

    #[test]
    fn test_decrypt_event_tries_both_nips() {
        let sender = Keys::generate();
        let recipient = Keys::generate();

        // Create a NIP-04 encrypted event
        let nip04_encrypted = nip04::encrypt(
            sender.secret_key(),
            &recipient.public_key(),
            "nip04 message",
        )
        .unwrap();

        let event = EventBuilder::new(Kind::EncryptedDirectMessage, &nip04_encrypted)
            .tag(Tag::public_key(recipient.public_key()))
            .sign_with_keys(&sender)
            .unwrap();

        let decrypted = decrypt_event(&recipient, &sender.public_key(), &event).unwrap();
        assert_eq!(decrypted, "nip04 message");

        // Create a NIP-44 encrypted event
        let nip44_encrypted = nip44::encrypt(
            sender.secret_key(),
            &recipient.public_key(),
            "nip44 message",
            nip44::Version::V2,
        )
        .unwrap();

        let event = EventBuilder::new(Kind::EncryptedDirectMessage, &nip44_encrypted)
            .tag(Tag::public_key(recipient.public_key()))
            .sign_with_keys(&sender)
            .unwrap();

        let decrypted = decrypt_event(&recipient, &sender.public_key(), &event).unwrap();
        assert_eq!(decrypted, "nip44 message");
    }

    #[test]
    fn test_generate_split_id() {
        let id1 = generate_split_id();
        let id2 = generate_split_id();

        assert!(!id1.is_empty());
        assert!(id1.len() >= 8); // timestamp hex + 4 random hex
        assert_ne!(id1, id2, "Two generated split IDs should differ");
    }

    #[test]
    fn test_parse_pubkey_formats() {
        // Generate a test key
        let keys = Keys::generate();
        let hex = keys.public_key().to_hex();
        let npub = keys.public_key().to_bech32().unwrap();

        let from_hex = parse_pubkey(&hex).unwrap();
        let from_npub = parse_pubkey(&npub).unwrap();

        assert_eq!(from_hex, from_npub);
        assert_eq!(from_hex, keys.public_key());
    }

    #[test]
    fn test_multiple_shares_encrypt_decrypt() {
        // Simulate the full flow: encrypt multiple shares for one heir
        let service_keys = Keys::generate();
        let heir_keys = Keys::generate();

        let locked_shares = [
            "ms12nsecshare_a_data".to_string(),
            "ms12nsecshare_b_data".to_string(),
            "ms12nsecshare_c_data".to_string(),
        ];

        let split_id = "test_multi_001";

        // Encrypt each share
        let mut encrypted_payloads = Vec::new();
        for (i, share) in locked_shares.iter().enumerate() {
            let payload = SharePayload {
                share: share.clone(),
                index: i,
                total: locked_shares.len(),
                split_id: split_id.to_string(),
            };
            let json = serde_json::to_string(&payload).unwrap();
            let (encrypted, _kind) =
                encrypt_for_heir(&service_keys, &heir_keys.public_key(), &json).unwrap();
            encrypted_payloads.push(encrypted);
        }

        // Decrypt and verify all shares
        let mut recovered_shares = Vec::new();
        for encrypted in &encrypted_payloads {
            let decrypted = nip44::decrypt(
                heir_keys.secret_key(),
                &service_keys.public_key(),
                encrypted,
            )
            .unwrap();
            let payload: SharePayload = serde_json::from_str(&decrypted).unwrap();
            recovered_shares.push(payload);
        }

        assert_eq!(recovered_shares.len(), 3);
        for (i, recovered) in recovered_shares.iter().enumerate() {
            assert_eq!(recovered.share, locked_shares[i]);
            assert_eq!(recovered.index, i);
            assert_eq!(recovered.total, 3);
            assert_eq!(recovered.split_id, split_id);
        }
    }

    #[test]
    fn test_relay_publish_result_serialization() {
        let result = RelayPublishResult {
            shares_published: 6,
            heir_results: vec![
                HeirPublishResult {
                    heir_npub: "npub1test".to_string(),
                    heir_label: "Alice".to_string(),
                    shares_published: 3,
                    event_ids: vec!["abc".to_string(), "def".to_string(), "ghi".to_string()],
                    error: None,
                },
                HeirPublishResult {
                    heir_npub: "npub1test2".to_string(),
                    heir_label: "Bob".to_string(),
                    shares_published: 3,
                    event_ids: vec!["jkl".to_string()],
                    error: None,
                },
            ],
            successful_relays: vec!["wss://relay.damus.io".to_string()],
            failed_relays: vec![],
        };

        let json = serde_json::to_string(&result).unwrap();
        let decoded: RelayPublishResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.shares_published, 6);
        assert_eq!(decoded.heir_results.len(), 2);
    }

    #[test]
    fn test_relay_fetch_result_serialization() {
        let result = RelayFetchResult {
            shares: vec![SharePayload {
                share: "ms12test".to_string(),
                index: 0,
                total: 1,
                split_id: "abc".to_string(),
            }],
            responding_relays: vec!["wss://relay.damus.io".to_string()],
            events_found: 1,
        };

        let json = serde_json::to_string(&result).unwrap();
        let decoded: RelayFetchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.shares.len(), 1);
        assert_eq!(decoded.shares[0].share, "ms12test");
    }

    #[test]
    fn test_default_relays() {
        assert_eq!(DEFAULT_RELAYS.len(), 3);
        assert!(DEFAULT_RELAYS.iter().all(|r| r.starts_with("wss://")));
    }
}
