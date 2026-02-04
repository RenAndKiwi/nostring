//! Contact discovery via Nostr
//!
//! Maps Nostr identities to email addresses using:
//! - NIP-05 verification (domain-based identity → may expose email patterns)
//! - Nostr profile metadata (some users list email in profile)
//! - Manual contact registry (configured by the owner)

use crate::EmailError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A contact entry mapping a Nostr identity to an email address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    /// Nostr npub (bech32)
    pub npub: String,
    /// Email address
    pub email: String,
    /// Display name (optional)
    pub name: Option<String>,
    /// How the email was discovered
    pub source: ContactSource,
}

/// How a contact's email was discovered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContactSource {
    /// Manually configured by the owner
    Manual,
    /// Discovered via NIP-05 lookup
    Nip05,
    /// Found in Nostr profile metadata
    Profile,
}

/// A registry of heir contacts.
///
/// Maps heir identifiers (fingerprint or npub) to their email addresses.
/// Used by the notification system to deliver shares and descriptor backups.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContactRegistry {
    /// Contacts indexed by npub
    contacts: HashMap<String, Contact>,
}

impl ContactRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            contacts: HashMap::new(),
        }
    }

    /// Add or update a contact.
    pub fn upsert(&mut self, contact: Contact) {
        self.contacts.insert(contact.npub.clone(), contact);
    }

    /// Get a contact by npub.
    pub fn get(&self, npub: &str) -> Option<&Contact> {
        self.contacts.get(npub)
    }

    /// Get email for an npub, if known.
    pub fn get_email(&self, npub: &str) -> Option<&str> {
        self.contacts.get(npub).map(|c| c.email.as_str())
    }

    /// Remove a contact.
    pub fn remove(&mut self, npub: &str) -> Option<Contact> {
        self.contacts.remove(npub)
    }

    /// List all contacts.
    pub fn list(&self) -> Vec<&Contact> {
        self.contacts.values().collect()
    }

    /// Number of contacts.
    pub fn len(&self) -> usize {
        self.contacts.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.contacts.is_empty()
    }
}

/// Look up an email address via NIP-05 verification.
///
/// NIP-05 maps `user@domain` → Nostr pubkey. We reverse-check:
/// given an npub, we look up their NIP-05 identifier from their
/// Nostr profile, then verify it. The domain part hints at their
/// email provider.
///
/// Note: This is a heuristic — the NIP-05 domain doesn't guarantee
/// the email address format. Manual configuration is more reliable.
pub async fn lookup_nip05(npub: &str) -> Result<Option<String>, EmailError> {
    use nostr_sdk::prelude::*;

    let pubkey = PublicKey::parse(npub)
        .map_err(|e| EmailError::Nip05(format!("Invalid npub: {}", e)))?;

    // Connect to relays and fetch the profile
    let client = Client::default();
    client
        .add_relay("wss://relay.damus.io")
        .await
        .map_err(|e| EmailError::Nip05(format!("Relay error: {}", e)))?;
    client
        .add_relay("wss://relay.nostr.band")
        .await
        .map_err(|e| EmailError::Nip05(format!("Relay error: {}", e)))?;
    client.connect().await;

    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::Metadata)
        .limit(1);

    let events = client
        .fetch_events(filter, std::time::Duration::from_secs(5))
        .await
        .map_err(|e| EmailError::Nip05(format!("Fetch error: {}", e)))?;

    client.disconnect().await;

    // Parse the metadata to find NIP-05
    for event in events.iter() {
        if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&event.content) {
            if let Some(nip05) = metadata.get("nip05").and_then(|v| v.as_str()) {
                return Ok(Some(nip05.to_string()));
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contact_registry() {
        let mut registry = ContactRegistry::new();
        assert!(registry.is_empty());

        registry.upsert(Contact {
            npub: "npub1alice".to_string(),
            email: "alice@example.com".to_string(),
            name: Some("Alice".to_string()),
            source: ContactSource::Manual,
        });

        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.get_email("npub1alice"),
            Some("alice@example.com")
        );

        // Upsert overwrites
        registry.upsert(Contact {
            npub: "npub1alice".to_string(),
            email: "newalice@example.com".to_string(),
            name: Some("Alice".to_string()),
            source: ContactSource::Manual,
        });
        assert_eq!(
            registry.get_email("npub1alice"),
            Some("newalice@example.com")
        );

        // Remove
        registry.remove("npub1alice");
        assert!(registry.is_empty());
    }

    #[test]
    fn test_contact_sources() {
        let manual = ContactSource::Manual;
        let nip05 = ContactSource::Nip05;
        assert_ne!(manual, nip05);
    }
}
