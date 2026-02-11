//! Persistent messaging client backed by encrypted SQLite (SQLCipher).
//!
//! MLS state (groups, messages, key packages) is stored on disk via
//! `mdk-sqlite-storage`. Encryption key is managed by the platform keyring.

use std::path::Path;

use mdk_core::prelude::*;
use mdk_sqlite_storage::MdkSqliteStorage;
use nostr::Keys;
use thiserror::Error;

use crate::groups::{GroupInfo, Message};
use crate::{GroupId, MessagingError};

/// Errors specific to persistent storage initialization.
#[derive(Error, Debug)]
pub enum PersistentError {
    #[error("Storage initialization failed: {0}")]
    StorageInit(String),
    #[error("Messaging error: {0}")]
    Messaging(#[from] MessagingError),
}

/// A messaging client with encrypted SQLite persistence.
///
/// MLS group state, messages, and key packages survive restarts.
/// Uses SQLCipher for at-rest encryption with platform keyring key management.
pub struct PersistentMessagingClient {
    keys: Keys,
    mdk: MDK<MdkSqliteStorage>,
}

impl PersistentMessagingClient {
    /// Open or create a persistent messaging store.
    ///
    /// - `db_path`: path to SQLite database file (created if missing)
    /// - `service_id`: keyring service identifier (e.g., "nostring-messaging")
    /// - `db_key_id`: keyring key identifier (e.g., user's npub)
    pub fn open<P: AsRef<Path>>(
        keys: Keys,
        db_path: P,
        service_id: &str,
        db_key_id: &str,
    ) -> Result<Self, PersistentError> {
        let storage = MdkSqliteStorage::new(db_path, service_id, db_key_id)
            .map_err(|e| PersistentError::StorageInit(e.to_string()))?;

        Ok(Self {
            keys,
            mdk: MDK::new(storage),
        })
    }

    /// Open with an explicit encryption key (for environments without keyring).
    pub fn open_with_key<P: AsRef<Path>>(
        keys: Keys,
        db_path: P,
        encryption_key: [u8; 32],
    ) -> Result<Self, PersistentError> {
        let config = mdk_sqlite_storage::EncryptionConfig::new(encryption_key);
        let storage = MdkSqliteStorage::new_with_key(db_path, config)
            .map_err(|e| PersistentError::StorageInit(e.to_string()))?;

        Ok(Self {
            keys,
            mdk: MDK::new(storage),
        })
    }

    /// Open without encryption (for testing only).
    #[cfg(test)]
    pub fn open_unencrypted<P: AsRef<Path>>(
        keys: Keys,
        db_path: P,
    ) -> Result<Self, PersistentError> {
        let storage = MdkSqliteStorage::new_unencrypted(db_path)
            .map_err(|e| PersistentError::StorageInit(e.to_string()))?;

        Ok(Self {
            keys,
            mdk: MDK::new(storage),
        })
    }

    pub fn public_key(&self) -> nostr::PublicKey {
        self.keys.public_key()
    }

    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    pub fn create_key_package(
        &self,
        relay_urls: Vec<nostr::RelayUrl>,
    ) -> Result<(String, Vec<nostr::Tag>), MessagingError> {
        let (encoded, tags) = self
            .mdk
            .create_key_package_for_event(&self.keys.public_key(), relay_urls)?;
        Ok((encoded, tags))
    }

    pub fn get_groups(&self) -> Result<Vec<GroupInfo>, MessagingError> {
        let mdk_groups = self.mdk.get_groups()?;
        Ok(mdk_groups.into_iter().map(GroupInfo::from).collect())
    }

    pub fn get_members(&self, group_id: &GroupId) -> Result<Vec<nostr::PublicKey>, MessagingError> {
        let members = self.mdk.get_members(group_id)?;
        Ok(members.into_iter().collect())
    }

    pub fn get_messages(&self, group_id: &GroupId) -> Result<Vec<Message>, MessagingError> {
        let msgs = self
            .mdk
            .get_messages(group_id, None)
            .map_err(|e| MessagingError::Processing(e.to_string()))?;
        Ok(msgs.into_iter().map(Message::from).collect())
    }

    /// Access the underlying MDK instance.
    pub fn mdk(&self) -> &MDK<MdkSqliteStorage> {
        &self.mdk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persistent_open_unencrypted() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let keys = Keys::generate();

        let client = PersistentMessagingClient::open_unencrypted(keys, &db_path).unwrap();
        assert!(client.get_groups().unwrap().is_empty());

        // Verify DB file was created
        assert!(db_path.exists());
    }

    #[test]
    fn test_persistent_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let keys = Keys::generate();

