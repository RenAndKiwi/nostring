//! Nostr DM (encrypted direct message) sending via NIP-17.
//!
//! Uses NIP-17 gift-wrapped private messages (sealed sender) instead of
//! the deprecated NIP-04. NIP-17 wraps the message in NIP-59 gift wrap,
//! so relays cannot see the recipient.

use crate::config::NostrConfig;
use crate::templates::NotificationMessage;
use crate::NotifyError;
use nostr_sdk::prelude::*;
use std::time::Duration;

/// Send a Nostr DM notification using NIP-17 (gift-wrapped private message).
pub async fn send_dm(
    config: &NostrConfig,
    notification: &NotificationMessage,
) -> Result<EventId, NotifyError> {
    let recipient = parse_pubkey(&config.recipient_pubkey)
        .map_err(|e| NotifyError::NostrFailed(format!("Invalid recipient pubkey: {}", e)))?;

    let keys = if let Some(ref secret) = config.secret_key {
        Keys::parse(secret)
            .map_err(|e| NotifyError::NostrFailed(format!("Invalid secret key: {}", e)))?
    } else {
        return Err(NotifyError::NostrFailed(
            "No secret key provided for Nostr DM. Set nostr.secret_key in config.".into(),
        ));
    };

    let client = Client::new(keys);

    for relay in &config.relays {
        if let Err(e) = client.add_relay(relay).await {
            log::warn!("Failed to add relay {}: {}", relay, e);
        }
    }

    client.connect().await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    let dm_content = format!("📢 {}\n\n{}", notification.subject, notification.body);

    // NIP-17: send_private_msg uses NIP-59 gift wrapping (sealed sender).
    // The relay sees a random pubkey, not the actual recipient.
    let output = client
        .send_private_msg(recipient, &dm_content, [])
        .await
        .map_err(|e| NotifyError::NostrFailed(format!("Failed to send NIP-17 DM: {}", e)))?;

    let event_id = output.id();

    log::info!(
        "NIP-17 DM sent to {} (event: {}, level: {:?})",
        config.recipient_pubkey,
        event_id,
        notification.level
    );

    client.disconnect().await;

    Ok(*event_id)
}

/// Send a Nostr DM to an arbitrary recipient using the provided sender keys.
///
/// Unlike `send_dm`, this doesn't require a full `NostrConfig` — just the
/// sender secret key, recipient npub, and relay list. Used for heir notification.
pub async fn send_dm_to_recipient(
    sender_secret: &str,
    recipient_npub: &str,
    relays: &[String],
    notification: &NotificationMessage,
) -> Result<EventId, NotifyError> {
    let recipient = parse_pubkey(recipient_npub)
        .map_err(|e| NotifyError::NostrFailed(format!("Invalid recipient pubkey: {}", e)))?;

    let keys = Keys::parse(sender_secret)
        .map_err(|e| NotifyError::NostrFailed(format!("Invalid secret key: {}", e)))?;

    let client = Client::new(keys);

    for relay in relays {
        if let Err(e) = client.add_relay(relay).await {
            log::warn!("Failed to add relay {}: {}", relay, e);
        }
    }

    client.connect().await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    let dm_content = format!("📢 {}\n\n{}", notification.subject, notification.body);

    let output = client
        .send_private_msg(recipient, &dm_content, [])
        .await
        .map_err(|e| NotifyError::NostrFailed(format!("Failed to send NIP-17 DM: {}", e)))?;

    let event_id = output.id();

    log::info!("NIP-17 DM sent to {} (event: {})", recipient_npub, event_id,);

    client.disconnect().await;
    Ok(*event_id)
}

/// Deliver a vault backup to an heir via NIP-17 gift-wrapped DM.
///
/// The vault backup JSON is embedded in a human-readable message instructing
/// the heir to store it safely. NIP-17 provides sealed-sender privacy.
///
/// # Arguments
/// * `sender_secret` - Owner's Nostr secret key (nsec or hex)
/// * `recipient_npub` - Heir's Nostr public key (npub or hex)
/// * `relays` - Nostr relays to publish through
/// * `vault_backup_json` - Serialized VaultBackup JSON string
pub async fn deliver_vault_backup(
    sender_secret: &str,
    recipient_npub: &str,
    relays: &[String],
    vault_backup_json: &str,
) -> Result<EventId, NotifyError> {
    let message = format_vault_backup_message(vault_backup_json);

    let notification = NotificationMessage {
        subject: "Vault Backup".to_string(),
        body: message,
        level: crate::NotificationLevel::Critical,
    };

    send_dm_to_recipient(sender_secret, recipient_npub, relays, &notification).await
}

/// Format the vault backup into a human-readable message.
fn format_vault_backup_message(vault_backup_json: &str) -> String {
    format!(
        "You are listed as an heir in a Bitcoin inheritance vault.\n\
         Store this message safely. You will need it to claim funds.\n\
         \n\
         ---\n\
         {}\n\
         ---\n\
         \n\
         To claim: import this backup into the NoString app when the time comes.",
        vault_backup_json
    )
}

/// Parse a public key from npub or hex format.
fn parse_pubkey(input: &str) -> Result<PublicKey, String> {
    if input.starts_with("npub") {
        return PublicKey::from_bech32(input).map_err(|e| e.to_string());
    }
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

    #[test]
    fn test_format_vault_backup_message() {
        let json = r#"{"version":1,"network":"testnet"}"#;
        let msg = format_vault_backup_message(json);

        assert!(msg.contains("heir in a Bitcoin inheritance vault"));
        assert!(msg.contains(json), "message must contain the full JSON");
        assert!(msg.contains("NoString app"));
        assert!(msg.contains("---"));
    }

    #[test]
    fn test_format_vault_backup_preserves_json() {
        // Ensure complex JSON with special chars survives formatting
        let json = r#"{"version":1,"heirs":[{"npub":"npub1abc","xpub":"xpub6D..."}],"vault_address":"tb1p..."}"#;
        let msg = format_vault_backup_message(json);

        // Extract JSON between the --- delimiters
        let parts: Vec<&str> = msg.split("---").collect();
        assert_eq!(
            parts.len(),
            3,
            "should have text before, JSON between, and text after ---"
        );
        let extracted = parts[1].trim();
        assert_eq!(
            extracted, json,
            "JSON must survive round-trip through formatting"
        );
    }
}
