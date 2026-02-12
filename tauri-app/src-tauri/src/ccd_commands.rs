//! CCD (Chain Code Delegation) Tauri commands.
//!
//! Provides collaborative custody with Taproot inheritable vaults:
//! - Co-signer registration (import xpub + chain code)
//! - Vault creation (MuSig2 key-path + heir script-path)
//! - Check-in PSBT generation (key-path spend to reset timelocks)
//! - Heartbeat status evaluation (deadman switch monitoring)

use crate::db;
use crate::state::AppState;
use bitcoin::secp256k1::PublicKey;
use nostring_ccd::register_cosigner_with_chain_code;
use nostring_ccd::types::ChainCode;
use nostring_inherit::heartbeat::{evaluate_heartbeat, HeartbeatConfig};
use nostring_inherit::policy::Timelock;
use nostring_inherit::taproot::create_inheritable_vault;
use nostring_inherit::taproot_checkin::{build_taproot_checkin_psbt, TaprootCheckinConfig};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tauri::State;

// ============================================================================
// Response types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CcdResult<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> CcdResult<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosignerInfo {
    pub label: String,
    pub pubkey: String,
    pub chain_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultInfo {
    pub address: String,
    pub address_index: u32,
    pub network: String,
    pub timelock_blocks: u32,
    pub heir_count: usize,
    pub aggregate_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatInfo {
    pub current_block: u64,
    pub expiry_block: u64,
    pub blocks_remaining: i64,
    pub days_remaining: f64,
    pub action: String,
    pub elapsed_fraction: f64,
}

// ============================================================================
// Validation
// ============================================================================

/// Maximum label length for cosigner names.
const MAX_LABEL_LEN: usize = 64;

/// Allowed characters in labels: alphanumeric, spaces, hyphens, underscores.
fn validate_label(label: &str) -> Result<(), String> {
    if label.is_empty() {
        return Err("Label cannot be empty".into());
    }
    if label.len() > MAX_LABEL_LEN {
        return Err(format!(
            "Label too long ({} chars, max {})",
            label.len(),
            MAX_LABEL_LEN
        ));
    }
    if !label
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
    {
        return Err(
            "Label may only contain letters, numbers, spaces, hyphens, and underscores".into(),
        );
    }
    Ok(())
}

// ============================================================================
// Startup health check
// ============================================================================

/// Check if CCD vault loaded correctly on startup.
///
/// Returns `Ok(None)` if no vault configured, `Ok(Some(error))` if
/// vault data exists but reconstruction failed. The frontend should
/// call this on startup and display a warning banner if non-null.
#[tauri::command]
pub async fn get_ccd_load_error(state: State<'_, AppState>) -> Result<Option<String>, ()> {
    let ccd = state.ccd.lock().unwrap();
    Ok(ccd.load_error.clone())
}

// ============================================================================
// Fee estimation
// ============================================================================

/// Default fee rate fallback (sat/vB).
const DEFAULT_FEE_RATE: f64 = 10.0;

/// Estimate fee rate from Electrum, with fallback.
fn estimate_fee_rate(client: &nostring_electrum::ElectrumClient) -> f64 {
    // Target 6 blocks (~1 hour) for check-in — not urgent
    client
        .estimate_fee_rate(6)
        .unwrap_or(DEFAULT_FEE_RATE)
        .clamp(1.0, 500.0) // Floor 1, ceiling 500 sat/vB (anti-manipulation)
}

// ============================================================================
// Commands
// ============================================================================

/// Register a co-signer's public key and chain code.
///
/// The co-signer generates their own keypair and chain code,
/// then shares the pubkey + chain code with the owner (e.g., via QR code).
/// The owner imports these to create CCD vaults.
#[tauri::command]
pub async fn register_cosigner(
    pubkey_hex: String,
    chain_code_hex: String,
    label: String,
    state: State<'_, AppState>,
) -> Result<CcdResult<CosignerInfo>, ()> {
    // Validate label
    if let Err(e) = validate_label(&label) {
        return Ok(CcdResult::err(e));
    }

    // Parse pubkey
    let pubkey = match PublicKey::from_str(&pubkey_hex) {
        Ok(pk) => pk,
        Err(e) => return Ok(CcdResult::err(format!("Invalid public key: {}", e))),
    };

    // Parse chain code (32 bytes hex)
    let cc_bytes = match hex::decode(&chain_code_hex) {
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
        Err(e) => return Ok(CcdResult::err(format!("Invalid chain code hex: {}", e))),
    };

    let chain_code = ChainCode(cc_bytes);
    let delegated = register_cosigner_with_chain_code(pubkey, chain_code, &label);

    // Persist to database
    {
        let conn = state.db.lock().unwrap();
        let _ = db::config_set(&conn, "cosigner_pubkey", &pubkey_hex);
        let _ = db::config_set(&conn, "cosigner_chain_code", &chain_code_hex);
        let _ = db::config_set(&conn, "cosigner_label", &label);
    }

    // Update in-memory CCD state
    {
        let mut ccd = state.ccd.lock().unwrap();
        ccd.cosigner = Some(delegated);
    }

    Ok(CcdResult::ok(CosignerInfo {
        label,
        pubkey: pubkey_hex,
        chain_code: chain_code_hex,
    }))
}

/// Create a CCD inheritable vault.
///
/// Combines MuSig2 key-path (owner + cosigner) with script-path
/// (heir timelock recovery). The vault address commits to both paths
/// in a single Taproot output.
///
/// Requires: cosigner registered, at least one heir configured.
#[tauri::command]
pub async fn create_ccd_vault(
    timelock_blocks: u16,
    address_index: Option<u32>,
    state: State<'_, AppState>,
) -> Result<CcdResult<VaultInfo>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CcdResult::err("Wallet is locked"));
    }
    drop(unlocked);

    // Get cosigner from CCD state
    let delegated = {
        let ccd = state.ccd.lock().unwrap();
        match &ccd.cosigner {
            Some(d) => d.clone(),
            None => return Ok(CcdResult::err("No co-signer registered")),
        }
    };

    // Get owner pubkey
    let owner_xpub_str = {
        let lock = state.owner_xpub.lock().unwrap();
        match &*lock {
            Some(x) => x.clone(),
            None => return Ok(CcdResult::err("No owner key available")),
        }
    };

    let owner_xpub = match bitcoin::bip32::Xpub::from_str(&owner_xpub_str) {
        Ok(x) => x,
        Err(e) => return Ok(CcdResult::err(format!("Invalid owner xpub: {}", e))),
    };

    let owner_pubkey = owner_xpub.public_key;

    // Get heirs and build path info
    let (path_info, heir_count) = {
        let registry = state.heir_registry.lock().unwrap();
        let heirs = registry.list();
        if heirs.is_empty() {
            return Ok(CcdResult::err(
                "No heirs configured. Add at least one heir first.",
            ));
        }
        let count = heirs.len();
        let path = match crate::state::CcdState::heirs_to_path_info(heirs) {
            Some(p) => p,
            None => return Ok(CcdResult::err("Failed to convert heir keys")),
        };
        (path, count)
    };

    let timelock = match Timelock::from_blocks(timelock_blocks) {
        Ok(t) => t,
        Err(e) => return Ok(CcdResult::err(format!("Invalid timelock: {}", e))),
    };

    let network = *state.network.lock().unwrap();
    let idx = address_index.unwrap_or(0);

    // Create the vault
    let vault = match create_inheritable_vault(
        &owner_pubkey,
        &delegated,
        idx,
        path_info,
        timelock,
        0,
        network,
    ) {
        Ok(v) => v,
        Err(e) => return Ok(CcdResult::err(format!("Vault creation failed: {}", e))),
    };

    let info = VaultInfo {
        address: vault.address.to_string(),
        address_index: idx,
        network: format!("{:?}", network),
        timelock_blocks: timelock_blocks as u32,
        heir_count,
        aggregate_key: vault.aggregate_xonly.to_string(),
    };

    // Persist vault parameters (for reconstruction on restart)
    {
        let conn = state.db.lock().unwrap();
        let _ = db::config_set(&conn, "ccd_vault_address", &info.address);
        let _ = db::config_set(&conn, "ccd_vault_index", &idx.to_string());
        let _ = db::config_set(&conn, "ccd_vault_timelock", &timelock_blocks.to_string());
    }

    // Store vault in CCD state
    {
        let mut ccd = state.ccd.lock().unwrap();
        ccd.vault = Some(vault);
    }

    Ok(CcdResult::ok(info))
}

