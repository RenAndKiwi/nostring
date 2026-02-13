//! Heir claim Tauri commands.
//!
//! These commands run on the HEIR's device, not the owner's.
//! The heir's app is keyless — it builds unsigned PSBTs and the
//! heir signs on their hardware wallet or signing device.
//!
//! Flow:
//! 1. import_vault_backup — import descriptor backup (from Nostr DM or manual paste)
//! 2. verify_heir_identity — prove this heir's xpub is in the vault
//! 3. check_claim_eligibility — is the timelock expired?
//! 4. build_heir_claim — paste destination address, get unsigned PSBT
//! 5. broadcast_heir_claim — submit signed PSBT, broadcast to network

use crate::db;
use crate::state::{AppState, CcdState};
use bitcoin::address::NetworkChecked;
use bitcoin::{Address, Amount, Network};
use nostring_ccd::register_cosigner_with_chain_code;
use nostring_ccd::types::ChainCode;
use nostring_inherit::heir::HeirKey;
use nostring_inherit::policy::Timelock;
use nostring_inherit::taproot::{
    build_heir_claim_psbt, create_inheritable_vault, estimate_heir_claim_vbytes,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tauri::State;

use crate::ccd_commands::CcdResult;

// ============================================================================
// Descriptor backup format (shared with heir app via nostring-inherit)
// ============================================================================

pub use nostring_inherit::backup::{extract_recovery_leaves, HeirBackupEntry, VaultBackup};

// ============================================================================
// Response types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimStatus {
    /// Can the heir claim now?
    pub eligible: bool,
    /// Blocks until timelock expires (negative = overdue, claimable)
    pub blocks_remaining: i64,
    /// Approximate days until eligible
    pub days_remaining: f64,
    /// Vault balance in satoshis
    pub vault_balance_sat: u64,
    /// Number of UTXOs in the vault
    pub utxo_count: usize,
    /// Heir's role in the quorum
    pub quorum_info: QuorumInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumInfo {
    /// This heir's label
    pub my_label: String,
    /// This heir's recovery script index
    pub my_recovery_index: usize,
    /// Total heirs in this recovery path
    pub total_heirs: usize,
    /// Threshold required to spend
    pub threshold: usize,
    /// Labels of all heirs in the quorum
    pub all_heirs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeirIdentityResult {
    /// Whether the provided xpub matches an heir in the vault
    pub verified: bool,
    /// The matched heir's label
    pub label: Option<String>,
    /// The recovery index for this heir
    pub recovery_index: Option<usize>,
    /// Quorum info if verified
    pub quorum: Option<QuorumInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimPsbtResult {
    /// Base64-encoded unsigned PSBT
    pub psbt_base64: String,
    /// Destination address
    pub destination: String,
    /// Amount being sent (sats)
    pub send_amount_sat: u64,
    /// Fee (sats)
    pub fee_sat: u64,
    /// Fee rate (sat/vB)
    pub fee_rate: f64,
}

// ============================================================================
// Commands
// ============================================================================

/// Import a vault descriptor backup.
///
/// The heir pastes the JSON backup received from the owner (via NIP-17 DM,
/// physical letter, or estate attorney). The app reconstructs the vault
/// and verifies the address matches.
#[tauri::command]
pub async fn import_vault_backup(
    backup_json: String,
    state: State<'_, AppState>,
) -> Result<CcdResult<String>, ()> {
    let backup: VaultBackup = match serde_json::from_str(&backup_json) {
        Ok(b) => b,
        Err(e) => return Ok(CcdResult::err(format!("Invalid backup format: {}", e))),
    };

    if backup.version != 1 {
        return Ok(CcdResult::err(format!(
            "Unsupported backup version {}. This app supports version 1.",
            backup.version
        )));
    }

    // Parse network
    let network = match backup.network.as_str() {
        "bitcoin" | "mainnet" => Network::Bitcoin,
        "testnet" | "testnet3" => Network::Testnet,
        "signet" => Network::Signet,
        "regtest" => Network::Regtest,
        other => return Ok(CcdResult::err(format!("Unknown network: {}", other))),
    };

    // Parse owner pubkey
    let owner_pubkey = match bitcoin::secp256k1::PublicKey::from_str(&backup.owner_pubkey) {
        Ok(pk) => pk,
        Err(e) => return Ok(CcdResult::err(format!("Invalid owner public key: {}", e))),
    };

    // Parse cosigner pubkey + chain code → DelegatedKey
    let cosigner_pubkey = match bitcoin::secp256k1::PublicKey::from_str(&backup.cosigner_pubkey) {
        Ok(pk) => pk,
        Err(e) => {
            return Ok(CcdResult::err(format!(
                "Invalid cosigner public key: {}",
                e
            )))
        }
    };

    let cc_bytes = match hex::decode(&backup.chain_code) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        Ok(b) => {
            return Ok(CcdResult::err(format!(
                "Chain code must be 32 bytes, got {}",
                b.len()
            )))
        }
        Err(e) => return Ok(CcdResult::err(format!("Invalid chain code: {}", e))),
    };

    let delegated =
        register_cosigner_with_chain_code(cosigner_pubkey, ChainCode(cc_bytes), "cosigner");

    // Parse heirs
    let heir_keys: Vec<HeirKey> = {
        let mut keys = Vec::new();
        for entry in &backup.heirs {
            let xpub = match bitcoin::bip32::Xpub::from_str(&entry.xpub) {
                Ok(x) => x,
                Err(e) => {
                    return Ok(CcdResult::err(format!(
                        "Invalid heir xpub for '{}': {}",
                        entry.label, e
                    )))
                }
            };
            let fp = xpub.fingerprint();
            let derivation = bitcoin::bip32::DerivationPath::from_str(&entry.derivation_path)
                .unwrap_or_else(|_| {
                    bitcoin::bip32::DerivationPath::from_str("m/84'/0'/0'").unwrap()
                });
            let mut heir = HeirKey::new(&entry.label, fp, xpub, Some(derivation));
            heir.npub = entry.npub.clone();
            keys.push(heir);
        }
        keys
    };

    if heir_keys.is_empty() {
        return Ok(CcdResult::err("Backup contains no heirs"));
    }

    // Build path info from heirs
    let path_info = match CcdState::heirs_to_path_info(&heir_keys) {
        Some(p) => p,
        None => return Ok(CcdResult::err("Failed to convert heir keys")),
    };

    let timelock = match Timelock::from_blocks(backup.timelock_blocks) {
        Ok(t) => t,
        Err(e) => return Ok(CcdResult::err(format!("Invalid timelock: {}", e))),
    };

    // Reconstruct the vault
    let vault = match create_inheritable_vault(
        &owner_pubkey,
        &delegated,
        backup.address_index,
        path_info,
        timelock,
        0,
        network,
    ) {
        Ok(v) => v,
        Err(e) => {
            return Ok(CcdResult::err(format!(
                "Failed to reconstruct vault: {}",
                e
            )))
        }
    };

    // Verify reconstructed address matches backup
    let reconstructed_addr = vault.address.to_string();
    if reconstructed_addr != backup.vault_address {
        return Ok(CcdResult::err(format!(
            "Address mismatch! Backup says {} but reconstruction gives {}. \
             The backup may be corrupted or for a different vault.",
            backup.vault_address, reconstructed_addr
        )));
    }

    // Persist the backup and vault
    {
        let conn = state.db.lock().unwrap();
        let _ = db::config_set(&conn, "heir_vault_backup", &backup_json);
        let _ = db::config_set(&conn, "heir_vault_network", &backup.network);
    }

    // Store heirs in the registry (for quorum tracking)
    {
        let mut registry = state.heir_registry.lock().unwrap();
        for heir in &heir_keys {
            registry.add(heir.clone());
        }
    }

    // Store vault in CCD state
    {
        let mut ccd = state.ccd.lock().unwrap();
        ccd.cosigner = Some(delegated);
        ccd.vault = Some(vault);
    }

    // Store network
    state.set_network(network);

    log::info!(
        "Heir vault imported: {} ({} heirs, timelock {} blocks)",
        reconstructed_addr,
        heir_keys.len(),
        backup.timelock_blocks
    );

    Ok(CcdResult::ok(reconstructed_addr))
}

/// Verify the heir's identity against the imported vault.
///
/// The heir provides their xpub and/or npub. The app checks if either matches
/// an heir in the vault and returns their role in the quorum.
///
/// Verification paths:
/// - **xpub match**: proves the heir controls the Bitcoin signing key
/// - **npub match**: proves the heir controls the Nostr identity (delivery channel)
/// - **both**: strongest verification
#[tauri::command]
pub async fn verify_heir_identity(
    heir_xpub: Option<String>,
    heir_npub: Option<String>,
    state: State<'_, AppState>,
) -> Result<CcdResult<HeirIdentityResult>, ()> {
    // Check vault exists
    {
        let ccd = state.ccd.lock().unwrap();
        if ccd.vault.is_none() {
            return Ok(CcdResult::err(
                "No vault imported. Import a vault backup first.",
            ));
        }
    }

    if heir_xpub.is_none() && heir_npub.is_none() {
        return Ok(CcdResult::err(
            "Provide at least one of: xpub (Bitcoin key) or npub (Nostr identity)",
        ));
    }

    let parsed_xpub = match &heir_xpub {
        Some(x) => match bitcoin::bip32::Xpub::from_str(x) {
            Ok(xp) => Some(xp),
            Err(e) => return Ok(CcdResult::err(format!("Invalid xpub: {}", e))),
        },
        None => None,
    };

    // Normalize npub (strip "npub1" prefix if bech32, or accept hex)
    let normalized_npub = heir_npub.as_deref().map(|s| s.trim().to_string());

    // Load the backup to get heir entries with recovery indices
    let backup: VaultBackup = {
        let conn = state.db.lock().unwrap();
        let backup_json = match db::config_get(&conn, "heir_vault_backup") {
            Ok(Some(s)) => s,
            _ => return Ok(CcdResult::err("No vault backup found in database")),
        };
        match serde_json::from_str(&backup_json) {
            Ok(b) => b,
            Err(e) => return Ok(CcdResult::err(format!("Corrupted backup data: {}", e))),
        }
    };

    // Find matching heir by xpub and/or npub
    let matched = backup.heirs.iter().find(|h| {
        // Match by xpub (fingerprint or public key comparison)
        let xpub_match = parsed_xpub.as_ref().is_some_and(|xpub| {
            let fp_match = h.fingerprint == xpub.fingerprint().to_string();
            let pk_match = bitcoin::bip32::Xpub::from_str(&h.xpub)
                .map(|x| x.public_key == xpub.public_key)
                .unwrap_or(false);
            fp_match || pk_match
        });

        // Match by npub
        let npub_match = normalized_npub
            .as_ref()
            .is_some_and(|npub| h.npub.as_ref().is_some_and(|stored| stored == npub));

        xpub_match || npub_match
    });

    match matched {
        Some(entry) => {
            let all_labels: Vec<String> = backup.heirs.iter().map(|h| h.label.clone()).collect();
            let threshold = backup.threshold;

            Ok(CcdResult::ok(HeirIdentityResult {
                verified: true,
                label: Some(entry.label.clone()),
                recovery_index: Some(entry.recovery_index),
                quorum: Some(QuorumInfo {
                    my_label: entry.label.clone(),
                    my_recovery_index: entry.recovery_index,
                    total_heirs: backup.heirs.len(),
                    threshold,
                    all_heirs: all_labels,
                }),
            }))
        }
        None => Ok(CcdResult::ok(HeirIdentityResult {
            verified: false,
            label: None,
            recovery_index: None,
            quorum: None,
        })),
    }
}

/// Check if the vault's timelock has expired and the heir can claim.
#[tauri::command]
pub async fn check_claim_eligibility(
    state: State<'_, AppState>,
) -> Result<CcdResult<ClaimStatus>, ()> {
    let vault = {
        let ccd = state.ccd.lock().unwrap();
        match &ccd.vault {
            Some(v) => v.clone(),
            None => return Ok(CcdResult::err("No vault imported")),
        }
    };

    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    let client = match nostring_electrum::ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => return Ok(CcdResult::err(format!("Electrum connection failed: {}", e))),
    };

    let current_height = match client.get_height() {
        Ok(h) => h,
        Err(e) => return Ok(CcdResult::err(format!("Failed to get block height: {}", e))),
    };

    let script = vault.address.script_pubkey();
    let utxos = match client.get_utxos_for_script(&script) {
        Ok(u) => u,
        Err(e) => return Ok(CcdResult::err(format!("Failed to get UTXOs: {}", e))),
    };

    if utxos.is_empty() {
        return Ok(CcdResult::err("No funds found in vault"));
    }

    let total_balance: u64 = utxos.iter().map(|u| u.value.to_sat()).sum();

    // Use the OLDEST UTXO's height — that's when the timelock started
    let oldest_height = utxos.iter().map(|u| u.height).min().unwrap_or(0);
    if oldest_height == 0 {
        return Ok(CcdResult::err(
            "Vault has unconfirmed UTXOs. Wait for confirmation.",
        ));
    }

    let timelock_blocks = vault.timelock.blocks() as u64;
    let expiry = oldest_height as u64 + timelock_blocks;
    let remaining = expiry as i64 - current_height as i64;
    let eligible = remaining <= 0;
    let days = remaining as f64 * 10.0 / 1440.0;

    // Load backup for quorum info
    let quorum = {
        let conn = state.db.lock().unwrap();
        let backup_json = db::config_get(&conn, "heir_vault_backup").ok().flatten();
        backup_json
            .and_then(|j| serde_json::from_str::<VaultBackup>(&j).ok())
            .map(|b| {
                let all_labels: Vec<String> = b.heirs.iter().map(|h| h.label.clone()).collect();
                let threshold = b.threshold;
                QuorumInfo {
                    my_label: "Unknown".into(), // Set after verify_heir_identity
                    my_recovery_index: 0,
                    total_heirs: b.heirs.len(),
                    threshold,
                    all_heirs: all_labels,
                }
            })
            .unwrap_or(QuorumInfo {
                my_label: "Unknown".into(),
                my_recovery_index: 0,
                total_heirs: 1,
                threshold: 1,
                all_heirs: vec![],
            })
    };

    Ok(CcdResult::ok(ClaimStatus {
        eligible,
        blocks_remaining: remaining,
        days_remaining: days,
        vault_balance_sat: total_balance,
        utxo_count: utxos.len(),
        quorum_info: quorum,
    }))
}

/// Build an unsigned heir claim PSBT.
///
/// The heir provides their destination address — any valid Bitcoin address.
/// The app builds a script-path spend PSBT. The heir signs it on their
/// hardware wallet or signing device. The app NEVER touches the heir's keys.
#[tauri::command]
pub async fn build_heir_claim(
    destination_address: String,
    recovery_index: usize,
    state: State<'_, AppState>,
) -> Result<CcdResult<ClaimPsbtResult>, ()> {
    let vault = {
        let ccd = state.ccd.lock().unwrap();
        match &ccd.vault {
            Some(v) => v.clone(),
            None => return Ok(CcdResult::err("No vault imported")),
        }
    };

    let network = *state.network.lock().unwrap();

    // Parse and validate destination address
    let destination: Address<NetworkChecked> = match Address::from_str(&destination_address) {
        Ok(addr) => match addr.require_network(network) {
            Ok(a) => a,
            Err(_) => {
                return Ok(CcdResult::err(format!(
                    "Address is for the wrong network. Expected {:?}.",
                    network
                )))
            }
        },
        Err(e) => return Ok(CcdResult::err(format!("Invalid Bitcoin address: {}", e))),
    };

    let electrum_url = state.electrum_url.lock().unwrap().clone();

    let client = match nostring_electrum::ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => return Ok(CcdResult::err(format!("Electrum connection failed: {}", e))),
    };

    // Estimate fee
    let fee_rate = client
        .estimate_fee_rate(6)
        .unwrap_or(10.0)
        .clamp(1.0, 500.0);

    // Get vault UTXOs
    let script = vault.address.script_pubkey();
    let utxos = match client.get_utxos_for_script(&script) {
        Ok(u) => u,
        Err(e) => return Ok(CcdResult::err(format!("Failed to get UTXOs: {}", e))),
    };

    if utxos.is_empty() {
        return Ok(CcdResult::err("No funds in vault"));
    }

    // Build (outpoint, txout) pairs
    let utxo_pairs: Vec<(bitcoin::OutPoint, bitcoin::TxOut)> = utxos
        .iter()
        .map(|u| {
            (
                u.outpoint,
                bitcoin::TxOut {
                    value: u.value,
                    script_pubkey: script.clone(),
                },
            )
        })
        .collect();

    let total_in: u64 = utxo_pairs.iter().map(|(_, o)| o.value.to_sat()).sum();

    // Estimate fee
    let vbytes = estimate_heir_claim_vbytes(utxo_pairs.len(), 1, 1);
    let fee_sat = (vbytes as f64 * fee_rate).ceil() as u64;
    let fee = Amount::from_sat(fee_sat);

    let send_amount = total_in.saturating_sub(fee_sat);
    if send_amount < 546 {
        return Ok(CcdResult::err(format!(
            "After fees ({} sat), only {} sat remains — below dust limit. \
             Wait for lower fees or deposit more to the vault.",
            fee_sat, send_amount
        )));
    }

    // Build the PSBT
    let psbt = match build_heir_claim_psbt(&vault, recovery_index, &utxo_pairs, &destination, fee) {
        Ok(p) => p,
        Err(e) => return Ok(CcdResult::err(format!("Failed to build claim PSBT: {}", e))),
    };

    use base64::prelude::*;
    let psbt_bytes = psbt.serialize();
    let psbt_b64 = BASE64_STANDARD.encode(&psbt_bytes);

    Ok(CcdResult::ok(ClaimPsbtResult {
        psbt_base64: psbt_b64,
        destination: destination_address,
        send_amount_sat: send_amount,
        fee_sat,
        fee_rate,
    }))
}

