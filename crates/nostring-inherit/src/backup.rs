//! VaultBackup — serializable descriptor format shared between owner and heir apps.
//!
//! Contains everything an heir needs to reconstruct the vault,
//! find the UTXO on-chain, and build a claim transaction.

use serde::{Deserialize, Serialize};

use crate::taproot::{InheritError, InheritableVault};

/// Serializable vault descriptor backup.
///
/// Delivered via NIP-17 encrypted DM or physical backup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultBackup {
    /// Format version (for future compatibility)
    pub version: u32,
    /// Bitcoin network
    pub network: String,
    /// Owner's compressed public key (hex)
    pub owner_pubkey: String,
    /// Co-signer's compressed public key (hex)
    pub cosigner_pubkey: String,
    /// CCD chain code (hex, 32 bytes)
    pub chain_code: String,
    /// BIP-32 derivation index for this vault
    pub address_index: u32,
    /// Timelock in blocks
    pub timelock_blocks: u16,
    /// Threshold required for multi-heir claim (e.g., 2 of 3).
    /// For single heir, this is 1. For n-of-n, equals heirs.len().
    pub threshold: usize,
    /// Heir information
    pub heirs: Vec<HeirBackupEntry>,
    /// The vault's P2TR address (for verification)
    pub vault_address: String,
    /// Taproot internal key (hex, x-only aggregate pubkey before taptweak)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taproot_internal_key: Option<String>,
    /// Precompiled recovery scripts with control blocks (one per Tapscript leaf)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recovery_leaves: Vec<RecoveryLeaf>,
    /// ISO-8601 creation timestamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Precompiled Tapscript leaf — everything the heir needs to build a script-path spend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryLeaf {
    /// Index into this vec (matches heir's recovery_index)
    pub leaf_index: usize,
    /// Compiled miniscript as hex (the actual Script bytes)
    pub script_hex: String,
    /// Taproot control block for this leaf (hex)
    pub control_block_hex: String,
    /// CSV timelock value for this spending path
    pub timelock_blocks: u16,
    /// Tapscript leaf version (0xc0)
    pub leaf_version: u8,
}

/// Per-heir entry in the backup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeirBackupEntry {
    pub label: String,
    pub xpub: String,
    pub fingerprint: String,
    pub derivation_path: String,
    /// Which recovery script leaf this heir uses
    pub recovery_index: usize,
    /// Nostr npub for DM delivery
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npub: Option<String>,
}

/// Extract precompiled recovery leaves from a vault.
pub fn extract_recovery_leaves(vault: &InheritableVault) -> Vec<RecoveryLeaf> {
    use bitcoin::taproot::LeafVersion;
    vault
        .recovery_scripts
        .iter()
        .enumerate()
        .filter_map(|(i, (timelock, script))| {
            let cb = vault
                .taproot_spend_info
                .control_block(&(script.clone(), LeafVersion::TapScript))?;
            Some(RecoveryLeaf {
                leaf_index: i,
                script_hex: hex::encode(script.as_bytes()),
                control_block_hex: hex::encode(cb.serialize()),
                timelock_blocks: timelock.blocks(),
                leaf_version: LeafVersion::TapScript.to_consensus(),
            })
        })
        .collect()
}