/// Build a Taproot key-path check-in PSBT.
///
/// This creates an unsigned PSBT that spends the vault via the MuSig2 key-path
/// and recreates it at the same address (resetting the timelock).
/// The PSBT must then go through the MuSig2 signing ceremony with the co-signer.
///
/// Returns base64-encoded unsigned PSBT.
#[tauri::command]
pub async fn build_checkin_psbt(state: State<'_, AppState>) -> Result<CcdResult<String>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CcdResult::err("Wallet is locked"));
    }
    drop(unlocked);

    let vault = {
        let ccd = state.ccd.lock().unwrap();
        match &ccd.vault {
            Some(v) => v.clone(),
            None => return Ok(CcdResult::err("No CCD vault created")),
        }
    };

    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    // Connect to Electrum
    let client = match nostring_electrum::ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => {
            return Ok(CcdResult::err(format!(
                "Failed to connect to Electrum: {}",
                e
            )))
        }
    };

    // Estimate fee rate dynamically
    let fee_rate = estimate_fee_rate(&client);

    let script = vault.address.script_pubkey();
    let utxos = match client.get_utxos_for_script(&script) {
        Ok(u) => u,
        Err(e) => return Ok(CcdResult::err(format!("Failed to get UTXOs: {}", e))),
    };

    if utxos.is_empty() {
        return Ok(CcdResult::err(
            "No UTXOs found at vault address. Deposit funds first.",
        ));
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

    let config = TaprootCheckinConfig {
        vault: vault.clone(),
        utxos: utxo_pairs,
        fee_rate,
        extra_outputs: vec![],
    };

    match build_taproot_checkin_psbt(&config) {
        Ok(checkin) => {
            use base64::prelude::*;
            let psbt_bytes = checkin.psbt.serialize();
            let psbt_b64 = BASE64_STANDARD.encode(&psbt_bytes);
            Ok(CcdResult::ok(psbt_b64))
        }
        Err(e) => Ok(CcdResult::err(format!(
            "Failed to build check-in PSBT: {}",
            e
        ))),
    }
}