/// Broadcast a signed heir claim PSBT.
///
/// The heir signed the PSBT on their hardware wallet.
/// This command finalizes and broadcasts it.
#[tauri::command]
pub async fn broadcast_heir_claim(
    signed_psbt_b64: String,
    state: State<'_, AppState>,
) -> Result<CcdResult<String>, ()> {
    use base64::prelude::*;

    let psbt_bytes = match BASE64_STANDARD.decode(&signed_psbt_b64) {
        Ok(b) => b,
        Err(e) => return Ok(CcdResult::err(format!("Invalid base64: {}", e))),
    };

    let psbt: bitcoin::psbt::Psbt = match bitcoin::psbt::Psbt::deserialize(&psbt_bytes) {
        Ok(p) => p,
        Err(e) => return Ok(CcdResult::err(format!("Invalid PSBT: {}", e))),
    };

    let tx = match psbt.extract_tx() {
        Ok(t) => t,
        Err(e) => {
            return Ok(CcdResult::err(format!(
                "PSBT not fully signed: {}. \
                 Make sure all required heirs have signed.",
                e
            )))
        }
    };

    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    let client = match nostring_electrum::ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => return Ok(CcdResult::err(format!("Electrum connection failed: {}", e))),
    };

    let txid = match client.broadcast(&tx) {
        Ok(id) => id,
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("non-BIP68-final") || msg.contains("non-final") {
                return Ok(CcdResult::err(
                    "Transaction rejected: timelock has not expired yet. \
                     The vault's check-in period has not elapsed.",
                ));
            }
            return Ok(CcdResult::err(format!("Broadcast failed: {}", e)));
        }
    };

    log::info!("Heir claim broadcast: {}", txid);

    // Log the claim
    {
        let mut ccd = state.ccd.lock().unwrap();
        ccd.vault = None; // Vault is spent
    }

    Ok(CcdResult::ok(txid.to_string()))
}

