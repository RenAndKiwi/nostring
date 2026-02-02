//! Tauri commands — the bridge between frontend and Rust backend
//!
//! All commands are async and return JSON-serializable results.

use crate::state::{AppState, PolicyStatus};
use nostring_core::seed::{generate_mnemonic, parse_mnemonic, derive_seed, WordCount};
use nostring_core::crypto::{encrypt_seed, decrypt_seed, EncryptedSeed};
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
pub async fn refresh_policy_status(state: State<'_, AppState>) -> Result<CommandResult<PolicyStatus>, ()> {
    // TODO: Connect to Electrum and fetch actual block height
    // For now, return mock data
    let mock_status = PolicyStatus {
        current_block: 880000,
        expiry_block: 906280, // ~6 months from now
        blocks_remaining: 26280,
        days_remaining: 182.5,
        urgency: "ok".to_string(),
        last_checkin: Some(1738400000),
    };

    let mut status_lock = state.policy_status.lock().unwrap();
    *status_lock = Some(mock_status.clone());

    Ok(CommandResult::ok(mock_status))
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

    // TODO: Build actual PSBT using nostring-inherit
    // For now, return placeholder
    Ok(CommandResult::ok("cHNidP8B...placeholder...".to_string()))
}

/// Complete a check-in with signed PSBT
#[tauri::command]
pub async fn complete_checkin(
    _signed_psbt: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<String>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }

    // TODO: Validate PSBT, extract transaction, broadcast via Electrum
    // For now, return placeholder txid
    Ok(CommandResult::ok("txid_placeholder_abc123".to_string()))
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

    // Validate PSBT format (should be base64 encoded)
    if !signed_psbt.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=') {
        return Ok(CommandResult::err("Invalid PSBT format"));
    }

    // TODO: Decode PSBT, validate signatures, finalize, and broadcast via Electrum
    // For now, simulate success with mock txid
    
    // In production:
    // 1. Decode base64 → PSBT bytes
    // 2. Parse PSBT using bitcoin crate
    // 3. Validate all inputs are signed
    // 4. Finalize PSBT → Transaction
    // 5. Broadcast via Electrum server
    
    let mock_txid = format!("check-in-{:016x}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs());
    
    Ok(CommandResult::ok(mock_txid))
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
