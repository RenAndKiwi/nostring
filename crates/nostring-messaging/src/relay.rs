//! Nostr relay integration for MLS messaging.
//!
//! Bridges MDK's local MLS operations with the Nostr relay network:
//! - Publish key packages for discovery
//! - Gift-wrap welcome messages to invited members
//! - Send/receive encrypted group messages via relays

use std::time::Duration;

use nostr::nips::nip59;
use nostr::{Event, EventId, Filter, Kind, PublicKey, RelayUrl, Tag, Timestamp};
use nostr_sdk::Client;

use crate::groups::{GroupInfo, Message};
use crate::{GroupId, MessagingClient, MessagingError};

/// A relay-connected messaging client.
///
/// Wraps `MessagingClient` with the ability to publish and subscribe
/// to Nostr relays for actual message exchange.
pub struct RelayMessagingClient {
    inner: MessagingClient,
    client: Client,
}

impl RelayMessagingClient {
    /// Connect to relays with the given identity.
    pub async fn connect(
        keys: nostr::Keys,
        relay_urls: Vec<String>,
    ) -> Result<Self, MessagingError> {
        let client = Client::new(keys.clone());

        for url in &relay_urls {
            client
                .add_relay(url.as_str())
                .await
                .map_err(|e| MessagingError::Processing(format!("relay add failed: {e}")))?;
        }

        client.connect().await;

        Ok(Self {
            inner: MessagingClient::new(keys),
            client,
        })
    }

    /// Get a reference to the inner (local) messaging client.
    pub fn inner(&self) -> &MessagingClient {
        &self.inner
    }

    /// Get a reference to the nostr-sdk Client.
    pub fn nostr_client(&self) -> &Client {
        &self.client
    }

    /// Publish our MLS key package to relays for discovery.
    pub async fn publish_key_package(&self) -> Result<EventId, MessagingError> {
        let relay_urls: Vec<RelayUrl> = self.client.relays().await.keys().cloned().collect();

        let (encoded, tags) = self.inner.create_key_package(relay_urls)?;

        let event = nostr::event::builder::EventBuilder::new(Kind::MlsKeyPackage, encoded)
            .tags(tags)
            .sign(self.inner.keys())
            .await
            .map_err(|e| MessagingError::Processing(format!("sign failed: {e}")))?;

        let output = self
            .client
            .send_event(&event)
            .await
            .map_err(|e| MessagingError::Processing(format!("publish failed: {e}")))?;

        Ok(*output.id())
    }

    /// Fetch a user's MLS key package from relays.
    pub async fn fetch_key_package(&self, pubkey: &PublicKey) -> Result<Event, MessagingError> {
        let filter = Filter::new()
            .author(*pubkey)
            .kind(Kind::MlsKeyPackage)
            .limit(1);

        let events = self
            .client
            .fetch_events(filter, Duration::from_secs(10))
            .await
            .map_err(|e| MessagingError::Processing(format!("fetch failed: {e}")))?;

        events
            .into_iter()
            .next()
            .ok_or_else(|| MessagingError::Processing(format!("no key package found for {pubkey}")))
    }

    /// Create a group and send gift-wrapped welcome messages to all members.
    pub async fn create_and_invite(
        &self,
        name: &str,
        description: &str,
        member_pubkeys: Vec<PublicKey>,
    ) -> Result<GroupInfo, MessagingError> {
        let mut key_package_events = Vec::new();
        for pk in &member_pubkeys {
            let kp_event = self.fetch_key_package(pk).await?;
            key_package_events.push(kp_event);
        }

        let relay_urls: Vec<RelayUrl> = self.client.relays().await.keys().cloned().collect();

        let result = self.inner.create_group(
            name,
            description,
            relay_urls,
            member_pubkeys.clone(),
            key_package_events,
        )?;

        // Gift-wrap and send welcome messages to each member
        for (i, welcome_rumor) in result.welcome_rumors.iter().enumerate() {
            let recipient = &member_pubkeys[i];
            self.client
                .gift_wrap(recipient, welcome_rumor.clone(), Vec::<Tag>::new())
                .await
                .map_err(|e| {
                    MessagingError::Processing(format!("gift-wrap to {recipient} failed: {e}"))
                })?;
        }

        Ok(result.group)
    }

    /// Send a text message to a group (publishes to relays).
    pub async fn send(&self, group_id: &GroupId, content: &str) -> Result<EventId, MessagingError> {
        let msg_result = self.inner.send_message(group_id, content)?;

        let output = self
            .client
            .send_event(&msg_result.event)
            .await
            .map_err(|e| MessagingError::Processing(format!("send failed: {e}")))?;

        Ok(*output.id())
    }

    /// Fetch and process new group messages from relays.
    pub async fn sync(
        &self,
        since: Option<Timestamp>,
    ) -> Result<Vec<(GroupId, Vec<Message>)>, MessagingError> {
        let groups = self.inner.get_groups()?;
        if groups.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_messages = Vec::new();

        for group in &groups {
            let group_id_hex = hex::encode(group.nostr_group_id);

            let mut filter = Filter::new().custom_tag(
                nostr::SingleLetterTag::lowercase(nostr::Alphabet::H),
                group_id_hex,
            );

            if let Some(ts) = since {
                filter = filter.since(ts);
            }

            let events = self
                .client
                .fetch_events(filter, Duration::from_secs(10))
                .await
                .map_err(|e| MessagingError::Processing(format!("sync failed: {e}")))?;

            for event in events {
                if event.pubkey == self.inner.public_key() {
                    continue;
                }
                let _ = self.inner.process_message(&event);
            }

            let messages = self.inner.get_messages(&group.mls_group_id)?;
            if !messages.is_empty() {
                all_messages.push((group.mls_group_id.clone(), messages));
            }
        }

        Ok(all_messages)
    }

    /// Fetch and process gift-wrapped welcome messages.
    pub async fn check_welcomes(
        &self,
        since: Option<Timestamp>,
    ) -> Result<Vec<GroupInfo>, MessagingError> {
        let mut filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(self.inner.public_key());

        if let Some(ts) = since {
            filter = filter.since(ts);
        }

        let events = self
            .client
            .fetch_events(filter, Duration::from_secs(10))
            .await
            .map_err(|e| MessagingError::Processing(format!("welcome fetch failed: {e}")))?;

        let mut new_groups = Vec::new();

        for gift_wrap in events {
            // Unwrap gift wrap — may fail if not addressed to us or corrupted
            let unwrapped =
                nip59::UnwrappedGift::from_gift_wrap(self.inner.keys(), &gift_wrap).await;

            match unwrapped {
                Ok(unwrapped) => {
                    let rumor = unwrapped.rumor;
                    match self.inner.process_welcome(&gift_wrap.id, &rumor) {
                        Ok(()) => {
                            if let Ok(group) = self.inner.accept_first_welcome() {
                                new_groups.push(group);
                            }
                        }
                        Err(_) => {
                            // Not an MLS welcome — could be a different gift-wrapped event.
                            // This is expected and not an error.
                        }
                    }
                }
                Err(_) => {
                    // Gift wrap not addressed to us or corrupted — skip silently.
                    // This is normal when filtering by Kind::GiftWrap broadly.
                }
            }
        }

        Ok(new_groups)
    }

    /// Disconnect from all relays.
    pub async fn disconnect(&self) {
        self.client.disconnect().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::Keys;

    #[test]
    fn test_relay_client_types() {
        // Verify the types compile and are constructable
        let _keys = Keys::generate();
    }
}