/// Merge partially-signed PSBTs from multiple heirs.
///
/// For threshold claims (e.g., 2-of-3), each heir signs the same unsigned PSBT
/// independently. This command combines their signatures into a single PSBT.
/// Once the threshold is met, the PSBT can be broadcast via `broadcast_heir_claim`.
///
/// Flow for 2-of-3:
/// 1. Heir A calls `build_heir_claim` → gets unsigned PSBT
/// 2. Heir A signs on their hardware wallet → partially signed PSBT
/// 3. Heir A sends partially signed PSBT to Heir B (via Nostr DM / QR)
/// 4. Heir B signs the same unsigned PSBT on their device → another partial PSBT
/// 5. Either heir calls `merge_heir_signatures` with both partial PSBTs
/// 6. If threshold met → call `broadcast_heir_claim`
#[tauri::command]
pub async fn merge_heir_signatures(signed_psbts_b64: Vec<String>) -> Result<CcdResult<String>, ()> {
    use base64::prelude::*;
    use bitcoin::psbt::Psbt;

    if signed_psbts_b64.is_empty() {
        return Ok(CcdResult::err("No PSBTs provided"));
    }

    // Deserialize all PSBTs
    let mut psbts: Vec<Psbt> = Vec::new();
    for (i, b64) in signed_psbts_b64.iter().enumerate() {
        let bytes = match BASE64_STANDARD.decode(b64) {
            Ok(b) => b,
            Err(e) => {
                return Ok(CcdResult::err(format!(
                    "Invalid base64 in PSBT {}: {}",
                    i, e
                )))
            }
        };
        let psbt = match Psbt::deserialize(&bytes) {
            Ok(p) => p,
            Err(e) => return Ok(CcdResult::err(format!("Invalid PSBT {}: {}", i, e))),
        };
        psbts.push(psbt);
    }

    // Start with the first PSBT and merge others into it
    let mut combined = psbts.remove(0);
    for (i, other) in psbts.iter().enumerate() {
        if let Err(e) = combined.combine(other.clone()) {
            return Ok(CcdResult::err(format!(
                "Failed to merge PSBT {}: {}",
                i + 1,
                e
            )));
        }
    }

    // Return the combined PSBT
    let combined_bytes = combined.serialize();
    let combined_b64 = BASE64_STANDARD.encode(&combined_bytes);

    Ok(CcdResult::ok(combined_b64))
}