/// Get heartbeat status for the current vault.
///
/// Evaluates the vault's timelock against the current blockchain height
/// and returns recommended actions (NoAction, Recommend, Critical, Expired).
#[tauri::command]
pub async fn get_heartbeat_status(
    state: State<'_, AppState>,
) -> Result<CcdResult<HeartbeatInfo>, ()> {
    let vault = {
        let ccd = state.ccd.lock().unwrap();
        match &ccd.vault {
            Some(v) => v.clone(),
            None => return Ok(CcdResult::err("No CCD vault created")),
        }
    };

    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    // Get current block height
    let client = match nostring_electrum::ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => {
            return Ok(CcdResult::err(format!(
                "Failed to connect to Electrum: {}",
                e
            )))
        }
    };

    let current_height = match client.get_height() {
        Ok(h) => h,
        Err(e) => return Ok(CcdResult::err(format!("Failed to get block height: {}", e))),
    };

    // Get UTXO confirmation height (= last check-in height)
    let script = vault.address.script_pubkey();
    let utxos = match client.get_utxos_for_script(&script) {
        Ok(u) => u,
        Err(e) => return Ok(CcdResult::err(format!("Failed to get UTXOs: {}", e))),
    };

    let last_checkin_height = utxos.iter().map(|u| u.height).max().unwrap_or(0);
    if last_checkin_height == 0 {
        return Ok(CcdResult::err(
            "Vault has no confirmed UTXOs. Deposit and wait for confirmation.",
        ));
    }

    let config = HeartbeatConfig::default();
    let status = evaluate_heartbeat(&vault, last_checkin_height, current_height, &config);

    let timelock_blocks = vault.timelock.blocks() as u64;
    let expiry = last_checkin_height as u64 + timelock_blocks;
    let remaining = expiry as i64 - current_height as i64;
    let days = remaining as f64 * 10.0 / 1440.0; // ~10 min per block

    let action_str = format!("{:?}", status.action);

    Ok(CcdResult::ok(HeartbeatInfo {
        current_block: current_height as u64,
        expiry_block: expiry,
        blocks_remaining: remaining,
        days_remaining: days,
        action: action_str,
        elapsed_fraction: status.elapsed_fraction,
    }))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_label_valid() {
        assert!(validate_label("My Cosigner").is_ok());
        assert!(validate_label("cosigner-1").is_ok());
        assert!(validate_label("cold_storage_2").is_ok());
        assert!(validate_label("A").is_ok());
    }

    #[test]
    fn test_validate_label_empty() {
        assert!(validate_label("").is_err());
    }

    #[test]
    fn test_validate_label_too_long() {
        let long = "a".repeat(MAX_LABEL_LEN + 1);
        assert!(validate_label(&long).is_err());
        // Exactly at limit is ok
        let exact = "a".repeat(MAX_LABEL_LEN);
        assert!(validate_label(&exact).is_ok());
    }

    #[test]
    fn test_validate_label_bad_chars() {
        assert!(validate_label("cos<script>igner").is_err());
        assert!(validate_label("cosigner;DROP TABLE").is_err());
        assert!(validate_label("cos/igner").is_err());
        assert!(validate_label("co\nsigner").is_err());
    }

    #[test]
    fn test_ccd_result_ok() {
        let r = CcdResult::ok("hello".to_string());
        assert!(r.success);
        assert_eq!(r.data.unwrap(), "hello");
        assert!(r.error.is_none());
    }

    #[test]
    fn test_ccd_result_err() {
        let r: CcdResult<String> = CcdResult::err("bad input");
        assert!(!r.success);
        assert!(r.data.is_none());
        assert_eq!(r.error.unwrap(), "bad input");
    }
}
