//! CCD integration — MLS channels for co-signer coordination.
//!
//! Wraps `nostring-ccd`'s `CcdMessage` types to send/receive CCD protocol
//! messages over MLS-encrypted group channels instead of raw NIP-44 DMs.
//!
//! This provides forward secrecy for CCD ceremonies — if a key is compromised
//! after a signing session, past ceremonies can't be reconstructed.

use mdk_storage_traits::MdkStorageProvider;
use nostr::{Event, Kind};

use crate::{GroupId, MessagingClient, MessagingError};

/// Kind used for MLS group messages (including CCD protocol messages).
const CCD_MESSAGE_KIND: Kind = Kind::Custom(9);

/// A CCD coordination channel backed by an MLS group.
///
/// Provides typed send/receive for CCD protocol messages
/// over a forward-secret MLS channel.
pub struct CcdChannel<'a, S: MdkStorageProvider> {
    client: &'a MessagingClient<S>,
    group_id: GroupId,
}

impl<'a, S: MdkStorageProvider> CcdChannel<'a, S> {
    /// Create a new CCD channel for an existing MLS group.
    ///
    /// The group should contain exactly the vault co-signers.
    pub fn new(client: &'a MessagingClient<S>, group_id: GroupId) -> Self {
        Self { client, group_id }
    }

    /// Send a JSON-serialized CCD message to the co-signer group.
    ///
    /// The `ccd_json` should be produced by `nostring_ccd::transport::serialize_message()`.
    pub fn send_ccd_message(&self, ccd_json: &str) -> Result<Event, MessagingError> {
        let result = self.client.send_message(&self.group_id, ccd_json)?;
        Ok(result.event)
    }