/// Generate a vault backup for the owner to send to heirs.
///
/// Called on the OWNER's device. Produces the JSON that heirs import.
#[tauri::command]
pub async fn export_vault_backup(state: State<'_, AppState>) -> Result<CcdResult<String>, ()> {
    let vault = {
        let ccd = state.ccd.lock().unwrap();
        match &ccd.vault {
            Some(v) => v.clone(),
            None => return Ok(CcdResult::err("No CCD vault created")),
        }
    };

    let network = *state.network.lock().unwrap();
    let network_str = match network {
        Network::Bitcoin => "bitcoin",
        Network::Testnet => "testnet",
        Network::Signet => "signet",
        Network::Regtest => "regtest",
        _ => "bitcoin",
    };

    // Get cosigner info
    let (cosigner_pubkey, chain_code) = {
        let conn = state.db.lock().unwrap();
        let pk = db::config_get(&conn, "cosigner_pubkey")
            .ok()
            .flatten()
            .unwrap_or_default();
        let cc = db::config_get(&conn, "cosigner_chain_code")
            .ok()
            .flatten()
            .unwrap_or_default();
        (pk, cc)
    };

    let address_index: u32 = {
        let conn = state.db.lock().unwrap();
        db::config_get(&conn, "ccd_vault_index")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };

    // Build heir entries.
    //
    // Recovery index mapping:
    // - Single-timelock vault (all heirs in one multi_a leaf): ALL heirs get index 0
    // - Cascade vault (heirs at different timelocks): each tier is a separate leaf
    //
    // The vault's recovery_scripts vec has one entry per Tapscript leaf.
    // For non-cascade, there's one leaf containing all heirs → index 0.
    // For cascade, each heir/group gets their own leaf index.
    let recovery_index_for_all = if vault.recovery_scripts.len() == 1 {
        // Single leaf: all heirs share recovery_index 0
        Some(0usize)
    } else {
        // Cascade: need per-heir mapping (TODO: store heir→leaf mapping in DB)
        None
    };

    let heirs: Vec<HeirBackupEntry> = {
        let registry = state.heir_registry.lock().unwrap();
        registry
            .list()
            .iter()
            .enumerate()
            .map(|(i, h)| HeirBackupEntry {
                label: h.label.clone(),
                xpub: h.xpub.to_string(),
                fingerprint: h.fingerprint.to_string(),
                derivation_path: h.derivation_path.to_string(),
                recovery_index: recovery_index_for_all.unwrap_or(i),
                npub: h.npub.clone(),
            })
            .collect()
    };

    let backup = VaultBackup {
        version: 1,
        network: network_str.to_string(),
        owner_pubkey: vault.owner_pubkey.to_string(),
        cosigner_pubkey,
        chain_code,
        address_index,
        timelock_blocks: vault.timelock.blocks(),
        threshold: {
            // Read threshold from DB (set during vault creation), default to n-of-n
            let conn = state.db.lock().unwrap();
            db::config_get(&conn, "ccd_vault_threshold")
                .ok()
                .flatten()
                .and_then(|s| s.parse().ok())
                .unwrap_or(heirs.len())
        },
        heirs,
        vault_address: vault.address.to_string(),
        taproot_internal_key: Some(hex::encode(vault.aggregate_xonly.serialize())),
        recovery_leaves: extract_recovery_leaves(&vault),
        created_at: Some({
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            format!("{}", secs)
        }),
    };

    match serde_json::to_string_pretty(&backup) {
        Ok(json) => Ok(CcdResult::ok(json)),
        Err(e) => Ok(CcdResult::err(format!("Serialization failed: {}", e))),
    }
}