        // Open, create a key package, close
        {
            let client =
                PersistentMessagingClient::open_unencrypted(keys.clone(), &db_path).unwrap();
            let relay = nostr::RelayUrl::parse("ws://localhost:8080").unwrap();
            let _ = client.create_key_package(vec![relay]).unwrap();
        }

        // Reopen — state should persist
        {
            let client = PersistentMessagingClient::open_unencrypted(keys, &db_path).unwrap();
            // Groups are empty (key packages don't create groups), but DB is valid
            assert!(client.get_groups().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn test_persistent_group_and_messages_survive_reopen() {
        use mdk_core::prelude::*;
        use mdk_storage_traits::test_utils::crypto_utils::generate_random_bytes;
        use nostr::event::builder::EventBuilder;
        use nostr::{EventId, Kind, RelayUrl};

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("persist_msg.db");
        let alice_keys = Keys::generate();
        let bob_keys = Keys::generate();
        let relay = RelayUrl::parse("ws://localhost:8080").unwrap();

        // Bob creates key package using in-memory client
        let bob_mem = crate::MessagingClient::new(bob_keys.clone());
        let (bob_kp, bob_tags) = bob_mem.create_key_package(vec![relay.clone()]).unwrap();
        let bob_kp_event = EventBuilder::new(Kind::MlsKeyPackage, bob_kp)
            .tags(bob_tags)
            .build(bob_keys.public_key())
            .sign(&bob_keys)
            .await
            .unwrap();

        let group_mls_id;
        let msg_event;

        // Alice: open persistent, create group, send message, close
        {
            let alice =
                PersistentMessagingClient::open_unencrypted(alice_keys.clone(), &db_path).unwrap();

            let image_hash: [u8; 32] = generate_random_bytes(32).try_into().unwrap();
            let image_key: [u8; 32] = generate_random_bytes(32).try_into().unwrap();
            let image_nonce: [u8; 12] = generate_random_bytes(12).try_into().unwrap();

            let config = NostrGroupConfigData::new(
                "persist-test".to_string(),
                "testing persistence".to_string(),
                Some(image_hash),
                Some(image_key),
                Some(image_nonce),
                vec![relay],
                vec![alice_keys.public_key(), bob_keys.public_key()],
            );

            let result = alice
                .mdk()
                .create_group(&alice_keys.public_key(), vec![bob_kp_event], config)
                .unwrap();

            group_mls_id = result.group.mls_group_id.clone();

            // Send a message
            let rumor = EventBuilder::new(Kind::Custom(9), "persisted hello")
                .build(alice_keys.public_key());
            msg_event = alice.mdk().create_message(&group_mls_id, rumor).unwrap();

            // Process our own message for local storage
            alice.mdk().process_message(&msg_event).unwrap();

            // Verify state before close
            let groups = alice.get_groups().unwrap();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].name, "persist-test");

            let msgs = alice.get_messages(&group_mls_id).unwrap();
            assert_eq!(msgs.len(), 1);
            assert_eq!(msgs[0].content, "persisted hello");
        }
        // alice drops here, DB should be flushed

        // Reopen and verify everything survived
        {
            let alice = PersistentMessagingClient::open_unencrypted(alice_keys, &db_path).unwrap();

            let groups = alice.get_groups().unwrap();
            assert_eq!(groups.len(), 1, "group should survive reopen");
            assert_eq!(groups[0].name, "persist-test");

            let msgs = alice.get_messages(&group_mls_id).unwrap();
            assert_eq!(msgs.len(), 1, "message should survive reopen");
            assert_eq!(msgs[0].content, "persisted hello");

            let members = alice.get_members(&group_mls_id).unwrap();
            assert_eq!(members.len(), 2, "members should survive reopen");
        }
    }

    #[test]
    fn test_persistent_open_with_key() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("encrypted.db");
        let keys = Keys::generate();
        let enc_key = [0x42u8; 32];

        // Open with explicit key
        {
            let client =
                PersistentMessagingClient::open_with_key(keys.clone(), &db_path, enc_key).unwrap();
            assert!(client.get_groups().unwrap().is_empty());
        }

        // Reopen with same key — should work
        {
            let client =
                PersistentMessagingClient::open_with_key(keys.clone(), &db_path, enc_key).unwrap();
            assert!(client.get_groups().unwrap().is_empty());
        }

        // Open with wrong key — should fail
        {
            let wrong_key = [0xFFu8; 32];
            let result = PersistentMessagingClient::open_with_key(keys, &db_path, wrong_key);
            assert!(result.is_err(), "wrong key should fail to open");
        }
    }
}