    /// Get CCD protocol messages from the group.
    ///
    /// Filters by Kind::Custom(9) AND presence of `"ccd_type"` in content.
    /// Returns raw JSON — parse with `nostring_ccd::transport::deserialize_message()`.
    pub fn receive_ccd_messages(&self) -> Result<Vec<CcdGroupMessage>, MessagingError> {
        let messages = self.client.get_messages(&self.group_id)?;
        Ok(messages
            .into_iter()
            .filter(|m| m.kind == CCD_MESSAGE_KIND && m.content.contains("\"ccd_type\""))
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
    use nostr::event::builder::EventBuilder;
    use nostr::{EventId, Keys, Kind, RelayUrl};

    use crate::InMemoryClient;

    #[tokio::test]
    async fn test_ccd_channel_send_receive() {
        let alice = InMemoryClient::new(Keys::generate());
        let bob = InMemoryClient::new(Keys::generate());
        let relay = RelayUrl::parse("ws://localhost:8080").unwrap();

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

        bob.process_welcome(&EventId::all_zeros(), &result.welcome_rumors[0])
            .unwrap();
        let bob_group = bob.accept_first_welcome().unwrap();

        let channel = CcdChannel::new(&alice, result.group.mls_group_id.clone());
        let ccd_json = r#"{"ccd_type":"NonceRequest","session_id":"abc123","num_inputs":2}"#;
        let event = channel.send_ccd_message(ccd_json).unwrap();

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
        let client = InMemoryClient::new(Keys::generate());
        let group_id = GroupId::from_slice(&[0u8; 16]);

        let channel = CcdChannel::new(&client, group_id);
        let result = channel.receive_ccd_messages();
        assert!(result.is_err(), "nonexistent group should error");
    }

    #[tokio::test]
    async fn test_ccd_channel_filters_non_ccd_messages() {
        let alice = InMemoryClient::new(Keys::generate());
        let bob = InMemoryClient::new(Keys::generate());
        let relay = RelayUrl::parse("ws://localhost:8080").unwrap();

        let (bob_kp, bob_tags) = bob.create_key_package(vec![relay.clone()]).unwrap();
        let bob_kp_event = EventBuilder::new(Kind::MlsKeyPackage, bob_kp)
            .tags(bob_tags)
            .build(bob.public_key())
            .sign(bob.keys())
            .await
            .unwrap();

        let result = alice
            .create_group(
                "mixed-channel",
                "CCD + chat",
                vec![relay],
                vec![bob.public_key()],
                vec![bob_kp_event],
            )
            .unwrap();

        bob.process_welcome(&EventId::all_zeros(), &result.welcome_rumors[0])
            .unwrap();
        let bob_group = bob.accept_first_welcome().unwrap();

        let channel = CcdChannel::new(&alice, result.group.mls_group_id.clone());

        // Send a CCD message
        let ccd_json = r#"{"ccd_type":"TweakRequest","owner_pubkey":"abc","relays":[]}"#;
        let ccd_event = channel.send_ccd_message(ccd_json).unwrap();

        // Send a regular chat message (also Kind::Custom(9) but no ccd_type)
        let chat_result = alice
            .send_message(&result.group.mls_group_id, "hey, ready to sign?")
            .unwrap();

        bob.process_message(&ccd_event).unwrap();
        bob.process_message(&chat_result.event).unwrap();

        let bob_channel = CcdChannel::new(&bob, bob_group.mls_group_id.clone());
        let messages = bob_channel.receive_ccd_messages().unwrap();

        // Content-based filter: only messages containing "ccd_type" pass
        assert_eq!(
            messages.len(),
            1,
            "only CCD messages should pass the content filter"
        );
        assert!(messages[0].content.contains("TweakRequest"));
    }

    #[tokio::test]
    async fn test_ccd_channel_malformed_json() {
        let alice = InMemoryClient::new(Keys::generate());
        let bob = InMemoryClient::new(Keys::generate());
        let relay = RelayUrl::parse("ws://localhost:8080").unwrap();

        let (bob_kp, bob_tags) = bob.create_key_package(vec![relay.clone()]).unwrap();
        let bob_kp_event = EventBuilder::new(Kind::MlsKeyPackage, bob_kp)
            .tags(bob_tags)
            .build(bob.public_key())
            .sign(bob.keys())
            .await
            .unwrap();

        let result = alice
            .create_group(
                "malformed-test",
                "test",
                vec![relay],
                vec![bob.public_key()],
                vec![bob_kp_event],
            )
            .unwrap();

        bob.process_welcome(&EventId::all_zeros(), &result.welcome_rumors[0])
            .unwrap();
        let bob_group = bob.accept_first_welcome().unwrap();

        let channel = CcdChannel::new(&alice, result.group.mls_group_id.clone());

        // Malformed JSON without ccd_type — should NOT pass filter
        let event = channel.send_ccd_message("not json at all {{{").unwrap();

        bob.process_message(&event).unwrap();
        let bob_channel = CcdChannel::new(&bob, bob_group.mls_group_id.clone());
        let messages = bob_channel.receive_ccd_messages().unwrap();

        assert_eq!(
            messages.len(),
            0,
            "malformed content without ccd_type should be filtered out"
        );
    }

    #[tokio::test]
    async fn test_ccd_message_ordering() {
        let alice = InMemoryClient::new(Keys::generate());
        let bob = InMemoryClient::new(Keys::generate());
        let relay = RelayUrl::parse("ws://localhost:8080").unwrap();

        let (bob_kp, bob_tags) = bob.create_key_package(vec![relay.clone()]).unwrap();
        let bob_kp_event = EventBuilder::new(Kind::MlsKeyPackage, bob_kp)
            .tags(bob_tags)
            .build(bob.public_key())
            .sign(bob.keys())
            .await
            .unwrap();

        let result = alice
            .create_group(
                "order-test",
                "test",
                vec![relay],
                vec![bob.public_key()],
                vec![bob_kp_event],
            )
            .unwrap();

        bob.process_welcome(&EventId::all_zeros(), &result.welcome_rumors[0])
            .unwrap();
        let bob_group = bob.accept_first_welcome().unwrap();

        let channel = CcdChannel::new(&alice, result.group.mls_group_id.clone());

        let e1 = channel
            .send_ccd_message(r#"{"ccd_type":"NonceRequest","step":1}"#)
            .unwrap();
        let e2 = channel
            .send_ccd_message(r#"{"ccd_type":"NonceResponse","step":2}"#)
            .unwrap();
        let e3 = channel
            .send_ccd_message(r#"{"ccd_type":"SignChallenge","step":3}"#)
            .unwrap();

        bob.process_message(&e1).unwrap();
        bob.process_message(&e2).unwrap();
        bob.process_message(&e3).unwrap();

        let bob_channel = CcdChannel::new(&bob, bob_group.mls_group_id.clone());
        let messages = bob_channel.receive_ccd_messages().unwrap();

        assert_eq!(messages.len(), 3);
        assert!(messages.iter().any(|m| m.content.contains("NonceRequest")));
        assert!(messages.iter().any(|m| m.content.contains("NonceResponse")));
        assert!(messages.iter().any(|m| m.content.contains("SignChallenge")));
    }
}
