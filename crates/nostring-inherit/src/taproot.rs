//! Inheritable CCD Vault — Taproot outputs with key-path (CCD) + script-path (heir timelock).
//!
//! Combines CCD's privacy-preserving collaborative custody with miniscript-based
//! timelock inheritance. During normal operation, the owner spends via key-path
//! (indistinguishable from single-sig on chain). If the owner stops checking in,
//! the heir can claim via script-path after the timelock expires.
//!
//! # Taproot Structure
//!
//! ```text
//! Output key = taptweak(internal_key, merkle_root)
//!   Key path:    MuSig2(owner, cosigner)       <- owner spends normally via CCD
//!   Script path: and(pk(HEIR), older(TIMELOCK)) <- heir spends after timelock
//! ```
//!
//! # Check-in
//!
//! Any key-path spend resets the timelock (CSV is relative to UTXO age).
//! The owner just uses the vault normally; every spend is a check-in.

use bitcoin::key::XOnlyPublicKey;
use bitcoin::secp256k1::{PublicKey, Secp256k1};
use bitcoin::taproot::{LeafVersion, TaprootBuilder, TaprootSpendInfo};
use bitcoin::{Address, Amount, Network, ScriptBuf, Sequence, TxOut};
use miniscript::descriptor::DescriptorPublicKey;
use miniscript::{Miniscript, Tap};
use thiserror::Error;

use crate::heir::HeirKey;
use crate::policy::{PathInfo, Timelock};
use nostring_ccd::types::{CcdError, DelegatedKey};
use nostring_ccd::{aggregate_taproot_key, compute_tweak};

