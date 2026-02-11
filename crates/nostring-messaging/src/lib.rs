//! NoString Messaging — MLS encrypted group messaging via Marmot/MDK.
//!
//! Provides forward-secret, post-compromise-secure group messaging using
//! the MLS protocol (RFC 9420) with Nostr as transport.
//!
//! Built on [mdk-core](https://github.com/marmot-protocol/mdk) (MIT licensed),
//! which wraps OpenMLS with Nostr-specific group management.
//!
//! # Storage Backends
//!
//! `MessagingClient<S>` is generic over storage:
//! - `InMemoryClient` — ephemeral, for testing and short-lived sessions
//! - `PersistentClient` — encrypted SQLite, for production use

use mdk_core::prelude::*;
use mdk_memory_storage::MdkMemoryStorage;
use mdk_sqlite_storage::MdkSqliteStorage;
use mdk_storage_traits::MdkStorageProvider;
use nostr::Keys;
use thiserror::Error;

pub mod ccd;
pub mod groups;
pub mod persistent;
pub mod relay;

// Re-export key types for consumers
pub use mdk_core::GroupId;

#[derive(Error, Debug)]
pub enum MessagingError {
    #[error("MLS error: {0}")]
    Mls(String),
    #[error("Group not found: {0}")]
    GroupNotFound(String),
    #[error("Message processing error: {0}")]
    Processing(String),
    #[error("Storage initialization failed: {0}")]
    StorageInit(String),
}

impl From<mdk_core::Error> for MessagingError {
    fn from(e: mdk_core::Error) -> Self {
        MessagingError::Mls(e.to_string())
    }
}

/// Generic messaging client over any MDK storage backend.
pub struct MessagingClient<S: MdkStorageProvider> {
    keys: Keys,
    mdk: MDK<S>,
}

/// In-memory messaging client (ephemeral, for testing).
pub type InMemoryClient = MessagingClient<MdkMemoryStorage>;

/// Persistent messaging client (encrypted SQLite).
pub type PersistentClient = MessagingClient<MdkSqliteStorage>;

// === Shared methods for all storage backends ===

impl<S: MdkStorageProvider> MessagingClient<S> {
    /// Get the Nostr public key for this client.
    pub fn public_key(&self) -> nostr::PublicKey {
        self.keys.public_key()
    }

    /// Get a reference to the Nostr keys.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// Create a key package for publishing to Nostr relays.
    pub fn create_key_package(
        &self,
        relay_urls: Vec<nostr::RelayUrl>,
    ) -> Result<(String, Vec<nostr::Tag>), MessagingError> {
        let (encoded, tags) = self
            .mdk
            .create_key_package_for_event(&self.keys.public_key(), relay_urls)?;
        Ok((encoded, tags))
    }

    /// Get all groups this client is a member of.
    pub fn get_groups(&self) -> Result<Vec<groups::GroupInfo>, MessagingError> {
        let mdk_groups = self.mdk.get_groups()?;
        Ok(mdk_groups
            .into_iter()
            .map(groups::GroupInfo::from)
            .collect())
    }

    /// Get members of a group.
    pub fn get_members(&self, group_id: &GroupId) -> Result<Vec<nostr::PublicKey>, MessagingError> {
        let members = self.mdk.get_members(group_id)?;
        Ok(members.into_iter().collect())
    }

    /// Get messages from a group.
    pub fn get_messages(&self, group_id: &GroupId) -> Result<Vec<groups::Message>, MessagingError> {
        let msgs = self
            .mdk
            .get_messages(group_id, None)
            .map_err(|e| MessagingError::Processing(e.to_string()))?;
        Ok(msgs.into_iter().map(groups::Message::from).collect())
    }

    /// Get the underlying MDK instance (for advanced operations).
    pub fn mdk(&self) -> &MDK<S> {
        &self.mdk
    }
}

// === In-memory constructor ===

impl MessagingClient<MdkMemoryStorage> {
    /// Create a new in-memory messaging client.
    pub fn new(keys: Keys) -> Self {
        Self {
            keys,
            mdk: MDK::new(MdkMemoryStorage::default()),
        }
    }
}

// === Persistent constructors ===

impl MessagingClient<MdkSqliteStorage> {
    /// Open or create a persistent messaging store with platform keyring.
    pub fn open<P: AsRef<std::path::Path>>(
        keys: Keys,
        db_path: P,
        service_id: &str,
        db_key_id: &str,
    ) -> Result<Self, MessagingError> {
        let storage = MdkSqliteStorage::new(db_path, service_id, db_key_id)
            .map_err(|e| MessagingError::StorageInit(e.to_string()))?;
        Ok(Self {
            keys,
            mdk: MDK::new(storage),
        })
    }

    /// Open with an explicit encryption key (for environments without keyring).
    pub fn open_with_key<P: AsRef<std::path::Path>>(
        keys: Keys,
        db_path: P,
        encryption_key: [u8; 32],
    ) -> Result<Self, MessagingError> {
        let config = mdk_sqlite_storage::EncryptionConfig::new(encryption_key);
        let storage = MdkSqliteStorage::new_with_key(db_path, config)
            .map_err(|e| MessagingError::StorageInit(e.to_string()))?;
        Ok(Self {
            keys,
            mdk: MDK::new(storage),
        })
    }

    /// Open without encryption (for testing only).
    #[cfg(test)]
    pub fn open_unencrypted<P: AsRef<std::path::Path>>(
        keys: Keys,
        db_path: P,
    ) -> Result<Self, MessagingError> {
        let storage = MdkSqliteStorage::new_unencrypted(db_path)
            .map_err(|e| MessagingError::StorageInit(e.to_string()))?;
        Ok(Self {
            keys,
            mdk: MDK::new(storage),
        })
    }
}
