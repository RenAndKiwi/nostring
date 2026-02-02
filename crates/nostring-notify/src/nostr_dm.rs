//! Nostr DM (encrypted direct message) sending

use crate::config::NostrConfig;
use crate::templates::NotificationMessage;
use crate::NotifyError;
use nostr_sdk::prelude::*;
use std::time::Duration;

/// Send a Nostr DM notification
pub async fn send_dm(
    config: &NostrConfig,
    notification: &NotificationMessage,
) -> Result<(), NotifyError> {
    // Parse the recipient public key
    let recipient = parse_pubkey(&config.recipient_pubkey)
        .map_err(|e| NotifyError::NostrFailed(format!("Invalid recipient pubkey: {}", e)))?;

    // Get or generate the sender's keys
    let keys = if let Some(ref secret) = config.secret_key {
        Keys::parse(secret)
            .map_err(|e| NotifyError::NostrFailed(format!("Invalid secret key: {}", e)))?
    } else {
        return Err(NotifyError::NostrFailed(
            "No secret key provided for Nostr DM. Set nostr.secret_key in config.".into(),
        ));
    };

    // Create the Nostr client
    let client = Client::new(keys.clone());

    // Add relays
    for relay in &config.relays {
        if let Err(e) = client.add_relay(relay).await {
            log::warn!("Failed to add relay {}: {}", relay, e);
        }
    }

    // Connect to relays
    client.connect().await;

    // Wait for connections (with timeout)
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Format the message (subject + body for DMs)
    let dm_content = format!(
        "📢 {}\n\n{}",
        notification.subject, notification.body
    );

    // Build and send the encrypted DM event (NIP-04 style for compatibility)
    // This creates a kind:4 encrypted direct message
    use nostr_sdk::nostr::nips::nip04;
    let encrypted = nip04::encrypt(keys.secret_key(), &recipient, &dm_content)
        .map_err(|e| NotifyError::NostrFailed(format!("Encryption failed: {}", e)))?;
    
    let event = EventBuilder::new(Kind::EncryptedDirectMessage, encrypted)
        .tag(Tag::public_key(recipient))
        .sign_with_keys(&keys)
        .map_err(|e| NotifyError::NostrFailed(format!("Failed to build event: {}", e)))?;

    // Send to relays
    let output = client
        .send_event(event)
        .await
        .map_err(|e| NotifyError::NostrFailed(format!("Failed to send event: {}", e)))?;
    
    let event_id = output.id();

    log::info!(
        "Nostr DM sent to {} (event: {}, level: {:?})",
        config.recipient_pubkey,
        event_id,
        notification.level
    );

    // Disconnect
    client.disconnect().await;

    Ok(())
}

/// Parse a public key from npub or hex format
fn parse_pubkey(input: &str) -> Result<PublicKey, String> {
    // Try npub first
    if input.starts_with("npub") {
        return PublicKey::from_bech32(input).map_err(|e| e.to_string());
    }
    
    // Try hex
    PublicKey::from_hex(input).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pubkey_hex() {
        let hex = "7fa56f5d6962ab1e3cd424e758c3002b8665f7b0d8dcee9fe9e288d7751ac194";
        let result = parse_pubkey(hex);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_pubkey_npub() {
        // Valid npub (Jack Dorsey's public key)
        let npub = "npub1sg6plzptd64u62a878hep2kev88swjh3tw00gjsfl8f237lmu63q0uf63m";
        let result = parse_pubkey(npub);
        assert!(result.is_ok(), "Failed to parse npub: {:?}", result);
    }

    #[test]
    fn test_parse_pubkey_invalid() {
        let invalid = "not_a_valid_key";
        let result = parse_pubkey(invalid);
        assert!(result.is_err());
    }

    // Note: Actual Nostr tests require relay access
    // Use: cargo test --package nostring-notify -- --ignored
}
