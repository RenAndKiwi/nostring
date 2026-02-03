//! Inheritance policy construction
//!
//! Build miniscript policies for timelock inheritance, adapted from Liana.
//!
//! # Policy Structure
//!
//! ```text
//! or_d(
//!   pk(OWNER),
//!   and_v(v:pkh(HEIR), older(TIMELOCK))
//! )
//! ```
//!
//! This creates a Bitcoin script where:
//! - The owner can spend at any time with their key
//! - The heir can only spend after TIMELOCK blocks have passed

use bitcoin::Sequence;
use miniscript::descriptor::DescriptorPublicKey;
use miniscript::policy::Concrete;
use miniscript::{Descriptor, Miniscript, Segwitv0};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PolicyError {
    #[error("Timelock must be positive and less than 2^16")]
    InvalidTimelock(u32),

    #[error("No recovery paths configured")]
    NoRecoveryPaths,

    #[error("Invalid threshold: {0} of {1} keys")]
    InvalidThreshold(usize, usize),

    #[error("Duplicate key in policy")]
    DuplicateKey,

    #[error("Missing key origin information")]
    MissingOrigin,

    #[error("Miniscript error: {0}")]
    Miniscript(#[from] miniscript::Error),

    #[error("Policy compilation failed: {0}")]
    Compilation(String),
}

/// Timelock duration in blocks (~10 min each)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timelock(u16);

impl Timelock {
    /// Create a timelock from a number of blocks
    pub fn from_blocks(blocks: u16) -> Result<Self, PolicyError> {
        if blocks == 0 {
            return Err(PolicyError::InvalidTimelock(blocks as u32));
        }
        Ok(Self(blocks))
    }

    /// 6 months (~26,280 blocks)
    pub fn six_months() -> Self {
        Self(26_280)
    }

    /// 1 year (~52,560 blocks)
    pub fn one_year() -> Self {
        Self(52_560)
    }

    /// Custom duration in days
    pub fn days(days: u16) -> Result<Self, PolicyError> {
        let blocks = (days as u32) * 144; // ~144 blocks per day
        if blocks > u16::MAX as u32 {
            return Err(PolicyError::InvalidTimelock(blocks));
        }
        Self::from_blocks(blocks as u16)
    }

    /// Get the block count
    pub fn blocks(&self) -> u16 {
        self.0
    }

    /// Convert to Bitcoin sequence value for CSV
    pub fn to_sequence(&self) -> Sequence {
        Sequence::from_height(self.0)
    }
}

/// Information about a spending path's keys
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathInfo {
    /// Single key
    Single(DescriptorPublicKey),
    /// Multi-signature: (threshold, keys)
    Multi(usize, Vec<DescriptorPublicKey>),
}

impl PathInfo {
    /// Create a single-key path
    pub fn single(key: DescriptorPublicKey) -> Self {
        Self::Single(key)
    }

    /// Create a multi-sig path
    pub fn multi(threshold: usize, keys: Vec<DescriptorPublicKey>) -> Result<Self, PolicyError> {
        if threshold == 0 || threshold > keys.len() {
            return Err(PolicyError::InvalidThreshold(threshold, keys.len()));
        }
        if keys.len() < 2 {
            return Err(PolicyError::InvalidThreshold(threshold, keys.len()));
        }
        Ok(Self::Multi(threshold, keys))
    }

    /// Get all keys in this path
    pub fn keys(&self) -> Vec<&DescriptorPublicKey> {
        match self {
            PathInfo::Single(key) => vec![key],
            PathInfo::Multi(_, keys) => keys.iter().collect(),
        }
    }

    /// Convert to a concrete policy
    fn to_policy(&self) -> Concrete<DescriptorPublicKey> {
        match self {
            PathInfo::Single(key) => Concrete::Key(key.clone()),
            PathInfo::Multi(thresh, keys) => {
                let key_policies: Vec<Arc<Concrete<DescriptorPublicKey>>> = keys
                    .iter()
                    .map(|k| Arc::new(Concrete::Key(k.clone())))
                    .collect();
                Concrete::Thresh(
                    miniscript::Threshold::new(*thresh, key_policies)
                        .expect("threshold already validated"),
                )
            }
        }
    }
}

/// A NoString inheritance policy
///
/// Defines who can spend (owner) and who inherits (recovery paths with timelocks).
#[derive(Debug, Clone)]
pub struct InheritancePolicy {
    /// Primary spending path (owner)
    pub primary: PathInfo,
    /// Recovery paths: timelock -> heir(s)
    pub recovery: BTreeMap<Timelock, PathInfo>,
}

impl InheritancePolicy {
    /// Create a new inheritance policy
    pub fn new(
        primary: PathInfo,
        recovery: BTreeMap<Timelock, PathInfo>,
    ) -> Result<Self, PolicyError> {
        if recovery.is_empty() {
            return Err(PolicyError::NoRecoveryPaths);
        }

        // Verify no duplicate keys across paths
        let mut seen_keys = std::collections::HashSet::new();
        for key in primary.keys() {
            if !seen_keys.insert(key.to_string()) {
                return Err(PolicyError::DuplicateKey);
            }
        }
        for path in recovery.values() {
            for key in path.keys() {
                if !seen_keys.insert(key.to_string()) {
                    return Err(PolicyError::DuplicateKey);
                }
            }
        }

        Ok(Self { primary, recovery })
    }

    /// Create a simple single-owner, single-heir policy
    pub fn simple(
        owner: DescriptorPublicKey,
        heir: DescriptorPublicKey,
        timelock: Timelock,
    ) -> Result<Self, PolicyError> {
        let mut recovery = BTreeMap::new();
        recovery.insert(timelock, PathInfo::Single(heir));
        Self::new(PathInfo::Single(owner), recovery)
    }

    /// Create a cascade inheritance policy with multiple heirs at different timelocks
    ///
    /// # Example
    /// ```ignore
    /// // Spouse can recover after 6 months, kids after 9 months, executor after 12 months
    /// let policy = InheritancePolicy::cascade(
    ///     owner_key,
    ///     vec![
    ///         (Timelock::six_months(), PathInfo::Single(spouse_key)),
    ///         (Timelock::days(270).unwrap(), PathInfo::multi(2, vec![kid1, kid2, kid3]).unwrap()),
    ///         (Timelock::one_year(), PathInfo::Single(executor_key)),
    ///     ],
    /// )?;
    /// ```
    pub fn cascade(
        owner: DescriptorPublicKey,
        heirs: Vec<(Timelock, PathInfo)>,
    ) -> Result<Self, PolicyError> {
        if heirs.is_empty() {
            return Err(PolicyError::NoRecoveryPaths);
        }

        let mut recovery = BTreeMap::new();
        for (timelock, path_info) in heirs {
            if recovery.contains_key(&timelock) {
                return Err(PolicyError::InvalidTimelock(timelock.blocks() as u32));
            }
            recovery.insert(timelock, path_info);
        }

        Self::new(PathInfo::Single(owner), recovery)
    }

    /// Create a multi-sig owner policy with cascade heirs
    ///
    /// Useful for corporate treasuries where multiple signatures are required
    /// for normal spending, but heirs can recover with their own thresholds.
    pub fn multisig_owner(
        owner_threshold: usize,
        owner_keys: Vec<DescriptorPublicKey>,
        heirs: Vec<(Timelock, PathInfo)>,
    ) -> Result<Self, PolicyError> {
        let primary = PathInfo::multi(owner_threshold, owner_keys)?;

        let mut recovery = BTreeMap::new();
        for (timelock, path_info) in heirs {
            recovery.insert(timelock, path_info);
        }

        Self::new(primary, recovery)
    }

    /// Get all timelocks in ascending order
    pub fn timelocks(&self) -> Vec<Timelock> {
        self.recovery.keys().copied().collect()
    }

    /// Get the earliest timelock (first recovery opportunity)
    pub fn earliest_timelock(&self) -> Option<Timelock> {
        self.recovery.keys().next().copied()
    }

    /// Get the latest timelock (final recovery opportunity)
    pub fn latest_timelock(&self) -> Option<Timelock> {
        self.recovery.keys().last().copied()
    }

    /// Count total recovery paths
    pub fn recovery_path_count(&self) -> usize {
        self.recovery.len()
    }

    /// Check if this is a cascade policy (multiple timelocks)
    pub fn is_cascade(&self) -> bool {
        self.recovery.len() > 1
    }

    /// Build a concrete policy (for compilation to miniscript)
    pub fn to_concrete_policy(&self) -> Concrete<DescriptorPublicKey> {
        // Primary path (owner)
        let primary = Arc::new(self.primary.to_policy());

        // Recovery paths as and(keys, older(timelock))
        let mut recovery_policies: Vec<Arc<Concrete<DescriptorPublicKey>>> = self
            .recovery
            .iter()
            .map(|(timelock, path_info)| {
                Arc::new(Concrete::And(vec![
                    Arc::new(path_info.to_policy()),
                    Arc::new(Concrete::Older(miniscript::RelLockTime::from_height(
                        timelock.blocks(),
                    ))),
                ]))
            })
            .collect();

        // Combine primary with all recovery paths using Or
        if recovery_policies.len() == 1 {
            Concrete::Or(vec![(1, primary), (1, recovery_policies.remove(0))])
        } else {
            // Multiple recovery paths: or(primary, or(recovery1, or(recovery2, ...)))
            let mut combined_recovery = recovery_policies.pop().unwrap();
            while let Some(path) = recovery_policies.pop() {
                combined_recovery = Arc::new(Concrete::Or(vec![(1, path), (1, combined_recovery)]));
            }
            Concrete::Or(vec![(1, primary), (1, combined_recovery)])
        }
    }

    /// Compile to a P2WSH descriptor
    pub fn to_wsh_descriptor(&self) -> Result<Descriptor<DescriptorPublicKey>, PolicyError> {
        let policy = self.to_concrete_policy();
        let ms: Miniscript<DescriptorPublicKey, Segwitv0> = policy
            .compile()
            .map_err(|e| PolicyError::Compilation(e.to_string()))?;
        Ok(Descriptor::new_wsh(ms)?)
    }
}

impl fmt::Display for Timelock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let days = self.0 / 144;
        if days >= 365 {
            write!(f, "~{:.1} years ({} blocks)", days as f32 / 365.0, self.0)
        } else if days >= 30 {
            write!(f, "~{:.1} months ({} blocks)", days as f32 / 30.0, self.0)
        } else {
            write!(f, "~{} days ({} blocks)", days, self.0)
        }
    }
}