#[derive(Error, Debug)]
pub enum InheritError {
    #[error("CCD error: {0}")]
    Ccd(#[from] CcdError),

    #[error("Policy error: {0}")]
    Policy(#[from] crate::policy::PolicyError),

    #[error("Taproot construction failed: {0}")]
    Taproot(String),

    #[error("PSBT error: {0}")]
    Psbt(String),

    #[error("No heirs configured")]
    NoHeirs,

    #[error("Miniscript error: {0}")]
    Miniscript(#[from] miniscript::Error),
}

/// An inheritable CCD vault with both key-path and script-path spending.
#[derive(Clone)]
pub struct InheritableVault {
    // CCD fields
    /// Owner's public key
    pub owner_pubkey: PublicKey,
    /// Co-signer's delegated key info
    pub delegated: DelegatedKey,
    /// BIP-32 child index for this vault
    pub address_index: u32,
    /// Co-signer's derived pubkey at this index
    pub cosigner_derived_pubkey: PublicKey,
    /// MuSig2 aggregate x-only key (internal key)
    pub aggregate_xonly: XOnlyPublicKey,

    // Inheritance fields
    /// The primary (earliest) timelock for this vault
    pub timelock: Timelock,
    /// Compiled recovery tapscript leaves (timelock, script) pairs
    pub recovery_scripts: Vec<(Timelock, ScriptBuf)>,
    /// Taproot spend info (internal key + script tree)
    pub taproot_spend_info: TaprootSpendInfo,

    /// The final P2TR address (with script commitment)
    pub address: Address,
    pub network: Network,
}

/// Create an inheritable CCD vault.
///
/// The vault address commits to both:
/// - Key path: MuSig2(owner, cosigner) for normal spending
/// - Script path(s): recovery path(s) compiled from the inheritance policy
///
/// For single-heir: one script leaf with `and(pk(heir), older(timelock))`
/// For multi-heir threshold: one leaf with `and(multi_a(k, heirs...), older(timelock))`
/// For cascade: multiple leaves at different timelocks
///
/// # Arguments
/// - `owner_pubkey`: Owner's secp256k1 public key
/// - `delegated`: Co-signer's delegated key (with chain code)
/// - `address_index`: BIP-32 child index for this vault
/// - `heirs`: Heir key(s) with optional threshold configuration
/// - `timelock`: CSV timelock for recovery
/// - `derivation_index`: Index for deriving concrete keys from descriptor xpubs
/// - `network`: Bitcoin network
pub fn create_inheritable_vault(
    owner_pubkey: &PublicKey,
    delegated: &DelegatedKey,
    address_index: u32,
    heirs: PathInfo,
    timelock: Timelock,
    derivation_index: u32,
    network: Network,
) -> Result<InheritableVault, InheritError> {
    // Compile the recovery path directly to Tapscript.
    // No dummy owner key needed — the owner's spending path is the Taproot
    // key-path (MuSig2 aggregate), not a script leaf.
    let recovery_ms = compile_recovery_to_tapscript(&heirs, &timelock)?;
    let recovery_script = derive_tapscript(&recovery_ms, derivation_index)?;

    let secp = Secp256k1::new();
    let recovery_scripts = vec![(timelock, recovery_script)];

    // Derive co-signer's child pubkey via CCD
    let disclosure = compute_tweak(delegated, address_index)?;
    let cosigner_derived = disclosure.derived_pubkey;

    // Internal key = MuSig2 aggregate
    let aggregate_xonly = aggregate_taproot_key(owner_pubkey, &cosigner_derived)?;

    // Build Taproot tree from recovery scripts
    let taproot_spend_info = build_taproot_tree(&secp, aggregate_xonly, &recovery_scripts)?;

    let address = Address::p2tr(
        &secp,
        aggregate_xonly,
        taproot_spend_info.merkle_root(),
        network,
    );

    Ok(InheritableVault {
        owner_pubkey: *owner_pubkey,
        delegated: delegated.clone(),
        address_index,
        cosigner_derived_pubkey: cosigner_derived,
        aggregate_xonly,
        timelock,
        recovery_scripts,
        taproot_spend_info,
        address,
        network,
    })
}

/// Builder for creating inheritable vaults from `HeirKey`s.
///
/// Provides a clean API for configuring vault parameters step by step.
///
/// # Example
/// ```ignore
/// let vault = InheritableVaultBuilder::new(owner_pk, delegated, Network::Testnet)
///     .heir(alice_key)
///     .heir(bob_key)
///     .heir(carol_key)
///     .threshold(2)  // 2-of-3
///     .timelock(Timelock::six_months())
///     .build()?;
/// ```
pub struct InheritableVaultBuilder {
    owner_pubkey: PublicKey,
    delegated: DelegatedKey,
    network: Network,
    heir_keys: Vec<HeirKey>,
    heir_threshold: Option<usize>,
    timelock: Option<Timelock>,
    address_index: u32,
    derivation_index: u32,
}

impl InheritableVaultBuilder {
    /// Start building a vault with required CCD parameters.
    pub fn new(owner_pubkey: PublicKey, delegated: DelegatedKey, network: Network) -> Self {
        Self {
            owner_pubkey,
            delegated,
            network,
            heir_keys: Vec::new(),
            heir_threshold: None,
            timelock: None,
            address_index: 0,
            derivation_index: 0,
        }
    }

    /// Add an heir.
    pub fn heir(mut self, key: HeirKey) -> Self {
        self.heir_keys.push(key);
        self
    }

    /// Set the threshold for multi-heir (e.g., 2-of-3). Defaults to all heirs required.
    pub fn threshold(mut self, k: usize) -> Self {
        self.heir_threshold = Some(k);
        self
    }

    /// Set the CSV timelock for recovery.
    pub fn timelock(mut self, tl: Timelock) -> Self {
        self.timelock = Some(tl);
        self
    }

    /// Set the BIP-32 address index for CCD derivation. Defaults to 0.
    pub fn address_index(mut self, idx: u32) -> Self {
        self.address_index = idx;
        self
    }

    /// Set the derivation index for resolving descriptor xpub wildcards. Defaults to 0.
    pub fn derivation_index(mut self, idx: u32) -> Self {
        self.derivation_index = idx;
        self
    }

    /// Build the inheritable vault.
    pub fn build(self) -> Result<InheritableVault, InheritError> {
        if self.heir_keys.is_empty() {
            return Err(InheritError::NoHeirs);
        }
        let timelock = self
            .timelock
            .ok_or_else(|| InheritError::Taproot("timelock is required".into()))?;

        let desc_keys: Vec<_> = self
            .heir_keys
            .iter()
            .map(|h| h.to_descriptor_key())
            .collect();
        let threshold = self.heir_threshold.unwrap_or(desc_keys.len());

        let heirs = if desc_keys.len() == 1 {
            PathInfo::Single(desc_keys.into_iter().next().unwrap())
        } else {
            PathInfo::multi(threshold, desc_keys).map_err(InheritError::Policy)?
        };

        create_inheritable_vault(
            &self.owner_pubkey,
            &self.delegated,
            self.address_index,
            heirs,
            timelock,
            self.derivation_index,
            self.network,
        )
    }
}

/// Create an inheritable vault with cascade inheritance (multiple timelocks).
///
/// Each (Timelock, PathInfo) pair becomes a separate script leaf.
pub fn create_cascade_vault(
    owner_pubkey: &PublicKey,
    delegated: &DelegatedKey,
    address_index: u32,
    recovery_paths: Vec<(Timelock, PathInfo)>,
    derivation_index: u32,
    network: Network,
) -> Result<InheritableVault, InheritError> {
    if recovery_paths.is_empty() {
        return Err(InheritError::NoHeirs);
    }

    // Compile each recovery path directly to Tapscript — no dummy owner key.
    let secp = Secp256k1::new();
    let mut recovery_scripts = Vec::new();
    for (tl, path_info) in &recovery_paths {
        let ms = compile_recovery_to_tapscript(path_info, tl)?;
        let concrete = derive_tapscript(&ms, derivation_index)?;
        recovery_scripts.push((*tl, concrete));
    }

    let disclosure = compute_tweak(delegated, address_index)?;
    let cosigner_derived = disclosure.derived_pubkey;
    let aggregate_xonly = aggregate_taproot_key(owner_pubkey, &cosigner_derived)?;

    let taproot_spend_info = build_taproot_tree(&secp, aggregate_xonly, &recovery_scripts)?;

    // Primary timelock is the earliest (most likely to be used)
    let primary_timelock = recovery_scripts
        .iter()
        .map(|(tl, _)| *tl)
        .min_by_key(|tl| tl.blocks())
        .expect("non-empty recovery_paths");

    let address = Address::p2tr(
        &secp,
        aggregate_xonly,
        taproot_spend_info.merkle_root(),
        network,
    );

    Ok(InheritableVault {
        owner_pubkey: *owner_pubkey,
        delegated: delegated.clone(),
        address_index,
        cosigner_derived_pubkey: cosigner_derived,
        aggregate_xonly,
        timelock: primary_timelock,
        recovery_scripts,
        taproot_spend_info,
        address,
        network,
    })
}

/// Build an unsigned PSBT for heir to claim funds via script-path.
///
/// Returns an unsigned PSBT. The caller handles signing (hardware wallet,
/// software signer, etc.).
///
/// # Arguments
/// - `vault`: The inheritable vault being claimed from
/// - `recovery_index`: Which recovery path to use (0 for single-heir, index for cascade)
/// - `utxos`: UTXOs to spend (outpoint + TxOut)
/// - `destination`: Where to send the funds
/// - `fee`: Transaction fee
pub fn build_heir_claim_psbt(
    vault: &InheritableVault,
    recovery_index: usize,
    utxos: &[(bitcoin::OutPoint, TxOut)],
    destination: &Address,
    fee: Amount,
) -> Result<bitcoin::psbt::Psbt, InheritError> {
    use bitcoin::psbt::Psbt;
    use bitcoin::transaction::{Transaction, TxIn, Version};

    if utxos.is_empty() {
        return Err(InheritError::Psbt("no UTXOs provided".into()));
    }

    let (timelock, recovery_script) =
        vault.recovery_scripts.get(recovery_index).ok_or_else(|| {
            InheritError::Psbt(format!("recovery index {} out of bounds", recovery_index))
        })?;

    // Total input value
    let total_in: Amount = utxos.iter().map(|(_, txout)| txout.value).sum();

    let send_amount = total_in
        .checked_sub(fee)
        .ok_or_else(|| InheritError::Psbt("fee exceeds total input value".into()))?;

    if send_amount.to_sat() < 546 {
        return Err(InheritError::Psbt(format!(
            "output {} sat is below dust limit (546 sat)",
            send_amount.to_sat()
        )));
    }

    // Build inputs with CSV sequence
    let inputs: Vec<TxIn> = utxos
        .iter()
        .map(|(outpoint, _)| TxIn {
            previous_output: *outpoint,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::from_height(timelock.blocks()),
            witness: bitcoin::Witness::new(),
        })
        .collect();

    let tx = Transaction {
        version: Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: inputs,
        output: vec![TxOut {
            value: send_amount,
            script_pubkey: destination.script_pubkey(),
        }],
    };

    let mut psbt = Psbt::from_unsigned_tx(tx)
        .map_err(|e| InheritError::Psbt(format!("PSBT creation failed: {}", e)))?;

    // Get control block for this recovery script
    let control_block = vault
        .taproot_spend_info
        .control_block(&(recovery_script.clone(), LeafVersion::TapScript))
        .ok_or_else(|| InheritError::Psbt("control block not found for recovery script".into()))?;

    // Populate each input
    for (i, (_, txout)) in utxos.iter().enumerate() {
        psbt.inputs[i].witness_utxo = Some(txout.clone());

        psbt.inputs[i].tap_scripts.insert(
            control_block.clone(),
            (recovery_script.clone(), LeafVersion::TapScript),
        );

        psbt.inputs[i].tap_internal_key = Some(vault.aggregate_xonly);
        psbt.inputs[i].tap_merkle_root = vault.taproot_spend_info.merkle_root();
    }

    Ok(psbt)
}

/// Estimate vbytes for a script-path heir claim transaction.
///
/// Script-path spending is heavier than key-path due to the control block
/// and script in the witness.
pub fn estimate_heir_claim_vbytes(
    num_inputs: usize,
    num_outputs: usize,
    tree_depth: usize,
) -> usize {
    // Weight units per input (script-path):
    //   Base: (36+1+4)*4 = 164 WU
    //   Witness: items_count(1) + sig(1+64) + script(~40-100) + control_block(1+33+32*depth)
    //   Approx witness: 1 + 65 + 50 + 33 + 32*depth = 149 + 32*depth
    //   Total per input: 164 + 149 + 32*depth = 313 + 32*depth WU
    let overhead_wu = 42;
    let input_wu = 313 + 32 * tree_depth;
    let output_wu = 172; // P2TR
    let total_wu = overhead_wu + (num_inputs * input_wu) + (num_outputs * output_wu);
    total_wu.div_ceil(4) + 1
}

// --- Internal helpers ---

/// Derive a concrete Tapscript from a compiled miniscript at a specific derivation index.
///
/// This resolves wildcard DescriptorPublicKeys (e.g., xpub/<0;1>/*) into
/// concrete x-only keys at the given index, then encodes the script.
fn derive_tapscript(
    ms: &Miniscript<DescriptorPublicKey, Tap>,
    derivation_index: u32,
) -> Result<ScriptBuf, InheritError> {
    // Translate DescriptorPublicKey -> DefiniteDescriptorKey -> concrete script
    // First, we need to convert multi-path keys to single-path (receive path = index 0)
    // Then derive at the specific index

    // The miniscript string representation can be parsed with concrete keys.
    // We'll use the Translator trait to map each DescriptorPublicKey to a concrete key.

    use miniscript::descriptor::DefiniteDescriptorKey;
    use miniscript::{TranslatePk, Translator};

    struct KeyDeriver {
        index: u32,
    }

    impl Translator<DescriptorPublicKey, DefiniteDescriptorKey, InheritError> for KeyDeriver {
        fn pk(&mut self, pk: &DescriptorPublicKey) -> Result<DefiniteDescriptorKey, InheritError> {
            // For multi-path descriptors, pick the receive path (first)
            let single_keys = pk.clone().into_single_keys();

            let receive_key = single_keys
                .into_iter()
                .next()
                .ok_or_else(|| InheritError::Taproot("empty key after single_keys split".into()))?;

            // Derive at the specific index
            let derived = receive_key
                .at_derivation_index(self.index)
                .map_err(|e| InheritError::Taproot(format!("key derivation failed: {}", e)))?;

            Ok(derived)
        }

        miniscript::translate_hash_fail!(DescriptorPublicKey, DefiniteDescriptorKey, InheritError);
    }

    let mut deriver = KeyDeriver {
        index: derivation_index,
    };

    let concrete_ms = ms
        .translate_pk(&mut deriver)
        .map_err(|e| InheritError::Taproot(format!("key translation failed: {:?}", e)))?;

    Ok(concrete_ms.encode())
}

/// Build a Taproot tree from recovery scripts.
///
/// For a single script, creates a single leaf at depth 0.
/// For multiple scripts, builds a balanced tree (earlier timelocks at lower depth
/// for efficiency, since they're more likely to be used).
fn build_taproot_tree(
    secp: &Secp256k1<bitcoin::secp256k1::All>,
    internal_key: XOnlyPublicKey,
    scripts: &[(Timelock, ScriptBuf)],
) -> Result<TaprootSpendInfo, InheritError> {
    if scripts.is_empty() {
        return Err(InheritError::NoHeirs);
    }

    let mut builder = TaprootBuilder::new();

    if scripts.len() == 1 {
        // Single leaf at depth 0
        builder = builder
            .add_leaf(0, scripts[0].1.clone())
            .map_err(|e| InheritError::Taproot(format!("taproot builder error: {}", e)))?;
    } else {
        // Build a Huffman-like tree: earlier timelocks (more likely to be used)
        // get shallower depth. TaprootBuilder::add_leaf fills slots left-to-right.
        //
        // For n leaves, we compute depths that form a valid binary tree.
        // Simple approach: all leaves at the same depth for power-of-2,
        // otherwise put earlier leaves at lower depth.
        let depths = compute_leaf_depths(scripts.len());
        for (i, (_, script)) in scripts.iter().enumerate() {
            builder = builder.add_leaf(depths[i], script.clone()).map_err(|e| {
                InheritError::Taproot(format!("taproot builder error at leaf {}: {}", i, e))
            })?;
        }
    }

    builder
        .finalize(secp, internal_key)
        .map_err(|_| InheritError::Taproot("taproot finalize failed".into()))
}

/// Compute depths for n leaves in a valid Taproot binary tree.
///
/// TaprootBuilder requires that leaves fill a complete binary tree.
/// Leaves are ordered by ascending timelock (index 0 = earliest timelock).
/// Earlier timelocks are more likely to be used, so they get shallower depth
/// (cheaper to spend on chain due to shorter merkle proof).
///
/// We build a left-leaning tree: the first leaf gets one side of the root,
/// the rest share the other side recursively.
///
/// Examples:
/// - 1 leaf:  [0]
/// - 2 leaves: [1, 1]
/// - 3 leaves: [1, 2, 2]
/// - 4 leaves: [1, 2, 3, 3]
/// - 5 leaves: [1, 2, 3, 4, 4]
fn compute_leaf_depths(n: usize) -> Vec<u8> {
    if n == 1 {
        return vec![0];
    }
    if n == 2 {
        return vec![1, 1];
    }

    // Left-leaning: first leaf at depth 1, remaining n-1 leaves form
    // a subtree on the right at depth 1. Recurse on the right subtree.
    let mut depths = vec![1u8]; // first leaf at depth 1
    let mut remaining = compute_leaf_depths(n - 1);
    // Shift remaining depths down by 1 (they're in a subtree at depth 1)
    for d in &mut remaining {
        *d += 1;
    }
    depths.extend(remaining);
    depths
}

/// Compile a recovery path (heir keys + timelock) directly to a Tapscript miniscript.
///
/// This avoids needing a dummy owner key — the owner's spending path is the
/// Taproot key-path (MuSig2), not a script leaf.
///
/// Produces: `and(heir_keys, older(timelock))`
fn compile_recovery_to_tapscript(
    heirs: &PathInfo,
    timelock: &Timelock,
) -> Result<Miniscript<DescriptorPublicKey, Tap>, InheritError> {
    use miniscript::policy::Concrete;
    use std::sync::Arc;

    let heir_policy = heirs.to_policy();
    let recovery_policy = Concrete::And(vec![
        Arc::new(heir_policy),
        Arc::new(Concrete::Older(miniscript::RelLockTime::from_height(
            timelock.blocks(),
        ))),
    ]);

    let ms: Miniscript<DescriptorPublicKey, Tap> = recovery_policy
        .compile()
        .map_err(|e| InheritError::Taproot(format!("tapscript compilation failed: {}", e)))?;

    Ok(ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{test_chain_code, test_keypair, test_xpub_str};
    use bitcoin::hashes::Hash as _; // needed for Txid::from_byte_array
    use bitcoin::secp256k1::Secp256k1;
    use miniscript::descriptor::DescriptorPublicKey;
    use nostring_ccd::register_cosigner_with_chain_code;
    use std::str::FromStr;

    fn single_heir_path() -> PathInfo {
        let heir_key = DescriptorPublicKey::from_str(&format!(
            "[00000002/86'/0'/0']{}/<0;1>/*",
            test_xpub_str()
        ))
        .unwrap();
        PathInfo::Single(heir_key)
    }

    fn multi_heir_path() -> PathInfo {
        let heir1 = DescriptorPublicKey::from_str(&format!(
            "[00000002/86'/0'/0']{}/<0;1>/*",
            test_xpub_str()
        ))
        .unwrap();
        let heir2 = DescriptorPublicKey::from_str(&format!(
            "[00000003/86'/0'/1']{}/<0;1>/*",
            test_xpub_str()
        ))
        .unwrap();
        let heir3 = DescriptorPublicKey::from_str(&format!(
            "[00000004/86'/0'/2']{}/<0;1>/*",
            test_xpub_str()
        ))
        .unwrap();
        PathInfo::multi(2, vec![heir1, heir2, heir3]).unwrap()
    }

    #[test]
    fn test_create_single_heir_vault() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        // Address should be P2TR
        assert!(vault.address.to_string().starts_with("tb1p"));

        // Should have one recovery script
        assert_eq!(vault.recovery_scripts.len(), 1);

        // Should have a merkle root (script tree exists)
        assert!(vault.taproot_spend_info.merkle_root().is_some());
    }

    #[test]
    fn test_create_multi_heir_vault() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            multi_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        assert!(vault.address.to_string().starts_with("tb1p"));
        assert_eq!(vault.recovery_scripts.len(), 1);

        // The recovery script should contain multi_a opcodes (OP_CHECKSIGADD)
        let script_asm = vault.recovery_scripts[0].1.to_asm_string();
        assert!(
            script_asm.contains("OP_CHECKSIGADD"),
            "multi-heir script should use OP_CHECKSIGADD: {}",
            script_asm
        );
    }

    #[test]
    fn test_create_cascade_vault() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let heir1 = DescriptorPublicKey::from_str(&format!(
            "[00000002/86'/0'/0']{}/<0;1>/*",
            test_xpub_str()
        ))
        .unwrap();
        let heir2 = DescriptorPublicKey::from_str(&format!(
            "[00000003/86'/0'/1']{}/<0;1>/*",
            test_xpub_str()
        ))
        .unwrap();

        let vault = create_cascade_vault(
            &owner_pk,
            &delegated,
            0,
            vec![
                (Timelock::six_months(), PathInfo::Single(heir1)),
                (Timelock::one_year(), PathInfo::Single(heir2)),
            ],
            0,
            Network::Testnet,
        )
        .unwrap();

        assert_eq!(
            vault.recovery_scripts.len(),
            2,
            "cascade should have 2 script leaves"
        );
        assert!(vault.address.to_string().starts_with("tb1p"));
    }

    #[test]
    fn test_inheritable_vault_deterministic() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let v1 = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();
        let v2 = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        assert_eq!(
            v1.address, v2.address,
            "same inputs must produce same address"
        );
    }

