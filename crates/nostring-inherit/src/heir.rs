//! Heir key management
//!
//! Handles importing and validating heir extended public keys.

use bitcoin::bip32::{DerivationPath, Fingerprint, Xpub};
use miniscript::descriptor::DescriptorPublicKey;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HeirError {
    #[error("Invalid xpub: {0}")]
    InvalidXpub(String),

    #[error("Missing fingerprint for xpub")]
    MissingFingerprint,

    #[error("Invalid derivation path: {0}")]
    InvalidDerivationPath(String),

    #[error("Parse error: {0}")]
    Parse(#[from] bitcoin::bip32::Error),
}

/// An heir's key information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeirKey {
    /// Human-readable label for this heir
    pub label: String,
    /// Master fingerprint of heir's device (hex string)
    #[serde(with = "fingerprint_serde")]
    pub fingerprint: Fingerprint,
    /// Extended public key (base58 string)
    #[serde(with = "xpub_serde")]
    pub xpub: Xpub,
    /// Derivation path from master (as string)
    #[serde(with = "derivation_path_serde")]
    pub derivation_path: DerivationPath,
}

/// Macro for creating serde modules that use FromStr/ToString
macro_rules! string_serde {
    ($mod_name:ident, $type:ty) => {
        mod $mod_name {
            use super::*;
            use serde::{Deserializer, Serializer};

            pub fn serialize<S>(value: &$type, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&value.to_string())
            }

            pub fn deserialize<'de, D>(deserializer: D) -> Result<$type, D::Error>
            where
                D: Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                <$type>::from_str(&s).map_err(serde::de::Error::custom)
            }
        }
    };
}

string_serde!(fingerprint_serde, Fingerprint);
string_serde!(xpub_serde, Xpub);
string_serde!(derivation_path_serde, DerivationPath);

impl HeirKey {
    /// Create a new heir key from components
    pub fn new(
        label: impl Into<String>,
        fingerprint: Fingerprint,
        xpub: Xpub,
        derivation_path: Option<DerivationPath>,
    ) -> Self {
        Self {
            label: label.into(),
            fingerprint,
            xpub,
            derivation_path: derivation_path
                .unwrap_or_else(|| DerivationPath::from_str("m/84'/0'/0'").unwrap()),
        }
    }

    /// Parse from a descriptor key string like "[fingerprint/path]xpub"
    pub fn from_descriptor_str(label: impl Into<String>, s: &str) -> Result<Self, HeirError> {
        let desc_key =
            DescriptorPublicKey::from_str(s).map_err(|e| HeirError::InvalidXpub(e.to_string()))?;

        // Extract origin (fingerprint + path) and xpub from either XPub or MultiXPub
        let (origin, xpub) = match desc_key {
            DescriptorPublicKey::XPub(xkey) => (xkey.origin, xkey.xkey),
            DescriptorPublicKey::MultiXPub(xkey) => (xkey.origin, xkey.xkey),
            _ => {
                return Err(HeirError::InvalidXpub(
                    "Expected xpub, got single key".into(),
                ))
            }
        };

        let (fingerprint, derivation_path) = origin.ok_or(HeirError::MissingFingerprint)?;

        Ok(Self {
            label: label.into(),
            fingerprint,
            xpub,
            derivation_path,
        })
    }

    /// Convert to a descriptor public key with multipath for receive/change
    pub fn to_descriptor_key(&self) -> DescriptorPublicKey {
        // Format: [fingerprint/path]xpub/<0;1>/*
        let key_str = format!(
            "[{}/{}]{}/<0;1>/*",
            self.fingerprint,
            self.derivation_path.to_string().trim_start_matches("m/"),
            self.xpub
        );
        DescriptorPublicKey::from_str(&key_str).expect("Valid descriptor key format")
    }

    /// Get the fingerprint
    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Collection of heirs for multi-heir inheritance
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeirRegistry {
    pub heirs: Vec<HeirKey>,
}

impl HeirRegistry {
    pub fn new() -> Self {
        Self { heirs: Vec::new() }
    }

    pub fn add(&mut self, heir: HeirKey) {
        self.heirs.push(heir);
    }

    pub fn remove(&mut self, fingerprint: &Fingerprint) -> Option<HeirKey> {
        if let Some(idx) = self
            .heirs
            .iter()
            .position(|h| &h.fingerprint == fingerprint)
        {
            Some(self.heirs.remove(idx))
        } else {
            None
        }
    }

    pub fn get(&self, fingerprint: &Fingerprint) -> Option<&HeirKey> {
        self.heirs.iter().find(|h| &h.fingerprint == fingerprint)
    }

    pub fn list(&self) -> &[HeirKey] {
        &self.heirs
    }

    pub fn is_empty(&self) -> bool {
        self.heirs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.heirs.len()
    }

    /// Get descriptor keys for all heirs
    pub fn to_descriptor_keys(&self) -> Vec<DescriptorPublicKey> {
        self.heirs.iter().map(|h| h.to_descriptor_key()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_xpub_str() -> &'static str {
        "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8"
    }

    #[test]
    fn test_heir_key_creation() {
        let xpub = Xpub::from_str(test_xpub_str()).unwrap();
        let fg = Fingerprint::from_str("00000001").unwrap();

        let heir = HeirKey::new("Alice", fg, xpub, None);

        assert_eq!(heir.label, "Alice");
        assert_eq!(heir.fingerprint, fg);
    }

    #[test]
    fn test_heir_from_descriptor_str() {
        let desc_str = format!("[00000001/84'/0'/0']{}", test_xpub_str());
        let heir = HeirKey::from_descriptor_str("Bob", &desc_str).unwrap();

        assert_eq!(heir.label, "Bob");
        assert_eq!(heir.fingerprint, Fingerprint::from_str("00000001").unwrap());
    }

    #[test]
    fn test_heir_to_descriptor_key() {
        let xpub = Xpub::from_str(test_xpub_str()).unwrap();
        let fg = Fingerprint::from_str("00000001").unwrap();
        let heir = HeirKey::new("Alice", fg, xpub, None);

        let desc_key = heir.to_descriptor_key();
        let key_str = desc_key.to_string();

        assert!(key_str.contains("[00000001/84'/0'/0']"));
        assert!(key_str.contains("/<0;1>/*"));
    }

    #[test]
    fn test_heir_registry() {
        let xpub = Xpub::from_str(test_xpub_str()).unwrap();
        let fg1 = Fingerprint::from_str("00000001").unwrap();
        let fg2 = Fingerprint::from_str("00000002").unwrap();

        let mut registry = HeirRegistry::new();
        assert!(registry.is_empty());

        registry.add(HeirKey::new("Alice", fg1, xpub, None));
        registry.add(HeirKey::new("Bob", fg2, xpub, None));

        assert_eq!(registry.len(), 2);
        assert!(registry.get(&fg1).is_some());
        assert_eq!(registry.get(&fg1).unwrap().label, "Alice");

        let removed = registry.remove(&fg1);
        assert!(removed.is_some());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_heir_serde_roundtrip() {
        let xpub = Xpub::from_str(test_xpub_str()).unwrap();
        let fg = Fingerprint::from_str("00000001").unwrap();
        let heir = HeirKey::new("Alice", fg, xpub, None);

        let json = serde_json::to_string(&heir).unwrap();
        let restored: HeirKey = serde_json::from_str(&json).unwrap();

        assert_eq!(heir.label, restored.label);
        assert_eq!(heir.fingerprint, restored.fingerprint);
        assert_eq!(heir.xpub, restored.xpub);
    }
}