// ============================================================================
// QR Code Compression
// ============================================================================

/// Compress the vault backup JSON into a `nostring:v1:<base64(gzip)>` URI for QR display.
#[tauri::command]
pub async fn compress_vault_for_qr(state: State<'_, AppState>) -> Result<CcdResult<String>, ()> {
    use base64::Engine;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    // First get the export JSON
    let export_result = export_vault_backup(state).await?;
    let json = match export_result.data {
        Some(j) => j,
        None => {
            return Ok(CcdResult::err(
                export_result.error.unwrap_or_else(|| "No backup".into()),
            ))
        }
    };

    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    if let Err(e) = encoder.write_all(json.as_bytes()) {
        return Ok(CcdResult::err(format!("Compression failed: {}", e)));
    }
    let compressed = match encoder.finish() {
        Ok(c) => c,
        Err(e) => return Ok(CcdResult::err(format!("Compression failed: {}", e))),
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&compressed);
    let uri = format!("nostring:v1:{}", b64);

    Ok(CcdResult::ok(uri))
}

// ============================================================================
// NIP-17 Descriptor Delivery
// ============================================================================

/// Result of delivering vault backup to heirs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryReport {
    /// Heirs who received the backup successfully
    pub delivered: Vec<String>,
    /// Heirs skipped (no npub configured)
    pub skipped: Vec<String>,
    /// Heirs where delivery failed
    pub failed: Vec<DeliveryFailure>,
}

