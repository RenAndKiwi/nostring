//! CCD integration — MLS channels for co-signer coordination.
//!
//! Wraps `nostring-ccd`'s `CcdMessage` types to send/receive CCD protocol
//! messages over MLS-encrypted group channels instead of raw NIP-44 DMs.
//!
//! This provides forward secrecy for CCD ceremonies — if a key is compromised
//! after a signing session, past ceremonies can't be reconstructed.
//!
//! # Usage
//!
//! ```ignore
//! use nostring_messaging::ccd::CcdChannel;
//!
//! // Create an MLS group for vault co-signers
//! let channel = CcdChannel::new(&messaging_client, &group_id);
//!
//! // Send a CCD message (tweak request, nonce exchange, etc.)
//! let event = channel.send_ccd_message(&msg)?;
//!
//! // Receive and parse CCD messages from group
//! let ccd_messages = channel.receive_ccd_messages()?;
//! ```

use nostr::{Event, Kind};

use crate::{GroupId, MessagingClient, MessagingError};

/// CCD message types that can be sent over MLS channels.
///
/// These mirror `nostring_ccd::transport::CcdMessage` but are serialized
/// as MLS group message content rather than NIP-44 DMs.
///
/// The content field is the JSON-serialized CcdMessage.
const CCD_MESSAGE_KIND: Kind = Kind::Custom(9);

/// A CCD coordination channel backed by an MLS group.
///
/// Provides typed send/receive for CCD protocol messages
/// over a forward-secret MLS channel.
pub struct CcdChannel<'a> {
    client: &'a MessagingClient,
    group_id: GroupId,
}

impl<'a> CcdChannel<'a> {
    /// Create a new CCD channel for an existing MLS group.
    ///
    /// The group should contain exactly the vault co-signers.
    pub fn new(client: &'a MessagingClient, group_id: GroupId) -> Self {
        Self { client, group_id }
    }

    /// Send a JSON-serialized CCD message to the co-signer group.
    ///
    /// The `ccd_json` should be produced by `nostring_ccd::transport::serialize_message()`.
    /// It's sent as the content of a Kind::Custom(9) MLS group message.
    pub fn send_ccd_message(&self, ccd_json: &str) -> Result<Event, MessagingError> {
        let result = self.client.send_message(&self.group_id, ccd_json)?;
        Ok(result.event)
    }

    /// Get all CCD messages from the group.
    ///
    /// Returns the raw JSON content of each message. Use
    /// `nostring_ccd::transport::deserialize_message()` to parse.
    pub fn receive_ccd_messages(&self) -> Result<Vec<CcdGroupMessage>, MessagingError> {
        let messages = self.client.get_messages(&self.group_id)?;
        Ok(messages
            .into_iter()
            .filter(|m| m.kind == CCD_MESSAGE_KIND)
            .map(|m| CcdGroupMessage {
                sender: m.sender,
                content: m.content,
                timestamp: m.created_at,
            })
            .collect())
    }

    /// Get the group ID for this channel.
    pub fn group_id(&self) -> &GroupId {
        &self.group_id
    }
}

/// A CCD protocol message received from an MLS group.
#[derive(Clone, Debug)]
pub struct CcdGroupMessage {
    /// Nostr public key of the sender (co-signer identity).
    pub sender: nostr::PublicKey,
    /// JSON-serialized CCD message content.
    /// Parse with `nostring_ccd::transport::deserialize_message()`.
    pub content: String,
    /// When the message was created.
    pub timestamp: nostr::Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdk_core::prelude::*;
    use mdk_storage_traits::test_utils::crypto_utils::generate_random_bytes;
    use nostr::event::builder::EventBuilder;
    use nostr::{EventId, Keys, RelayUrl};

    #[tokio::test]
    async fn test_ccd_channel_send_receive() {
        let alice = MessagingClient::new(Keys::generate());
        let bob = MessagingClient::new(Keys::generate());
        let relay = RelayUrl::parse("ws://localhost:8080").unwrap();

        // Set up group (same as group lifecycle test)
        let (bob_kp_encoded, bob_tags) = bob.create_key_package(vec![relay.clone()]).unwrap();
        let bob_kp_event = EventBuilder::new(Kind::MlsKeyPackage, bob_kp_encoded)
            .tags(bob_tags)
            .build(bob.public_key())
            .sign(bob.keys())
            .await
            .unwrap();

        let result = alice
            .create_group(
                "vault-cosigners",
                "CCD coordination",
                vec![relay],
                vec![bob.public_key()],
                vec![bob_kp_event],
            )
            .unwrap();

        // Bob joins
        bob.process_welcome(&EventId::all_zeros(), &result.welcome_rumors[0])
            .unwrap();
        let bob_group = bob.accept_first_welcome().unwrap();

        // Alice sends a CCD message via the channel
        let channel = CcdChannel::new(&alice, result.group.mls_group_id.clone());
        let ccd_json = r#"{"ccd_type":"NonceRequest","session_id":"abc123","num_inputs":2}"#;
        let event = channel.send_ccd_message(ccd_json).unwrap();

        // Bob receives and processes the message
        bob.process_message(&event).unwrap();
        let bob_channel = CcdChannel::new(&bob, bob_group.mls_group_id.clone());
        let messages = bob_channel.receive_ccd_messages().unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, alice.public_key());
        assert!(messages[0].content.contains("NonceRequest"));
        assert!(messages[0].content.contains("abc123"));
    }

    #[test]
    fn test_ccd_channel_empty() {
        let client = MessagingClient::new(Keys::generate());
        let group_id = GroupId::from_slice(&[0u8; 16]);

        // This will fail because the group doesn't exist in MDK,
        // but it verifies the types compile correctly.
        let channel = CcdChannel::new(&client, group_id);
        // get_messages on nonexistent group returns error, which is expected
        let _ = channel.receive_ccd_messages();
    }
}