impl Ord for Timelock {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for Timelock {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl InheritancePolicy {
    /// Create a simple policy with a multi-sig heir group
    pub fn simple_with_multisig_heir(
        owner: DescriptorPublicKey,
        heirs: PathInfo,
        timelock: Timelock,
    ) -> Result<Self, PolicyError> {
        let mut recovery = BTreeMap::new();
        recovery.insert(timelock, heirs);
        Self::new(PathInfo::Single(owner), recovery)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::bip32::Xpub;
    use std::str::FromStr;

    fn test_xpub() -> Xpub {
        // Standard test xpub
        Xpub::from_str("xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8").unwrap()
    }

    fn owner_key() -> DescriptorPublicKey {
        let xpub = test_xpub();
        DescriptorPublicKey::from_str(&format!("[00000001/84'/0'/0']{}/<0;1>/*", xpub)).unwrap()
    }

    fn heir_key() -> DescriptorPublicKey {
        let xpub = test_xpub();
        // Use different derivation path for heir
        DescriptorPublicKey::from_str(&format!("[00000002/84'/0'/1']{}/<0;1>/*", xpub)).unwrap()
    }

    #[test]
    fn test_timelock_creation() {
        let tl = Timelock::from_blocks(1000).unwrap();
        assert_eq!(tl.blocks(), 1000);

        let tl = Timelock::six_months();
        assert_eq!(tl.blocks(), 26_280);

        let tl = Timelock::one_year();
        assert_eq!(tl.blocks(), 52_560);

        let tl = Timelock::days(30).unwrap();
        assert_eq!(tl.blocks(), 4320); // 30 * 144

        // Zero timelock should fail
        assert!(Timelock::from_blocks(0).is_err());
    }

    #[test]
    fn test_timelock_display() {
        let tl = Timelock::six_months();
        let display = format!("{}", tl);
        assert!(display.contains("months"));
        assert!(display.contains("26280"));
    }

    #[test]
    fn test_simple_policy_creation() {
        let policy =
            InheritancePolicy::simple(owner_key(), heir_key(), Timelock::six_months()).unwrap();

        assert!(matches!(policy.primary, PathInfo::Single(_)));
        assert_eq!(policy.recovery.len(), 1);
    }

    #[test]
    fn test_policy_requires_recovery() {
        let result = InheritancePolicy::new(
            PathInfo::Single(owner_key()),
            BTreeMap::new(), // No recovery paths
        );
        assert!(matches!(result, Err(PolicyError::NoRecoveryPaths)));
    }

    #[test]
    fn test_concrete_policy_generation() {
        let policy =
            InheritancePolicy::simple(owner_key(), heir_key(), Timelock::six_months()).unwrap();

        let concrete = policy.to_concrete_policy();
        let policy_str = format!("{}", concrete);

        // Should contain or() with older()
        assert!(policy_str.contains("or("));
        assert!(policy_str.contains("older("));
    }

    #[test]
    fn test_wsh_descriptor_compilation() {
        let policy =
            InheritancePolicy::simple(owner_key(), heir_key(), Timelock::six_months()).unwrap();

        let descriptor = policy.to_wsh_descriptor().unwrap();
        let desc_str = format!("{}", descriptor);

        // Should be a wsh descriptor
        assert!(desc_str.starts_with("wsh("));
        println!("Generated descriptor: {}", desc_str);
    }

    // === Phase 4: Multi-Heir + Cascade Tests ===

    fn heir_key_2() -> DescriptorPublicKey {
        let xpub = test_xpub();
        DescriptorPublicKey::from_str(&format!("[00000003/84'/0'/2']{}/<0;1>/*", xpub)).unwrap()
    }

    fn heir_key_3() -> DescriptorPublicKey {
        let xpub = test_xpub();
        DescriptorPublicKey::from_str(&format!("[00000004/84'/0'/3']{}/<0;1>/*", xpub)).unwrap()
    }

    fn spouse_key() -> DescriptorPublicKey {
        let xpub = test_xpub();
        DescriptorPublicKey::from_str(&format!("[00000005/84'/0'/5']{}/<0;1>/*", xpub)).unwrap()
    }

    fn executor_key() -> DescriptorPublicKey {
        let xpub = test_xpub();
        DescriptorPublicKey::from_str(&format!("[00000006/84'/0'/6']{}/<0;1>/*", xpub)).unwrap()
    }

    #[test]
    fn test_cascade_policy_creation() {
        // Spouse at 6 months, executor at 12 months
        let policy = InheritancePolicy::cascade(
            owner_key(),
            vec![
                (Timelock::six_months(), PathInfo::Single(heir_key())),
                (Timelock::one_year(), PathInfo::Single(heir_key_2())),
            ],
        )
        .unwrap();

        assert!(policy.is_cascade());
        assert_eq!(policy.recovery_path_count(), 2);
        assert_eq!(policy.earliest_timelock(), Some(Timelock::six_months()));
        assert_eq!(policy.latest_timelock(), Some(Timelock::one_year()));
    }

    #[test]
    fn test_cascade_timelocks_ordered() {
        let policy = InheritancePolicy::cascade(
            owner_key(),
            vec![
                // Add in random order - should be sorted by timelock
                (Timelock::one_year(), PathInfo::Single(heir_key_3())),
                (Timelock::six_months(), PathInfo::Single(heir_key())),
                (Timelock::days(270).unwrap(), PathInfo::Single(heir_key_2())),
            ],
        )
        .unwrap();

        let timelocks = policy.timelocks();
        assert_eq!(timelocks.len(), 3);
        // Should be sorted ascending
        assert!(timelocks[0].blocks() < timelocks[1].blocks());
        assert!(timelocks[1].blocks() < timelocks[2].blocks());
    }

    #[test]
    fn test_cascade_duplicate_timelock_rejected() {
        let result = InheritancePolicy::cascade(
            owner_key(),
            vec![
                (Timelock::six_months(), PathInfo::Single(heir_key())),
                (Timelock::six_months(), PathInfo::Single(heir_key_2())), // Duplicate!
            ],
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_multisig_heir_threshold() {
        // Owner can spend, but after 6 months, 2-of-3 heirs can recover
        let heirs = PathInfo::multi(2, vec![heir_key(), heir_key_2(), heir_key_3()]).unwrap();

        let policy = InheritancePolicy::simple_with_multisig_heir(
            owner_key(),
            heirs,
            Timelock::six_months(),
        );

        assert!(policy.is_ok());
        let policy = policy.unwrap();
        assert!(!policy.is_cascade()); // Single recovery path
        assert_eq!(policy.recovery_path_count(), 1);
    }

    #[test]
    fn test_cascade_with_multisig_heirs() {
        // Complex cascade:
        // - 6 months: spouse alone
        // - 9 months: 2-of-3 kids
        // - 12 months: executor alone

        let kids = PathInfo::multi(2, vec![heir_key(), heir_key_2(), heir_key_3()]).unwrap();

        let policy = InheritancePolicy::cascade(
            owner_key(),
            vec![
                (Timelock::six_months(), PathInfo::Single(spouse_key())),
                (Timelock::days(270).unwrap(), kids),
                (Timelock::one_year(), PathInfo::Single(executor_key())),
            ],
        )
        .unwrap();

        assert!(policy.is_cascade());
        assert_eq!(policy.recovery_path_count(), 3);
    }

    #[test]
    fn test_cascade_compiles_to_wsh() {
        let policy = InheritancePolicy::cascade(
            owner_key(),
            vec![
                (Timelock::six_months(), PathInfo::Single(heir_key())),
                (Timelock::one_year(), PathInfo::Single(heir_key_2())),
            ],
        )
        .unwrap();

        let descriptor = policy.to_wsh_descriptor();
        assert!(
            descriptor.is_ok(),
            "Cascade policy should compile: {:?}",
            descriptor.err()
        );

        let desc_str = descriptor.unwrap().to_string();
        assert!(desc_str.starts_with("wsh("));
        println!("Cascade descriptor: {}", desc_str);
    }

    #[test]
    fn test_multisig_owner_with_cascade() {
        // 2-of-2 corporate owners, with cascade heirs
        let policy = InheritancePolicy::multisig_owner(
            2,
            vec![owner_key(), heir_key()], // Using heir_key as second owner for test
            vec![
                (Timelock::six_months(), PathInfo::Single(heir_key_2())),
                (Timelock::one_year(), PathInfo::Single(heir_key_3())),
            ],
        )
        .unwrap();

        assert!(policy.is_cascade());

        // Primary should be multi-sig
        match &policy.primary {
            PathInfo::Multi(thresh, keys) => {
                assert_eq!(*thresh, 2);
                assert_eq!(keys.len(), 2);
            }
            _ => panic!("Expected multi-sig primary path"),
        }
    }
}
