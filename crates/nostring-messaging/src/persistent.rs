//! Persistent storage tests.
//!
//! The `PersistentClient` type alias and constructors (`open`, `open_with_key`,
//! `open_unencrypted`) live in `lib.rs` on `MessagingClient<MdkSqliteStorage>`.
//! This module contains tests for persistent storage behavior.

#[cfg(test)]
mod tests {
    use nostr::Keys;

    use crate::PersistentClient;

    #[test]
    fn test_persistent_open_unencrypted() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let keys = Keys::generate();

        let client = PersistentClient::open_unencrypted(keys, &db_path).unwrap();
        assert!(client.get_groups().unwrap().is_empty());
        assert!(db_path.exists());
    }

    #[test]
    fn test_persistent_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let keys = Keys::generate();

        {
            let client = PersistentClient::open_unencrypted(keys.clone(), &db_path).unwrap();
            let relay = nostr::RelayUrl::parse("ws://localhost:8080").unwrap();
            let _ = client.create_key_package(vec![relay]).unwrap();
        }

        {
            let client = PersistentClient::open_unencrypted(keys, &db_path).unwrap();
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
        let bob_mem = crate::InMemoryClient::new(bob_keys.clone());
        let (bob_kp, bob_tags) = bob_mem.create_key_package(vec![relay.clone()]).unwrap();
        let bob_kp_event = EventBuilder::new(Kind::MlsKeyPackage, bob_kp)
            .tags(bob_tags)
            .build(bob_keys.public_key())
            .sign(&bob_keys)
            .await
            .unwrap();

        let group_mls_id;

        // Alice: open persistent, create group, send message, close
        {
            let alice = PersistentClient::open_unencrypted(alice_keys.clone(), &db_path).unwrap();

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

            let rumor = EventBuilder::new(Kind::Custom(9), "persisted hello")
                .build(alice_keys.public_key());
            let msg_event = alice.mdk().create_message(&group_mls_id, rumor).unwrap();
            alice.mdk().process_message(&msg_event).unwrap();

            let groups = alice.get_groups().unwrap();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].name, "persist-test");

            let msgs = alice.get_messages(&group_mls_id).unwrap();
            assert_eq!(msgs.len(), 1);
            assert_eq!(msgs[0].content, "persisted hello");
        }

        // Reopen and verify
        {
            let alice = PersistentClient::open_unencrypted(alice_keys, &db_path).unwrap();

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

        {
            let client = PersistentClient::open_with_key(keys.clone(), &db_path, enc_key).unwrap();
            assert!(client.get_groups().unwrap().is_empty());
        }

        {
            let client = PersistentClient::open_with_key(keys.clone(), &db_path, enc_key).unwrap();
            assert!(client.get_groups().unwrap().is_empty());
        }

        {
            let wrong_key = [0xFFu8; 32];
            let result = PersistentClient::open_with_key(keys, &db_path, wrong_key);
            assert!(result.is_err(), "wrong key should fail to open");
        }
    }

    #[tokio::test]
    async fn test_persistent_encrypted_group_survives_reopen() {
        use mdk_core::prelude::*;
        use mdk_storage_traits::test_utils::crypto_utils::generate_random_bytes;
        use nostr::event::builder::EventBuilder;
        use nostr::{EventId, Kind, RelayUrl};

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("enc_persist.db");
        let alice_keys = Keys::generate();
        let bob_keys = Keys::generate();
        let enc_key = [0xABu8; 32];
        let relay = RelayUrl::parse("ws://localhost:8080").unwrap();

        let bob_mem = crate::InMemoryClient::new(bob_keys.clone());
        let (bob_kp, bob_tags) = bob_mem.create_key_package(vec![relay.clone()]).unwrap();
        let bob_kp_event = EventBuilder::new(Kind::MlsKeyPackage, bob_kp)
            .tags(bob_tags)
            .build(bob_keys.public_key())
            .sign(&bob_keys)
            .await
            .unwrap();

        let group_mls_id;

        // Create group with encrypted storage
        {
            let alice =
                PersistentClient::open_with_key(alice_keys.clone(), &db_path, enc_key).unwrap();

            let image_hash: [u8; 32] = generate_random_bytes(32).try_into().unwrap();
            let image_key: [u8; 32] = generate_random_bytes(32).try_into().unwrap();
            let image_nonce: [u8; 12] = generate_random_bytes(12).try_into().unwrap();

            let config = NostrGroupConfigData::new(
                "encrypted-group".to_string(),
                "encrypted persistence".to_string(),
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

            let rumor = EventBuilder::new(Kind::Custom(9), "encrypted hello")
                .build(alice_keys.public_key());
            let msg_event = alice.mdk().create_message(&group_mls_id, rumor).unwrap();
            alice.mdk().process_message(&msg_event).unwrap();
        }

        // Reopen with same key — state should survive
        {
            let alice = PersistentClient::open_with_key(alice_keys, &db_path, enc_key).unwrap();

            let groups = alice.get_groups().unwrap();
            assert_eq!(groups.len(), 1, "encrypted group should survive reopen");
            assert_eq!(groups[0].name, "encrypted-group");

            let msgs = alice.get_messages(&group_mls_id).unwrap();
            assert_eq!(msgs.len(), 1, "encrypted message should survive reopen");
            assert_eq!(msgs[0].content, "encrypted hello");
        }
    }
}