/// A failed delivery attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryFailure {
    pub heir_label: String,
    pub error: String,
}

/// Deliver the vault backup to all heirs with Nostr npubs via NIP-17.
///
/// Requires the owner's Nostr secret key (nsec or hex) and relay list.
/// Heirs without npubs are skipped (use manual export for those).
///
/// Returns a report showing which heirs received the backup, which were
/// skipped, and which failed.
#[tauri::command]
pub async fn deliver_descriptor_to_heirs(
    owner_nsec: String,
    relays: Vec<String>,
    state: State<'_, AppState>,
) -> Result<CcdResult<DeliveryReport>, ()> {
    {
        let unlocked = state.unlocked.lock().unwrap();
        if !*unlocked {
            return Ok(CcdResult::err("Wallet is locked"));
        }
    }

    if relays.is_empty() {
        return Ok(CcdResult::err("No relays configured"));
    }

    // Build the VaultBackup from current state (all locks released before async)
    let backup = {
        let ccd = state.ccd.lock().unwrap();
        let vault = match &ccd.vault {
            Some(v) => v.clone(),
            None => return Ok(CcdResult::err("No vault created")),
        };
        let cosigner = match &ccd.cosigner {
            Some(c) => c.clone(),
            None => return Ok(CcdResult::err("No co-signer registered")),
        };
        let heirs = state.heir_registry.lock().unwrap().clone();
        let network = *state.network.lock().unwrap();
        drop(ccd);
        match build_vault_backup(&vault, &cosigner, &heirs, network) {
            Ok(b) => b,
            Err(()) => return Ok(CcdResult::err("Failed to build vault backup")),
        }
    };

    let backup_json = match serde_json::to_string_pretty(&backup) {
        Ok(j) => j,
        Err(e) => return Ok(CcdResult::err(format!("Failed to serialize backup: {}", e))),
    };

    let mut report = DeliveryReport {
        delivered: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
    };

    // Deliver to each heir with an npub
    for heir_entry in &backup.heirs {
        let npub = match &heir_entry.npub {
            Some(n) if !n.is_empty() => n,
            _ => {
                report.skipped.push(heir_entry.label.clone());
                continue;
            }
        };

        match nostring_notify::nostr_dm::deliver_vault_backup(
            &owner_nsec,
            npub,
            &relays,
            &backup_json,
        )
        .await
        {
            Ok(event_id) => {
                log::info!(
                    "Vault backup delivered to {} (event: {})",
                    heir_entry.label,
                    event_id
                );
                report.delivered.push(heir_entry.label.clone());
            }
            Err(e) => {
                log::warn!("Failed to deliver to {}: {}", heir_entry.label, e);
                report.failed.push(DeliveryFailure {
                    heir_label: heir_entry.label.clone(),
                    error: e.to_string(),
                });
            }
        }
    }

    Ok(CcdResult::ok(report))
}