    #[test]
    fn test_different_timelock_different_address() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let v1 = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();
        let v2 = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::one_year(),
            0,
            Network::Testnet,
        )
        .unwrap();

        assert_ne!(
            v1.address, v2.address,
            "different timelocks must produce different addresses"
        );
    }

    #[test]
    fn test_differs_from_plain_ccd_vault() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let inheritable = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();
        let plain =
            nostring_ccd::vault::create_vault(&owner_pk, &delegated, 0, Network::Testnet).unwrap();

        assert_ne!(
            inheritable.address, plain.address,
            "inheritable vault must differ from plain CCD vault (script tree changes output key)"
        );
    }

    #[test]
    fn test_key_path_taptweak_is_sound() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        let secp = Secp256k1::new();
        let output_key = vault.taproot_spend_info.output_key();

        // Verify address matches output key
        let addr_from_output = Address::p2tr_tweaked(output_key, vault.network);
        assert_eq!(vault.address, addr_from_output);

        // Output key must differ from internal key (taptweak applied with merkle root)
        assert_ne!(
            vault.aggregate_xonly,
            output_key.to_x_only_public_key(),
            "output key must differ from internal key"
        );

        // Verify p2tr(internal, merkle_root) matches
        let addr_rebuilt = Address::p2tr(
            &secp,
            vault.aggregate_xonly,
            vault.taproot_spend_info.merkle_root(),
            vault.network,
        );
        assert_eq!(vault.address, addr_rebuilt);
    }

    #[test]
    fn test_heir_claim_psbt_single_utxo() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        let outpoint = bitcoin::OutPoint {
            txid: bitcoin::Txid::from_byte_array([0xAA; 32]),
            vout: 0,
        };
        let utxo_value = Amount::from_sat(100_000);
        let fee = Amount::from_sat(500);

        let secp = Secp256k1::new();
        let (_, dest_pk) = test_keypair(99);
        let dest_xonly = dest_pk.x_only_public_key().0;
        let destination = Address::p2tr(&secp, dest_xonly, None, Network::Testnet);

        let psbt = build_heir_claim_psbt(
            &vault,
            0,
            &[(
                outpoint,
                TxOut {
                    value: utxo_value,
                    script_pubkey: vault.address.script_pubkey(),
                },
            )],
            &destination,
            fee,
        )
        .unwrap();

        assert_eq!(psbt.unsigned_tx.input.len(), 1);
        assert_eq!(psbt.unsigned_tx.output.len(), 1);
        assert_eq!(psbt.unsigned_tx.output[0].value, utxo_value - fee);

        // CSV sequence set
        assert_eq!(
            psbt.unsigned_tx.input[0].sequence,
            Sequence::from_height(Timelock::six_months().blocks())
        );

        // Tap scripts populated
        assert!(!psbt.inputs[0].tap_scripts.is_empty());
        assert!(psbt.inputs[0].tap_internal_key.is_some());
        assert!(psbt.inputs[0].tap_merkle_root.is_some());
    }

    #[test]
    fn test_heir_claim_psbt_multiple_utxos() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        let utxos = vec![
            (
                bitcoin::OutPoint {
                    txid: bitcoin::Txid::from_byte_array([0xAA; 32]),
                    vout: 0,
                },
                TxOut {
                    value: Amount::from_sat(50_000),
                    script_pubkey: vault.address.script_pubkey(),
                },
            ),
            (
                bitcoin::OutPoint {
                    txid: bitcoin::Txid::from_byte_array([0xBB; 32]),
                    vout: 1,
                },
                TxOut {
                    value: Amount::from_sat(30_000),
                    script_pubkey: vault.address.script_pubkey(),
                },
            ),
        ];

        let secp = Secp256k1::new();
        let (_, dest_pk) = test_keypair(99);
        let destination =
            Address::p2tr(&secp, dest_pk.x_only_public_key().0, None, Network::Testnet);

        let psbt =
            build_heir_claim_psbt(&vault, 0, &utxos, &destination, Amount::from_sat(600)).unwrap();

        assert_eq!(psbt.unsigned_tx.input.len(), 2);
        assert_eq!(
            psbt.unsigned_tx.output[0].value.to_sat(),
            50_000 + 30_000 - 600
        );

        // Both inputs should have tap scripts
        for input in &psbt.inputs {
            assert!(!input.tap_scripts.is_empty());
            assert!(input.tap_internal_key.is_some());
        }
    }

    #[test]
    fn test_fee_exceeds_value_rejected() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        let secp = Secp256k1::new();
        let (_, dest_pk) = test_keypair(99);
        let destination =
            Address::p2tr(&secp, dest_pk.x_only_public_key().0, None, Network::Testnet);

        let result = build_heir_claim_psbt(
            &vault,
            0,
            &[(
                bitcoin::OutPoint {
                    txid: bitcoin::Txid::from_byte_array([0xAA; 32]),
                    vout: 0,
                },
                TxOut {
                    value: Amount::from_sat(500),
                    script_pubkey: vault.address.script_pubkey(),
                },
            )],
            &destination,
            Amount::from_sat(1000),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_dust_output_rejected() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        let secp = Secp256k1::new();
        let (_, dest_pk) = test_keypair(99);
        let destination =
            Address::p2tr(&secp, dest_pk.x_only_public_key().0, None, Network::Testnet);

        // 600 sat - 500 fee = 100 sat (below dust)
        let result = build_heir_claim_psbt(
            &vault,
            0,
            &[(
                bitcoin::OutPoint {
                    txid: bitcoin::Txid::from_byte_array([0xAA; 32]),
                    vout: 0,
                },
                TxOut {
                    value: Amount::from_sat(600),
                    script_pubkey: vault.address.script_pubkey(),
                },
            )],
            &destination,
            Amount::from_sat(500),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_no_utxos_rejected() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        let secp = Secp256k1::new();
        let (_, dest_pk) = test_keypair(99);
        let destination =
            Address::p2tr(&secp, dest_pk.x_only_public_key().0, None, Network::Testnet);

        let result = build_heir_claim_psbt(&vault, 0, &[], &destination, Amount::from_sat(500));
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_recovery_index_rejected() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        let secp = Secp256k1::new();
        let (_, dest_pk) = test_keypair(99);
        let destination =
            Address::p2tr(&secp, dest_pk.x_only_public_key().0, None, Network::Testnet);

        // Recovery index 1 doesn't exist for single-heir vault
        let result = build_heir_claim_psbt(
            &vault,
            1,
            &[(
                bitcoin::OutPoint {
                    txid: bitcoin::Txid::from_byte_array([0xAA; 32]),
                    vout: 0,
                },
                TxOut {
                    value: Amount::from_sat(10_000),
                    script_pubkey: vault.address.script_pubkey(),
                },
            )],
            &destination,
            Amount::from_sat(500),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_recovery_script_contains_csv() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            single_heir_path(),
            Timelock::six_months(),
            0,
            Network::Testnet,
        )
        .unwrap();

        let asm = vault.recovery_scripts[0].1.to_asm_string();
        assert!(
            asm.contains("OP_CHECKSIGVERIFY") || asm.contains("OP_CHECKSIG"),
            "recovery script must verify heir's signature: {}",
            asm
        );
        assert!(
            asm.contains("OP_CSV") || asm.contains("OP_CHECKSEQUENCEVERIFY"),
            "recovery script must contain CSV: {}",
            asm
        );
    }

    #[test]
    fn test_three_leaf_cascade() {
        // 3 leaves is not a power of 2 — tests the tree builder handles it
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let heir1 = DescriptorPublicKey::from_str(&format!(
            "[00000002/86'/0'/0']{}/<0;1>/*",
            test_xpub_str()
        ))
        .unwrap();
        let heir2 = DescriptorPublicKey::from_str(&format!(
            "[00000003/86'/0'/1']{}/<0;1>/*",
            test_xpub_str()
        ))
        .unwrap();
        let heir3 = DescriptorPublicKey::from_str(&format!(
            "[00000004/86'/0'/2']{}/<0;1>/*",
            test_xpub_str()
        ))
        .unwrap();

        let vault = create_cascade_vault(
            &owner_pk,
            &delegated,
            0,
            vec![
                (Timelock::six_months(), PathInfo::Single(heir1)),
                (Timelock::days(270).unwrap(), PathInfo::Single(heir2)),
                (Timelock::one_year(), PathInfo::Single(heir3)),
            ],
            0,
            Network::Testnet,
        )
        .unwrap();

        assert_eq!(vault.recovery_scripts.len(), 3);
        assert!(vault.address.to_string().starts_with("tb1p"));
    }

    #[test]
    fn test_signing_produces_valid_witness() {
        // End-to-end: create vault, build claim PSBT, sign it, verify witness structure.
        // Uses a concrete x-only key (not xpub) so we can actually sign.
        use bitcoin::secp256k1::Keypair;
        use bitcoin::sighash::{Prevouts, SighashCache};
        use bitcoin::taproot::{LeafVersion, Signature, TapLeafHash};

        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let (heir_sk, heir_pk) = test_keypair(3);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        // Use a concrete single-key DescriptorPublicKey (no xpub/wildcard)
        // so the compiled script contains a real x-only key we can sign with.
        let heir_xonly = heir_pk.x_only_public_key().0;
        let heir_desc_key = DescriptorPublicKey::from_str(&format!("{}", heir_xonly)).unwrap();

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            PathInfo::Single(heir_desc_key),
            Timelock::from_blocks(144).unwrap(),
            0, // derivation_index doesn't matter for concrete keys
            Network::Testnet,
        )
        .unwrap();

        // Build claim PSBT
        let outpoint = bitcoin::OutPoint {
            txid: bitcoin::Txid::from_byte_array([0xAA; 32]),
            vout: 0,
        };
        let utxo_value = Amount::from_sat(50_000);
        let fee = Amount::from_sat(300);

        let secp = Secp256k1::new();
        let destination = Address::p2tr(&secp, heir_xonly, None, Network::Testnet);

        let utxo_txout = TxOut {
            value: utxo_value,
            script_pubkey: vault.address.script_pubkey(),
        };

        let psbt = build_heir_claim_psbt(
            &vault,
            0,
            &[(outpoint, utxo_txout.clone())],
            &destination,
            fee,
        )
        .unwrap();

        // Sign the PSBT manually (simulating what a hardware wallet does)
        let recovery_script = &vault.recovery_scripts[0].1;

        let leaf_hash = TapLeafHash::from_script(recovery_script, LeafVersion::TapScript);

        let mut sighash_cache = SighashCache::new(&psbt.unsigned_tx);
        let prevouts = Prevouts::All(&[utxo_txout]);

        let sighash = sighash_cache
            .taproot_script_spend_signature_hash(
                0,
                &prevouts,
                leaf_hash,
                bitcoin::TapSighashType::Default,
            )
            .expect("sighash computation should succeed");

        let msg = bitcoin::secp256k1::Message::from_digest(*sighash.as_byte_array());
        let keypair = Keypair::from_secret_key(&secp, &heir_sk);
        let schnorr_sig = secp.sign_schnorr(&msg, &keypair);

        // Verify the signature
        assert!(
            secp.verify_schnorr(&schnorr_sig, &msg, &heir_xonly).is_ok(),
            "Schnorr signature should verify"
        );

        // Build the witness: [signature, script, control_block]
        let control_block = vault
            .taproot_spend_info
            .control_block(&(recovery_script.clone(), LeafVersion::TapScript))
            .expect("control block should exist");

        let tap_sig = Signature {
            signature: schnorr_sig,
            sighash_type: bitcoin::TapSighashType::Default,
        };

        let mut tx = psbt.unsigned_tx.clone();
        tx.input[0].witness.push(tap_sig.to_vec());
        tx.input[0].witness.push(recovery_script.as_bytes());
        tx.input[0].witness.push(control_block.serialize());

        // Verify witness structure
        assert_eq!(tx.input[0].witness.len(), 3, "witness should have 3 items");

        // Signature: 64 bytes for Default sighash type
        let sig_len = tx.input[0].witness[0].len();
        assert!(
            sig_len == 64 || sig_len == 65,
            "sig should be 64 or 65 bytes, got {}",
            sig_len
        );

        // Script matches
        assert_eq!(&tx.input[0].witness[1][..], recovery_script.as_bytes());

        // Control block: 33 bytes minimum (version + internal key) + 32*depth merkle path
        assert!(
            tx.input[0].witness[2].len() >= 33,
            "control block too short: {}",
            tx.input[0].witness[2].len()
        );

        // Verify the control block validates against the output key
        assert!(
            control_block.verify_taproot_commitment(
                &secp,
                vault.taproot_spend_info.output_key().to_x_only_public_key(),
                recovery_script,
            ),
            "control block should verify against output key"
        );
    }

    #[test]
    fn test_consensus_script_verification() {
        // Full consensus verification: create a spending transaction and verify
        // it against libbitcoinconsensus (Bitcoin Core's script interpreter).
        // This proves the Taproot script-path witness is valid Bitcoin.
        use bitcoin::consensus::Encodable;
        use bitcoin::secp256k1::Keypair;
        use bitcoin::sighash::{Prevouts, SighashCache};
        use bitcoin::taproot::{LeafVersion, Signature, TapLeafHash};

        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let (heir_sk, heir_pk) = test_keypair(3);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let heir_xonly = heir_pk.x_only_public_key().0;
        let heir_desc_key = DescriptorPublicKey::from_str(&format!("{}", heir_xonly)).unwrap();

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            PathInfo::Single(heir_desc_key),
            Timelock::from_blocks(1).unwrap(), // 1 block for testability
            0,
            Network::Testnet,
        )
        .unwrap();

        // Create a fake funding UTXO
        let utxo_value = Amount::from_sat(50_000);
        let utxo_txout = TxOut {
            value: utxo_value,
            script_pubkey: vault.address.script_pubkey(),
        };

        let outpoint = bitcoin::OutPoint {
            txid: bitcoin::Txid::from_byte_array([0xCC; 32]),
            vout: 0,
        };

        let secp = Secp256k1::new();
        let destination = Address::p2tr(&secp, heir_xonly, None, Network::Testnet);

        let psbt = build_heir_claim_psbt(
            &vault,
            0,
            &[(outpoint, utxo_txout.clone())],
            &destination,
            Amount::from_sat(300),
        )
        .unwrap();

        // Sign
        let recovery_script = &vault.recovery_scripts[0].1;
        let leaf_hash = TapLeafHash::from_script(recovery_script, LeafVersion::TapScript);
        let mut sighash_cache = SighashCache::new(&psbt.unsigned_tx);
        let prevouts = Prevouts::All(&[utxo_txout.clone()]);
        let sighash = sighash_cache
            .taproot_script_spend_signature_hash(
                0,
                &prevouts,
                leaf_hash,
                bitcoin::TapSighashType::Default,
            )
            .unwrap();
        let msg = bitcoin::secp256k1::Message::from_digest(*sighash.as_byte_array());
        let keypair = Keypair::from_secret_key(&secp, &heir_sk);
        let schnorr_sig = secp.sign_schnorr(&msg, &keypair);

        let control_block = vault
            .taproot_spend_info
            .control_block(&(recovery_script.clone(), LeafVersion::TapScript))
            .unwrap();

        let tap_sig = Signature {
            signature: schnorr_sig,
            sighash_type: bitcoin::TapSighashType::Default,
        };

        let mut tx = psbt.unsigned_tx.clone();
        tx.input[0].witness.push(tap_sig.to_vec());
        tx.input[0].witness.push(recovery_script.as_bytes());
        tx.input[0].witness.push(control_block.serialize());

        // Serialize the spending transaction
        let mut tx_bytes = Vec::new();
        tx.consensus_encode(&mut tx_bytes).unwrap();

        // Build spent_outputs for Taproot verification
        let script_bytes = utxo_txout.script_pubkey.as_bytes();
        let spent_utxo = bitcoinconsensus::Utxo {
            script_pubkey: script_bytes.as_ptr(),
            script_pubkey_len: script_bytes.len() as u32,
            value: utxo_txout.value.to_sat() as i64,
        };

        // Verify against Bitcoin Core's consensus rules
        let result = bitcoinconsensus::verify(
            utxo_txout.script_pubkey.as_bytes(),
            utxo_value.to_sat(),
            &tx_bytes,
            Some(&[spent_utxo]),
            0,
        );

        assert!(
            result.is_ok(),
            "consensus verification failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_consensus_multi_heir_verification() {
        // Full consensus verification with 2-of-3 multi-heir threshold.
        // Three heirs, two must sign. Proves multi_a (OP_CHECKSIGADD) works.
        use bitcoin::consensus::Encodable;
        use bitcoin::secp256k1::Keypair;
        use bitcoin::sighash::{Prevouts, SighashCache};
        use bitcoin::taproot::{LeafVersion, Signature, TapLeafHash};

        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let (heir1_sk, heir1_pk) = test_keypair(10);
        let (heir2_sk, heir2_pk) = test_keypair(11);
        let (_heir3_sk, heir3_pk) = test_keypair(12);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        // Build 2-of-3 heir vault using concrete x-only keys
        let h1_xonly = heir1_pk.x_only_public_key().0;
        let h2_xonly = heir2_pk.x_only_public_key().0;
        let h3_xonly = heir3_pk.x_only_public_key().0;

        let h1_desc = DescriptorPublicKey::from_str(&format!("{}", h1_xonly)).unwrap();
        let h2_desc = DescriptorPublicKey::from_str(&format!("{}", h2_xonly)).unwrap();
        let h3_desc = DescriptorPublicKey::from_str(&format!("{}", h3_xonly)).unwrap();

        let heirs = PathInfo::multi(2, vec![h1_desc, h2_desc, h3_desc]).unwrap();

        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            heirs,
            Timelock::from_blocks(1).unwrap(),
            0,
            Network::Testnet,
        )
        .unwrap();

        let utxo_value = Amount::from_sat(50_000);
        let utxo_txout = TxOut {
            value: utxo_value,
            script_pubkey: vault.address.script_pubkey(),
        };
        let outpoint = bitcoin::OutPoint {
            txid: bitcoin::Txid::from_byte_array([0xDD; 32]),
            vout: 0,
        };

        let secp = Secp256k1::new();
        let destination = Address::p2tr(&secp, h1_xonly, None, Network::Testnet);

        let psbt = build_heir_claim_psbt(
            &vault,
            0,
            &[(outpoint, utxo_txout.clone())],
            &destination,
            Amount::from_sat(300),
        )
        .unwrap();

        // Sign with heirs 1 and 2 (threshold = 2)
        let recovery_script = &vault.recovery_scripts[0].1;
        let leaf_hash = TapLeafHash::from_script(recovery_script, LeafVersion::TapScript);
        let mut sighash_cache = SighashCache::new(&psbt.unsigned_tx);
        let prevouts = Prevouts::All(&[utxo_txout.clone()]);
        let sighash = sighash_cache
            .taproot_script_spend_signature_hash(
                0,
                &prevouts,
                leaf_hash,
                bitcoin::TapSighashType::Default,
            )
            .unwrap();
        let msg = bitcoin::secp256k1::Message::from_digest(*sighash.as_byte_array());

        let sig1 = secp.sign_schnorr(&msg, &Keypair::from_secret_key(&secp, &heir1_sk));
        let sig2 = secp.sign_schnorr(&msg, &Keypair::from_secret_key(&secp, &heir2_sk));

        let tap_sig1 = Signature {
            signature: sig1,
            sighash_type: bitcoin::TapSighashType::Default,
        };
        let tap_sig2 = Signature {
            signature: sig2,
            sighash_type: bitcoin::TapSighashType::Default,
        };

        // Build witness for multi_a(2, key1, key2, key3):
        // Witness stack (bottom to top): [sig_for_key3_or_empty, sig_for_key2, sig_for_key1, script, control_block]
        // For multi_a, keys are checked in order. Provide sigs for keys that sign,
        // empty byte vec for keys that don't.
        // Key order in script: h1, h2, h3. We sign with h1 and h2, skip h3.
        let control_block = vault
            .taproot_spend_info
            .control_block(&(recovery_script.clone(), LeafVersion::TapScript))
            .unwrap();

        let mut tx = psbt.unsigned_tx.clone();
        // multi_a witness: push sigs in REVERSE key order, empty for non-signers
        tx.input[0].witness.push(&[] as &[u8]); // heir3 did not sign
        tx.input[0].witness.push(tap_sig2.to_vec()); // heir2 signed
        tx.input[0].witness.push(tap_sig1.to_vec()); // heir1 signed
        tx.input[0].witness.push(recovery_script.as_bytes());
        tx.input[0].witness.push(control_block.serialize());

        // Serialize and verify against Bitcoin Core consensus
        let mut tx_bytes = Vec::new();
        tx.consensus_encode(&mut tx_bytes).unwrap();

        let script_bytes = utxo_txout.script_pubkey.as_bytes();
        let spent_utxo = bitcoinconsensus::Utxo {
            script_pubkey: script_bytes.as_ptr(),
            script_pubkey_len: script_bytes.len() as u32,
            value: utxo_txout.value.to_sat() as i64,
        };

        let result = bitcoinconsensus::verify(
            utxo_txout.script_pubkey.as_bytes(),
            utxo_value.to_sat(),
            &tx_bytes,
            Some(&[spent_utxo]),
            0,
        );

        assert!(
            result.is_ok(),
            "2-of-3 multi-heir consensus verification failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_leaf_depths_left_leaning() {
        // Verify earlier leaves get shallower depth
        let depths_3 = super::compute_leaf_depths(3);
        assert_eq!(depths_3, vec![1, 2, 2], "3 leaves: {:?}", depths_3);

        let depths_4 = super::compute_leaf_depths(4);
        assert_eq!(depths_4, vec![1, 2, 3, 3], "4 leaves: {:?}", depths_4);

        // First leaf always at depth 1 (for n >= 2)
        for n in 2..=8 {
            let depths = super::compute_leaf_depths(n);
            assert_eq!(depths[0], 1, "first leaf should be at depth 1 for n={}", n);
            assert_eq!(depths.len(), n);
        }
    }

    #[test]
    fn test_builder_with_heir_keys() {
        use crate::heir::HeirKey;
        use bitcoin::bip32::{Fingerprint, Xpub};

        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let xpub = Xpub::from_str(test_xpub_str()).unwrap();
        let heir = HeirKey::new(
            "Alice",
            Fingerprint::from_str("00000002").unwrap(),
            xpub,
            None,
        );

        let vault = InheritableVaultBuilder::new(owner_pk, delegated, Network::Testnet)
            .heir(heir)
            .timelock(Timelock::six_months())
            .build()
            .unwrap();

        assert!(vault.address.to_string().starts_with("tb1p"));
        assert_eq!(vault.recovery_scripts.len(), 1);
    }

    #[test]
    fn test_builder_multi_heir_with_threshold() {
        use crate::heir::HeirKey;
        use bitcoin::bip32::{Fingerprint, Xpub};

        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let xpub = Xpub::from_str(test_xpub_str()).unwrap();
        let alice = HeirKey::new(
            "Alice",
            Fingerprint::from_str("00000002").unwrap(),
            xpub,
            None,
        );
        let bob = HeirKey::new(
            "Bob",
            Fingerprint::from_str("00000003").unwrap(),
            xpub,
            None,
        );
        let carol = HeirKey::new(
            "Carol",
            Fingerprint::from_str("00000004").unwrap(),
            xpub,
            None,
        );

        let vault = InheritableVaultBuilder::new(owner_pk, delegated, Network::Testnet)
            .heir(alice)
            .heir(bob)
            .heir(carol)
            .threshold(2)
            .timelock(Timelock::six_months())
            .build()
            .unwrap();

        assert!(vault.address.to_string().starts_with("tb1p"));
    }

    #[test]
    fn test_builder_missing_timelock_rejected() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let result = InheritableVaultBuilder::new(owner_pk, delegated, Network::Testnet).build();

        assert!(result.is_err(), "should fail with no heirs");
    }

    #[test]
    fn test_builder_missing_heirs_rejected() {
        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let result = InheritableVaultBuilder::new(owner_pk, delegated, Network::Testnet)
            .timelock(Timelock::six_months())
            .build();

        assert!(result.is_err(), "should fail with no heirs");
    }

    #[test]
    fn test_builder_threshold_defaults_to_all() {
        use crate::heir::HeirKey;
        use bitcoin::bip32::{Fingerprint, Xpub};

        let (_owner_sk, owner_pk) = test_keypair(1);
        let (_cosigner_sk, cosigner_pk) = test_keypair(2);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, test_chain_code(), "test");

        let xpub = Xpub::from_str(test_xpub_str()).unwrap();
        let alice = HeirKey::new(
            "Alice",
            Fingerprint::from_str("00000002").unwrap(),
            xpub,
            None,
        );
        let bob = HeirKey::new(
            "Bob",
            Fingerprint::from_str("00000003").unwrap(),
            xpub,
            None,
        );

        // No .threshold() call — should default to requiring all heirs (2-of-2)
        let vault = InheritableVaultBuilder::new(owner_pk, delegated.clone(), Network::Testnet)
            .heir(alice.clone())
            .heir(bob.clone())
            .timelock(Timelock::six_months())
            .build()
            .unwrap();

        // With explicit threshold(2) should produce the same address
        let vault2 = InheritableVaultBuilder::new(owner_pk, delegated, Network::Testnet)
            .heir(alice)
            .heir(bob)
            .threshold(2)
            .timelock(Timelock::six_months())
            .build()
            .unwrap();

        assert_eq!(
            vault.address, vault2.address,
            "default threshold should equal explicit all-required"
        );
    }

    #[test]
    fn test_estimate_vbytes() {
        let vbytes_single = estimate_heir_claim_vbytes(1, 1, 0);
        assert!(
            vbytes_single > 100 && vbytes_single < 250,
            "1-in-1-out: {}",
            vbytes_single
        );

        let vbytes_deep = estimate_heir_claim_vbytes(1, 1, 3);
        assert!(vbytes_deep > vbytes_single, "deeper tree should be heavier");
    }
}