impl VaultBackup {
    /// Reconstruct an InheritableVault from the backup data and verify the address matches.
    ///
    /// This proves the backup is valid — the vault_address in the backup must match
    /// the address computed from the raw key material.
    pub fn reconstruct(&self) -> Result<InheritableVault, InheritError> {
        use bitcoin::secp256k1::PublicKey;
        use nostring_ccd::types::DelegatedKey;

        // Parse owner pubkey
        let owner_bytes = hex::decode(&self.owner_pubkey)
            .map_err(|e| InheritError::Backup(format!("invalid owner_pubkey hex: {}", e)))?;
        let owner_pubkey = PublicKey::from_slice(&owner_bytes)
            .map_err(|e| InheritError::Backup(format!("invalid owner_pubkey: {}", e)))?;

        // Parse cosigner pubkey
        let cosigner_bytes = hex::decode(&self.cosigner_pubkey)
            .map_err(|e| InheritError::Backup(format!("invalid cosigner_pubkey hex: {}", e)))?;
        let cosigner_pubkey = PublicKey::from_slice(&cosigner_bytes)
            .map_err(|e| InheritError::Backup(format!("invalid cosigner_pubkey: {}", e)))?;

        // Parse chain code
        let chain_code_bytes = hex::decode(&self.chain_code)
            .map_err(|e| InheritError::Backup(format!("invalid chain_code hex: {}", e)))?;
        if chain_code_bytes.len() != 32 {
            return Err(InheritError::Backup(format!(
                "chain_code must be 32 bytes, got {}",
                chain_code_bytes.len()
            )));
        }
        let mut cc = [0u8; 32];
        cc.copy_from_slice(&chain_code_bytes);
        let chain_code = nostring_ccd::types::ChainCode(cc);

        // Build DelegatedKey and derive at index

        let delegated = DelegatedKey {
            cosigner_pubkey,
            chain_code,
            label: "backup-cosigner".into(),
        };

        // Parse heir xpubs into PathInfo
        use bitcoin::bip32::Xpub;
        use miniscript::DescriptorPublicKey;
        use std::str::FromStr;

        let heir_xpubs: Vec<Xpub> = self
            .heirs
            .iter()
            .map(|h| {
                Xpub::from_str(&h.xpub).map_err(|e| {
                    InheritError::Backup(format!("invalid heir xpub '{}': {}", h.label, e))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Convert xpubs to PathInfo (same logic as CcdState::heirs_to_path_info)
        let path_info = if heir_xpubs.len() == 1 {
            let xonly = heir_xpubs[0].public_key.x_only_public_key().0;
            let desc = DescriptorPublicKey::from_str(&format!("{}", xonly)).map_err(|e| {
                InheritError::Backup(format!("failed to create descriptor key: {}", e))
            })?;
            crate::policy::PathInfo::Single(desc)
        } else {
            let descs: Result<Vec<DescriptorPublicKey>, _> = heir_xpubs
                .iter()
                .map(|xpub| {
                    let xonly = xpub.public_key.x_only_public_key().0;
                    DescriptorPublicKey::from_str(&format!("{}", xonly))
                        .map_err(|e| InheritError::Backup(format!("descriptor key error: {}", e)))
                })
                .collect();
            let descs = descs?;
            let threshold = descs.len();
            crate::policy::PathInfo::Multi(threshold, descs)
        };

        let timelock = crate::policy::Timelock::from_blocks(self.timelock_blocks)
            .map_err(|e| InheritError::Backup(format!("invalid timelock: {}", e)))?;

        // Parse network
        let network = match self.network.as_str() {
            "mainnet" | "bitcoin" => bitcoin::Network::Bitcoin,
            "testnet" => bitcoin::Network::Testnet,
            "signet" => bitcoin::Network::Signet,
            "regtest" => bitcoin::Network::Regtest,
            _ => {
                return Err(InheritError::Backup(format!(
                    "unknown network: {}",
                    self.network
                )))
            }
        };

        // Create the vault using the existing function
        let vault = crate::taproot::create_inheritable_vault(
            &owner_pubkey,
            &delegated,
            self.address_index,
            path_info,
            timelock,
            0, // derivation_index for tapscript
            network,
        )?;

        // CRITICAL: Verify the address matches
        if vault.address.to_string() != self.vault_address {
            return Err(InheritError::Backup(format!(
                "address mismatch: computed {} but backup says {}. Backup may be corrupt or tampered.",
                vault.address, self.vault_address
            )));
        }

        Ok(vault)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_backup() -> VaultBackup {
        VaultBackup {
            version: 1,
            network: "bitcoin".into(),
            owner_pubkey: "02a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc"
                .into(),
            cosigner_pubkey:
                "03a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc".into(),
            chain_code: "ab".repeat(32),
            address_index: 0,
            timelock_blocks: 26280,
            threshold: 1,
            heirs: vec![HeirBackupEntry {
                label: "Alice".into(),
                xpub: "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8".into(),
                fingerprint: "00000000".into(),
                derivation_path: "m/84'/1'/0'".into(),
                recovery_index: 0,
                npub: None,
            }],
            vault_address: "placeholder".into(), // Will be set after reconstruction
            taproot_internal_key: None,
            recovery_leaves: vec![],
            created_at: None,
        }
    }

    #[test]
    fn test_backup_roundtrip() {
        let backup = sample_backup();
        let json = serde_json::to_string_pretty(&backup).unwrap();
        let restored: VaultBackup = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, 1);
        assert_eq!(restored.network, "bitcoin");
        assert_eq!(restored.heirs.len(), 1);
        assert_eq!(restored.heirs[0].label, "Alice");
    }

    #[test]
    fn test_backward_compat() {
        // Old v1 without new fields
        let old_json = serde_json::json!({
            "version": 1,
            "network": "bitcoin",
            "owner_pubkey": "02a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc",
            "cosigner_pubkey": "03a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc",
            "chain_code": "abababababababababababababababababababababababababababababababab",
            "address_index": 0,
            "timelock_blocks": 26280,
            "threshold": 1,
            "heirs": [{
                "label": "Alice",
                "xpub": "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8",
                "fingerprint": "00000000",
                "derivation_path": "m/84'/1'/0'",
                "recovery_index": 0
            }],
            "vault_address": "tb1ptest"
        });
        let restored: VaultBackup = serde_json::from_value(old_json).unwrap();
        assert!(restored.taproot_internal_key.is_none());
        assert!(restored.recovery_leaves.is_empty());
        assert!(restored.heirs[0].npub.is_none());
    }

    #[test]
    fn test_reconstruct_verifies_address() {
        // Create a backup with wrong address — reconstruct should fail
        let mut backup = sample_backup();
        backup.vault_address = "tb1pwrongaddress".into();
        let result = backup.reconstruct();
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(
            err.contains("address mismatch") || err.contains("invalid"),
            "Expected address mismatch error, got: {}",
            err
        );
    }

    #[test]
    fn test_reconstruct_valid() {
        // Create a vault first to get the correct address, then reconstruct from backup
        use bitcoin::bip32::Xpub;
        use bitcoin::secp256k1::PublicKey;
        use nostring_ccd::types::{ChainCode, DelegatedKey};
        use std::str::FromStr;

        let owner_pubkey = PublicKey::from_slice(
            &hex::decode("02a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc")
                .unwrap(),
        )
        .unwrap();
        let cosigner_pubkey = PublicKey::from_slice(
            &hex::decode("03a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc")
                .unwrap(),
        )
        .unwrap();
        let chain_code = ChainCode([0xab; 32]);
        let delegated = DelegatedKey {
            cosigner_pubkey,
            chain_code,
            label: "backup-cosigner".into(),
        };
        let heir_xpub = Xpub::from_str(
            "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8"
        ).unwrap();

        // Create vault to get the real address
        let xonly = heir_xpub.public_key.x_only_public_key().0;
        let desc = miniscript::DescriptorPublicKey::from_str(&format!("{}", xonly)).unwrap();
        let path_info = crate::policy::PathInfo::Single(desc);
        let timelock = crate::policy::Timelock::from_blocks(26280).unwrap();
        let vault = crate::taproot::create_inheritable_vault(
            &owner_pubkey,
            &delegated,
            0,
            path_info,
            timelock,
            0,
            bitcoin::Network::Bitcoin,
        )
        .unwrap();

        // Build backup with correct address
        let mut backup = sample_backup();
        backup.vault_address = vault.address.to_string();

        // Reconstruct should succeed
        let reconstructed = backup.reconstruct().unwrap();
        assert_eq!(reconstructed.address.to_string(), vault.address.to_string());
    }

    #[test]
    fn test_reconstruct_invalid_owner_pubkey() {
        let mut backup = sample_backup();
        backup.owner_pubkey = "deadbeef".into();
        assert!(backup.reconstruct().is_err());
    }

    #[test]
    fn test_reconstruct_invalid_chain_code() {
        let mut backup = sample_backup();
        backup.chain_code = "tooshort".into();
        assert!(backup.reconstruct().is_err());
    }
}
