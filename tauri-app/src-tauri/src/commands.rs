//! Tauri commands — the bridge between frontend and Rust backend
//!
//! All commands are async and return JSON-serializable results.

use crate::state::{AppState, PolicyStatus};
use bitcoin::psbt::Psbt;
use nostring_core::crypto::{decrypt_seed, encrypt_seed, EncryptedSeed};
use nostring_core::seed::{derive_seed, generate_mnemonic, parse_mnemonic, WordCount};
use nostring_electrum::ElectrumClient;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Result type for commands
#[derive(Debug, Serialize, Deserialize)]
pub struct CommandResult<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> CommandResult<T> {
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

// ============================================================================
// Seed Management Commands
// ============================================================================

/// Generate a new BIP-39 mnemonic
#[tauri::command]
pub async fn create_seed(word_count: Option<usize>) -> CommandResult<String> {
    let wc = match word_count.unwrap_or(24) {
        12 => WordCount::Words12,
        15 => WordCount::Words15,
        18 => WordCount::Words18,
        21 => WordCount::Words21,
        24 => WordCount::Words24,
        _ => return CommandResult::err("Word count must be 12, 15, 18, 21, or 24"),
    };
    
    match generate_mnemonic(wc) {
        Ok(mnemonic) => CommandResult::ok(mnemonic.to_string()),
        Err(e) => CommandResult::err(format!("Failed to generate mnemonic: {}", e)),
    }
}

/// Validate a BIP-39 mnemonic
#[tauri::command]
pub async fn validate_seed(mnemonic: String) -> CommandResult<bool> {
    match parse_mnemonic(&mnemonic) {
        Ok(_) => CommandResult::ok(true),
        Err(e) => CommandResult::err(format!("Invalid mnemonic: {}", e)),
    }
}

/// Import and encrypt a seed
#[tauri::command]
pub async fn import_seed(
    mnemonic: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<bool>, ()> {
    // Parse and validate mnemonic
    let parsed = match parse_mnemonic(&mnemonic) {
        Ok(m) => m,
        Err(e) => return Ok(CommandResult::err(format!("Invalid mnemonic: {}", e))),
    };

    // Derive the 64-byte seed from mnemonic
    let seed = derive_seed(&parsed, "");

    // Encrypt the seed
    match encrypt_seed(&seed, &password) {
        Ok(encrypted) => {
            // Store encrypted bytes in state
            let encrypted_bytes = encrypted.to_bytes();
            let mut seed_lock = state.encrypted_seed.lock().unwrap();
            *seed_lock = Some(encrypted_bytes);

            // Mark as unlocked
            let mut unlocked = state.unlocked.lock().unwrap();
            *unlocked = true;

            Ok(CommandResult::ok(true))
        }
        Err(e) => Ok(CommandResult::err(format!("Failed to encrypt seed: {}", e))),
    }
}

/// Check if a seed is loaded
#[tauri::command]
pub async fn has_seed(state: State<'_, AppState>) -> Result<bool, ()> {
    let seed_lock = state.encrypted_seed.lock().unwrap();
    Ok(seed_lock.is_some())
}

/// Unlock (decrypt) the seed with password
#[tauri::command]
pub async fn unlock_seed(
    password: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<bool>, ()> {
    let seed_lock = state.encrypted_seed.lock().unwrap();

    match &*seed_lock {
        None => Ok(CommandResult::err("No seed loaded")),
        Some(encrypted_bytes) => {
            let encrypted = match EncryptedSeed::from_bytes(encrypted_bytes) {
                Ok(e) => e,
                Err(_) => return Ok(CommandResult::err("Corrupted seed data")),
            };

            match decrypt_seed(&encrypted, &password) {
                Ok(_) => {
                    drop(seed_lock); // Release lock before acquiring another
                    let mut unlocked = state.unlocked.lock().unwrap();
                    *unlocked = true;
                    Ok(CommandResult::ok(true))
                }
                Err(_) => Ok(CommandResult::err("Incorrect password")),
            }
        }
    }
}

/// Lock the wallet (clear unlocked state)
#[tauri::command]
pub async fn lock_wallet(state: State<'_, AppState>) -> Result<(), ()> {
    let mut unlocked = state.unlocked.lock().unwrap();
    *unlocked = false;
    Ok(())
}

// ============================================================================
// Policy Status Commands
// ============================================================================

/// Get current inheritance policy status
#[tauri::command]
pub async fn get_policy_status(state: State<'_, AppState>) -> Result<Option<PolicyStatus>, ()> {
    let status_lock = state.policy_status.lock().unwrap();
    Ok(status_lock.clone())
}

/// Refresh policy status from blockchain
#[tauri::command]
pub async fn refresh_policy_status(
    state: State<'_, AppState>,
) -> Result<CommandResult<PolicyStatus>, ()> {
    // Get Electrum URL and network from state
    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    // Connect to Electrum
    let client = match ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => return Ok(CommandResult::err(format!("Failed to connect to Electrum: {}", e))),
    };

    // Get current block height
    let current_block = match client.get_height() {
        Ok(h) => h as u64,
        Err(e) => return Ok(CommandResult::err(format!("Failed to get block height: {}", e))),
    };

    // Check if we have inheritance config
    let config_lock = state.inheritance_config.lock().unwrap();
    let (expiry_block, blocks_remaining, days_remaining, urgency) = if let Some(config) = &*config_lock {
        // Calculate based on config
        // For simplicity, assume UTXO was created at current_block - timelock_blocks
        // In production, track the actual UTXO confirmation height
        let timelock = config.timelock_blocks as u64;
        let expiry = current_block + timelock; // Simplified - should use actual UTXO height
        let remaining = expiry.saturating_sub(current_block) as i64;
        let days = remaining as f64 * 10.0 / 60.0 / 24.0; // ~10 min per block

        let urgency = if remaining > 4320 {
            "ok" // > 30 days
        } else if remaining > 1008 {
            "warning" // > 7 days
        } else {
            "critical"
        };

        (expiry, remaining, days, urgency.to_string())
    } else {
        // No config yet - return placeholder
        (current_block + 26280, 26280, 182.5, "ok".to_string())
    };
    drop(config_lock);

    let status = PolicyStatus {
        current_block,
        expiry_block,
        blocks_remaining,
        days_remaining,
        urgency,
        last_checkin: None, // TODO: track this
    };

    let mut status_lock = state.policy_status.lock().unwrap();
    *status_lock = Some(status.clone());

    Ok(CommandResult::ok(status))
}

// ============================================================================
// Check-in Commands
// ============================================================================

/// Initiate a check-in (creates unsigned PSBT)
#[tauri::command]
pub async fn initiate_checkin(state: State<'_, AppState>) -> Result<CommandResult<String>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    // Check if we have inheritance config
    let config = {
        let config_lock = state.inheritance_config.lock().unwrap();
        match &*config_lock {
            Some(c) => c.clone(),
            None => return Ok(CommandResult::err("No inheritance policy configured. Please set up your policy first.")),
        }
    };

    // Get Electrum URL and network
    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    // Connect to Electrum
    let client = match ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => return Ok(CommandResult::err(format!("Failed to connect to Electrum: {}", e))),
    };

    // Parse the descriptor to get the script
    use miniscript::Descriptor;
    use miniscript::descriptor::DescriptorPublicKey;
    use std::str::FromStr;
    
    let descriptor: Descriptor<DescriptorPublicKey> = match Descriptor::from_str(&config.descriptor) {
        Ok(d) => d,
        Err(e) => return Ok(CommandResult::err(format!("Invalid descriptor: {}", e))),
    };

    // Get the script pubkey for the inheritance address (index 0)
    use miniscript::descriptor::DefiniteDescriptorKey;
    let derived: Descriptor<DefiniteDescriptorKey> = match descriptor.at_derivation_index(0) {
        Ok(d) => d,
        Err(e) => return Ok(CommandResult::err(format!("Failed to derive script: {}", e))),
    };
    let script = derived.script_pubkey();

    // Find UTXOs
    let utxos = match client.get_utxos_for_script(&script) {
        Ok(u) => u,
        Err(e) => return Ok(CommandResult::err(format!("Failed to get UTXOs: {}", e))),
    };

    if utxos.is_empty() {
        return Ok(CommandResult::err("No UTXOs found for inheritance address. Please deposit funds first."));
    }

    // Use the first UTXO for check-in
    let utxo = &utxos[0];

    // Build the check-in PSBT using nostring-inherit
    use nostring_inherit::checkin::{CheckinTxBuilder, InheritanceUtxo as InhUtxo};
    use bitcoin::ScriptBuf;

    let inheritance_utxo = InhUtxo::new(
        utxo.outpoint,
        utxo.value,
        utxo.height,
        ScriptBuf::from(script.to_owned()),
    );

    // Fee rate (sats/vbyte) - TODO: make configurable or estimate
    let fee_rate = 10;

    let builder = CheckinTxBuilder::new(inheritance_utxo, descriptor, fee_rate);
    
    match builder.build_psbt_base64() {
        Ok(psbt_base64) => Ok(CommandResult::ok(psbt_base64)),
        Err(e) => Ok(CommandResult::err(format!("Failed to build PSBT: {}", e))),
    }
}

/// Complete a check-in with signed PSBT
/// 
/// This is an alias for broadcast_signed_psbt - kept for API compatibility.
#[tauri::command]
pub async fn complete_checkin(
    signed_psbt: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<String>, ()> {
    // Delegate to broadcast_signed_psbt
    broadcast_signed_psbt(signed_psbt, state).await
}

/// Broadcast a signed PSBT (from QR scan)
#[tauri::command]
pub async fn broadcast_signed_psbt(
    signed_psbt: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<String>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    // 1. Decode base64 → PSBT bytes
    use base64::prelude::*;
    let psbt_bytes = match BASE64_STANDARD.decode(&signed_psbt) {
        Ok(b) => b,
        Err(e) => return Ok(CommandResult::err(format!("Invalid base64: {}", e))),
    };

    // 2. Parse PSBT (PSBT has its own deserialize method)
    let psbt: Psbt = match Psbt::deserialize(&psbt_bytes) {
        Ok(p) => p,
        Err(e) => return Ok(CommandResult::err(format!("Invalid PSBT: {}", e))),
    };

    // 3. Finalize PSBT → Transaction
    // In a real implementation, we'd use miniscript to finalize properly
    // For now, we assume the PSBT is already finalized with signatures
    let tx = match psbt.extract_tx() {
        Ok(t) => t,
        Err(e) => return Ok(CommandResult::err(format!("PSBT not fully signed: {}", e))),
    };

    // 4. Get Electrum client
    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    let client = match ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => return Ok(CommandResult::err(format!("Failed to connect to Electrum: {}", e))),
    };

    // 5. Broadcast transaction
    match client.broadcast(&tx) {
        Ok(txid) => {
            log::info!("Check-in broadcast successful: {}", txid);
            Ok(CommandResult::ok(txid.to_string()))
        }
        Err(e) => Ok(CommandResult::err(format!("Broadcast failed: {}", e))),
    }
}

// ============================================================================
// Settings Commands
// ============================================================================

/// Get Electrum server URL
#[tauri::command]
pub async fn get_electrum_url(state: State<'_, AppState>) -> Result<String, ()> {
    let url = state.electrum_url.lock().unwrap();
    Ok(url.clone())
}

/// Set Electrum server URL
#[tauri::command]
pub async fn set_electrum_url(url: String, state: State<'_, AppState>) -> Result<(), ()> {
    let mut electrum_url = state.electrum_url.lock().unwrap();
    *electrum_url = url;
    Ok(())
}
