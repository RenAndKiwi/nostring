//! Group management — create, join, send messages to MLS groups.

use mdk_core::prelude::*;
use mdk_storage_traits::groups::types::Group as MdkGroup;
use mdk_storage_traits::messages::types::Message as MdkMessage;
use mdk_storage_traits::test_utils::crypto_utils::generate_random_bytes;
use nostr::event::builder::EventBuilder;
#[cfg(test)]
use nostr::Keys;
use nostr::{Event, EventId, Kind, PublicKey, RelayUrl, UnsignedEvent};

use crate::{GroupId, MessagingClient, MessagingError};

/// Information about an MLS group.
#[derive(Clone, Debug)]
pub struct GroupInfo {
    pub mls_group_id: GroupId,
    pub nostr_group_id: [u8; 32],
    pub name: String,
    pub description: String,
}

impl From<MdkGroup> for GroupInfo {
    fn from(g: MdkGroup) -> Self {
        Self {
            mls_group_id: g.mls_group_id,
            nostr_group_id: g.nostr_group_id,
            name: g.name,
            description: g.description,
        }
    }
}

/// A decrypted message from a group.
#[derive(Clone, Debug)]
pub struct Message {
    pub sender: PublicKey,
    pub content: String,
    pub kind: Kind,
    pub created_at: nostr::Timestamp,
}

impl From<MdkMessage> for Message {
    fn from(m: MdkMessage) -> Self {
        Self {
            sender: m.pubkey,
            content: m.content,
            kind: m.kind,
            created_at: m.created_at,
        }
    }
}

/// Result of creating a group.
pub struct GroupCreateResult {
    pub group: GroupInfo,
    /// Welcome rumors to gift-wrap (NIP-59) and send to each invited member.
    pub welcome_rumors: Vec<UnsignedEvent>,
}

/// Result of sending a message.
pub struct MessageSendResult {
    /// The encrypted MLS message event to publish to relays.
    pub event: Event,
}

impl MessagingClient {
    /// Create a new MLS group and invite members.
    ///
    /// `member_key_package_events` are Kind::MlsKeyPackage events fetched from relays.
    pub fn create_group(
        &self,
        name: &str,
        description: &str,
        relay_urls: Vec<RelayUrl>,
        member_pubkeys: Vec<PublicKey>,
        member_key_package_events: Vec<Event>,
    ) -> Result<GroupCreateResult, MessagingError> {
        let image_hash: [u8; 32] = generate_random_bytes(32).try_into().unwrap();
        let image_key: [u8; 32] = generate_random_bytes(32).try_into().unwrap();
        let image_nonce: [u8; 12] = generate_random_bytes(12).try_into().unwrap();

        let mut all_members = vec![self.keys.public_key()];
        all_members.extend(member_pubkeys);

        let config = NostrGroupConfigData::new(
            name.to_string(),
            description.to_string(),
            Some(image_hash),
            Some(image_key),
            Some(image_nonce),
            relay_urls,
            all_members,
        );

        let result =
            self.mdk
                .create_group(&self.keys.public_key(), member_key_package_events, config)?;

        Ok(GroupCreateResult {
            group: GroupInfo::from(result.group),
            welcome_rumors: result.welcome_rumors,
        })
    }

    /// Process a welcome rumor received via gift-wrap.
    pub fn process_welcome(
        &self,
        gift_wrap_event_id: &EventId,
        welcome_rumor: &UnsignedEvent,
    ) -> Result<(), MessagingError> {
        self.mdk
            .process_welcome(gift_wrap_event_id, welcome_rumor)?;
        Ok(())
    }

    /// Accept the first pending welcome and join the group.
    pub fn accept_first_welcome(&self) -> Result<GroupInfo, MessagingError> {
        let welcomes = self
            .mdk
            .get_pending_welcomes(None)
            .map_err(|e| MessagingError::Processing(e.to_string()))?;

        let welcome = welcomes
            .first()
            .ok_or_else(|| MessagingError::GroupNotFound("no pending welcomes".into()))?;

        self.mdk.accept_welcome(welcome)?;

        let groups = self.mdk.get_groups()?;
        let group = groups
            .first()
            .ok_or_else(|| MessagingError::GroupNotFound("group not found after accept".into()))?;

        Ok(GroupInfo::from(group.clone()))
    }

    /// Send a text message to a group.
    pub fn send_message(
        &self,
        group_id: &GroupId,
        content: &str,
    ) -> Result<MessageSendResult, MessagingError> {
        let rumor = EventBuilder::new(Kind::Custom(9), content).build(self.keys.public_key());
        let event = self.mdk.create_message(group_id, rumor)?;
        Ok(MessageSendResult { event })
    }

    /// Process a received MLS message event from relays.
    pub fn process_message(&self, event: &Event) -> Result<(), MessagingError> {
        self.mdk.process_message(event)?;
        Ok(())
    }

    /// Merge a pending commit (after adding/removing members).
    pub fn merge_pending_commit(&self, group_id: &GroupId) -> Result<(), MessagingError> {
        self.mdk.merge_pending_commit(group_id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_client() -> MessagingClient {
        MessagingClient::new(Keys::generate())
    }

    #[tokio::test]
    async fn test_create_key_package() {
        let client = create_test_client();
        let relay = RelayUrl::parse("ws://localhost:8080").unwrap();
        let (encoded, tags) = client.create_key_package(vec![relay]).unwrap();
        assert!(!encoded.is_empty());
        assert!(!tags.is_empty());
    }

    #[tokio::test]
    async fn test_group_lifecycle() {
        let alice = create_test_client();
        let bob = create_test_client();
        let relay = RelayUrl::parse("ws://localhost:8080").unwrap();

        // Bob creates a key package
        let (bob_kp_encoded, bob_tags) = bob.create_key_package(vec![relay.clone()]).unwrap();
        let bob_kp_event = EventBuilder::new(Kind::MlsKeyPackage, bob_kp_encoded)
            .tags(bob_tags)
            .build(bob.public_key())
            .sign(bob.keys())
            .await
            .unwrap();

        // Alice creates a group with Bob
        let result = alice
            .create_group(
                "test-group",
                "A test group",
                vec![relay],
                vec![bob.public_key()],
                vec![bob_kp_event],
            )
            .unwrap();

        assert_eq!(result.group.name, "test-group");
        assert_eq!(result.welcome_rumors.len(), 1);

        // Alice sends a message
        let msg_result = alice
            .send_message(&result.group.mls_group_id, "Hello Bob!")
            .unwrap();

        // Bob processes the welcome
        bob.process_welcome(&EventId::all_zeros(), &result.welcome_rumors[0])
            .unwrap();
        let bob_group = bob.accept_first_welcome().unwrap();

        // Bob processes the message
        bob.process_message(&msg_result.event).unwrap();
        let messages = bob.get_messages(&bob_group.mls_group_id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello Bob!");
        assert_eq!(messages[0].sender, alice.public_key());

        // Verify members
        let members = alice.get_members(&result.group.mls_group_id).unwrap();
        assert_eq!(members.len(), 2);
    }

    #[test]
    fn test_empty_groups() {
        let client = create_test_client();
        assert!(client.get_groups().unwrap().is_empty());
    }
}