/// Build a VaultBackup from current app state.
fn build_vault_backup(
    vault: &nostring_inherit::taproot::InheritableVault,
    cosigner: &nostring_ccd::types::DelegatedKey,
    heirs: &nostring_inherit::heir::HeirRegistry,
    network: bitcoin::Network,
) -> Result<VaultBackup, ()> {
    let heir_entries: Vec<HeirBackupEntry> = heirs
        .heirs
        .iter()
        .enumerate()
        .map(|(i, heir)| HeirBackupEntry {
            label: heir.label.clone(),
            xpub: heir.xpub.to_string(),
            fingerprint: heir.fingerprint.to_string(),
            derivation_path: heir.derivation_path.to_string(),
            recovery_index: i,
            npub: heir.npub.clone(),
        })
        .collect();

    let network_str = match network {
        bitcoin::Network::Bitcoin => "mainnet",
        bitcoin::Network::Testnet => "testnet",
        bitcoin::Network::Signet => "signet",
        bitcoin::Network::Regtest => "regtest",
        _ => "unknown",
    };

    let recovery_leaves = extract_recovery_leaves(vault);
    let internal_key_hex = hex::encode(vault.aggregate_xonly.serialize());

    Ok(VaultBackup {
        version: 1,
        network: network_str.to_string(),
        owner_pubkey: hex::encode(vault.owner_pubkey.serialize()),
        cosigner_pubkey: hex::encode(cosigner.cosigner_pubkey.serialize()),
        chain_code: hex::encode(cosigner.chain_code.0),
        address_index: vault.address_index,
        timelock_blocks: vault.timelock.blocks(),
        threshold: heir_entries.len().max(1),
        heirs: heir_entries,
        vault_address: vault.address.to_string(),
        taproot_internal_key: Some(internal_key_hex),
        recovery_leaves,
        created_at: Some({
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            format!("{}", secs)
        }),
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use nostring_inherit::backup::RecoveryLeaf;

    fn sample_backup() -> VaultBackup {
        VaultBackup {
            version: 1,
            network: "testnet".into(),
            owner_pubkey: "02a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc".into(),
            cosigner_pubkey: "03a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc".into(),
            chain_code: "ab".repeat(32),
            address_index: 0,
            timelock_blocks: 26280,
            threshold: 1,
            heirs: vec![HeirBackupEntry {
                label: "Alice".into(),
                xpub: "tpubD6NzVbkrYhZ4XgiXtGrdW5XDZA5gE4REcKytCFfnBKUmG3YMRnHk3JdCCcZd4XR2C3dAPHRjcL5LQtxWUpm2m2YbB5YFESaqxBJo8v4gMB7".into(),
                fingerprint: "00000000".into(),
                derivation_path: "m/84'/1'/0'".into(),
                recovery_index: 0,
                npub: Some("npub1test".into()),
            }],
            vault_address: "tb1ptestaddress".into(),
            taproot_internal_key: Some("a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc".into()),
            recovery_leaves: vec![RecoveryLeaf {
                leaf_index: 0,
                script_hex: "20abcd1234".into(),
                control_block_hex: "c0deadbeef".into(),
                timelock_blocks: 26280,
                leaf_version: 0xc0,
            }],
            created_at: Some("1739318400".into()),
        }
    }

    #[test]
    fn test_backup_roundtrip() {
        let backup = sample_backup();
        let json = serde_json::to_string_pretty(&backup).unwrap();
        let restored: VaultBackup = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.version, 1);
        assert_eq!(restored.network, "testnet");
        assert_eq!(restored.heirs.len(), 1);
        assert_eq!(restored.heirs[0].label, "Alice");
        assert_eq!(restored.heirs[0].recovery_index, 0);
        assert!(restored.heirs[0].npub.is_some());
        assert_eq!(restored.vault_address, "tb1ptestaddress");
        // New fields
        assert!(restored.taproot_internal_key.is_some());
        assert_eq!(restored.recovery_leaves.len(), 1);
        assert_eq!(restored.recovery_leaves[0].script_hex, "20abcd1234");
        assert_eq!(restored.recovery_leaves[0].control_block_hex, "c0deadbeef");
        assert_eq!(restored.recovery_leaves[0].timelock_blocks, 26280);
        assert_eq!(restored.recovery_leaves[0].leaf_version, 0xc0);
    }

    #[test]
    fn test_backup_backward_compat() {
        // Old v1 backup without new fields should still parse
        let old_json = serde_json::json!({
            "version": 1,
            "network": "testnet",
            "owner_pubkey": "02a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc",
            "cosigner_pubkey": "03a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc",
            "chain_code": "ab".repeat(32),
            "address_index": 0,
            "timelock_blocks": 26280,
            "threshold": 1,
            "heirs": [{
                "label": "Alice",
                "xpub": "tpubD6NzVbkrYhZ4XgiXtGrdW5XDZA5gE4REcKytCFfnBKUmG3YMRnHk3JdCCcZd4XR2C3dAPHRjcL5LQtxWUpm2m2YbB5YFESaqxBJo8v4gMB7",
                "fingerprint": "00000000",
                "derivation_path": "m/84'/1'/0'",
                "recovery_index": 0
            }],
            "vault_address": "tb1ptestaddress"
        });
        let restored: VaultBackup = serde_json::from_value(old_json).unwrap();
        assert_eq!(restored.version, 1);
        assert!(restored.taproot_internal_key.is_none());
        assert!(restored.recovery_leaves.is_empty());
        assert_eq!(restored.heirs[0].npub, None);
    }

    #[test]
    fn test_backup_without_optional_fields() {
        let json = r#"{
            "version": 1,
            "network": "bitcoin",
            "owner_pubkey": "02abc",
            "cosigner_pubkey": "03def",
            "chain_code": "abababababababababababababababababababababababababababababababababab",
            "address_index": 0,
            "timelock_blocks": 26280,
            "threshold": 1,
            "heirs": [],
            "vault_address": "bc1ptest"
        }"#;
        let backup: VaultBackup = serde_json::from_str(json).unwrap();
        assert!(backup.created_at.is_none());
        assert!(backup.heirs.is_empty());
        assert_eq!(backup.threshold, 1);
    }

    #[test]
    fn test_heir_entry_without_npub() {
        let json = r#"{
            "label": "Bob",
            "xpub": "xpub123",
            "fingerprint": "aabbccdd",
            "derivation_path": "m/84'/0'/0'",
            "recovery_index": 1
        }"#;
        let entry: HeirBackupEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.label, "Bob");
        assert_eq!(entry.recovery_index, 1);
        assert!(entry.npub.is_none());
    }

    #[test]
    fn test_quorum_info_single_heir() {
        let quorum = QuorumInfo {
            my_label: "Alice".into(),
            my_recovery_index: 0,
            total_heirs: 1,
            threshold: 1,
            all_heirs: vec!["Alice".into()],
        };
        assert_eq!(quorum.threshold, 1);
        assert_eq!(quorum.total_heirs, 1);
    }

    #[test]
    fn test_quorum_info_multi_heir() {
        let quorum = QuorumInfo {
            my_label: "Bob".into(),
            my_recovery_index: 0,
            total_heirs: 3,
            threshold: 2,
            all_heirs: vec!["Alice".into(), "Bob".into(), "Charlie".into()],
        };
        assert_eq!(quorum.threshold, 2);
        assert_eq!(quorum.total_heirs, 3);
    }

    #[test]
    fn test_address_network_validation() {
        // Testnet address should fail on mainnet
        let testnet_addr = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx";
        let parsed = Address::from_str(testnet_addr).unwrap();
        assert!(
            parsed.clone().require_network(Network::Bitcoin).is_err(),
            "testnet address must be rejected on mainnet"
        );
        assert!(
            parsed.require_network(Network::Testnet).is_ok(),
            "testnet address must be accepted on testnet"
        );

        // Mainnet address should fail on testnet
        let mainnet_addr = "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq";
        let parsed = Address::from_str(mainnet_addr).unwrap();
        assert!(
            parsed.clone().require_network(Network::Testnet).is_err(),
            "mainnet address must be rejected on testnet"
        );
        assert!(
            parsed.require_network(Network::Bitcoin).is_ok(),
            "mainnet address must be accepted on mainnet"
        );
    }

    #[test]
    fn test_backup_threshold_preserved() {
        let mut backup = sample_backup();
        backup.threshold = 2;
        backup.heirs.push(HeirBackupEntry {
            label: "Bob".into(),
            xpub: "tpubD6NzVbkrYhZ4XgiXtGrdW5XDZA5gE4REcKytCFfnBKUmG3YMRnHk3JdCCcZd4XR2C3dAPHRjcL5LQtxWUpm2m2YbB5YFESaqxBJo8v4gMB7".into(),
            fingerprint: "11111111".into(),
            derivation_path: "m/84'/1'/0'".into(),
            recovery_index: 0,
            npub: None,
        });
        backup.heirs.push(HeirBackupEntry {
            label: "Charlie".into(),
            xpub: "tpubD6NzVbkrYhZ4XgiXtGrdW5XDZA5gE4REcKytCFfnBKUmG3YMRnHk3JdCCcZd4XR2C3dAPHRjcL5LQtxWUpm2m2YbB5YFESaqxBJo8v4gMB7".into(),
            fingerprint: "22222222".into(),
            derivation_path: "m/84'/1'/0'".into(),
            recovery_index: 0,
            npub: None,
        });

        let json = serde_json::to_string(&backup).unwrap();
        let restored: VaultBackup = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.threshold, 2);
        assert_eq!(restored.heirs.len(), 3);
        // All share recovery_index 0 (single timelock, multi_a leaf)
        assert!(restored.heirs.iter().all(|h| h.recovery_index == 0));
    }

    #[test]
    fn test_delivery_report_serialization() {
        let report = DeliveryReport {
            delivered: vec!["Alice".into()],
            skipped: vec!["Bob".into()],
            failed: vec![DeliveryFailure {
                heir_label: "Charlie".into(),
                error: "Connection timeout".into(),
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        let restored: DeliveryReport = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.delivered.len(), 1);
        assert_eq!(restored.skipped.len(), 1);
        assert_eq!(restored.failed.len(), 1);
        assert_eq!(restored.delivered[0], "Alice");
        assert_eq!(restored.skipped[0], "Bob");
        assert_eq!(restored.failed[0].heir_label, "Charlie");
        assert_eq!(restored.failed[0].error, "Connection timeout");
    }

    #[test]
    fn test_build_vault_backup() {
        use bitcoin::secp256k1::{Secp256k1, SecretKey};
        use bitcoin::Network;
        use nostring_ccd::types::ChainCode;
        use nostring_inherit::heir::{HeirKey, HeirRegistry};
        use nostring_inherit::taproot::create_inheritable_vault;

        let secp = Secp256k1::new();
        let owner_sk = SecretKey::from_slice(&[0x01; 32]).unwrap();
        let cosigner_sk = SecretKey::from_slice(&[0x02; 32]).unwrap();
        let owner_pk = owner_sk.public_key(&secp);
        let cosigner_pk = cosigner_sk.public_key(&secp);

        let chain_code = ChainCode([0xCC; 32]);
        let delegated =
            nostring_ccd::register_cosigner_with_chain_code(cosigner_pk, chain_code, "cosigner");

        // Create heir
        let heir_sk = SecretKey::from_slice(&[0x03; 32]).unwrap();
        let heir_pk = heir_sk.public_key(&secp);
        let heir_xpub = bitcoin::bip32::Xpub {
            network: bitcoin::NetworkKind::Test,
            depth: 0,
            parent_fingerprint: bitcoin::bip32::Fingerprint::from([0; 4]),
            child_number: bitcoin::bip32::ChildNumber::from_normal_idx(0).unwrap(),
            public_key: heir_pk,
            chain_code: bitcoin::bip32::ChainCode::from([0xAA; 32]),
        };
        let mut registry = HeirRegistry::new();
        registry.add(HeirKey {
            label: "Alice".into(),
            fingerprint: bitcoin::bip32::Fingerprint::from([0xAA, 0xBB, 0xCC, 0xDD]),
            xpub: heir_xpub,
            derivation_path: "m/86'/1'/0'".parse().unwrap(),
            npub: Some("npub1abc".into()),
        });

        let timelock = nostring_inherit::policy::Timelock::from_blocks(26280).unwrap();
        let path_info = crate::state::CcdState::heirs_to_path_info(&registry.heirs).unwrap();
        let vault = create_inheritable_vault(
            &owner_pk,
            &delegated,
            0,
            path_info,
            timelock,
            0,
            Network::Testnet,
        )
        .unwrap();

        let backup = build_vault_backup(&vault, &delegated, &registry, Network::Testnet).unwrap();

        assert_eq!(backup.version, 1);
        assert_eq!(backup.network, "testnet");
        assert_eq!(backup.address_index, 0);
        assert_eq!(backup.heirs.len(), 1);
        assert_eq!(backup.heirs[0].label, "Alice");
        assert_eq!(backup.heirs[0].npub, Some("npub1abc".into()));
        assert_eq!(backup.threshold, 1);
        assert!(!backup.vault_address.is_empty());
        assert!(backup.created_at.is_some());
    }
}
