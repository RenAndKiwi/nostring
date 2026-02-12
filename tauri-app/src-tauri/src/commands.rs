//! Tauri commands — bridge between the frontend and Rust backend.
//!
//! Architecture: watch-only first. The recommended path imports an xpub
//! (no private keys). Seed import/create remain as advanced options.
//! All commands are async and return JSON-serializable results.
//!
//! Every mutation writes through to SQLite via `AppState` helpers so
//! state survives app restarts.

use crate::state::{AppState, PolicyStatus};
use bitcoin::psbt::Psbt;
use nostring_core::crypto::{decrypt_seed, encrypt_seed, EncryptedSeed};
use nostring_core::seed::{derive_seed, generate_mnemonic, parse_mnemonic, WordCount};
use nostring_electrum::ElectrumClient;
use serde::{Deserialize, Serialize};
use tauri::State;
use zeroize::Zeroize;

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

/// Check password strength before using it for seed encryption.
///
/// Returns entropy analysis with strength classification and warnings.
/// The frontend should call this as the user types to provide real-time feedback.
#[tauri::command]
pub async fn check_password_strength(password: String) -> CommandResult<PasswordStrengthResult> {
    let analysis = nostring_core::password::estimate_entropy(&password);
    CommandResult::ok(PasswordStrengthResult {
        entropy_bits: analysis.entropy_bits,
        strength: format!("{:?}", analysis.strength),
        description: analysis.strength.description().to_string(),
        meets_minimum: analysis.meets_minimum,
        warnings: analysis.warnings,
    })
}

/// Password strength analysis result
#[derive(Debug, Serialize, Deserialize)]
pub struct PasswordStrengthResult {
    pub entropy_bits: f64,
    pub strength: String,
    pub description: String,
    pub meets_minimum: bool,
    pub warnings: Vec<String>,
}

/// Import and encrypt a seed (persisted to SQLite)
#[tauri::command]
pub async fn import_seed(
    mut mnemonic: String,
    mut password: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<bool>, ()> {
    let parsed = match parse_mnemonic(&mnemonic) {
        Ok(m) => m,
        Err(e) => {
            // Zeroize sensitive inputs before returning
            mnemonic.zeroize();
            password.zeroize();
            return Ok(CommandResult::err(format!("Invalid mnemonic: {}", e)));
        }
    };

    // Zeroize the mnemonic string — we have the parsed form now
    mnemonic.zeroize();

    // derive_seed returns Zeroizing<[u8; 64]> — auto-zeroized on drop
    let seed = derive_seed(&parsed, "");

    let result = match encrypt_seed(&seed, &password) {
        Ok(encrypted) => {
            let encrypted_bytes = encrypted.to_bytes();

            // Write-through: memory + SQLite
            state.set_encrypted_seed(encrypted_bytes);
            state.set_watch_only(false);

            let mut unlocked = state.unlocked.lock().unwrap();
            *unlocked = true;

            Ok(CommandResult::ok(true))
        }
        Err(e) => Ok(CommandResult::err(format!("Failed to encrypt seed: {}", e))),
    };

    // Zeroize password after use (seed is auto-zeroized via Zeroizing wrapper)
    password.zeroize();

    result
}

/// Import a watch-only wallet (xpub only, no private keys).
///
/// This is the **recommended** setup path. Persisted to SQLite.
#[tauri::command]
pub async fn import_watch_only(
    xpub: String,
    _password: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<bool>, ()> {
    if !xpub.starts_with("xpub")
        && !xpub.starts_with("ypub")
        && !xpub.starts_with("zpub")
        && !xpub.starts_with("tpub")
        && !xpub.starts_with("[")
    {
        return Ok(CommandResult::err(
            "Invalid xpub format. Expected xpub, ypub, zpub, tpub, or descriptor.",
        ));
    }

    // Write-through: memory + SQLite
    state.set_owner_xpub(&xpub);
    state.set_watch_only(true);

    let mut unlocked = state.unlocked.lock().unwrap();
    *unlocked = true;

    Ok(CommandResult::ok(true))
}

/// Check if a wallet is configured (seed **or** watch-only xpub).
#[tauri::command]
pub async fn has_seed(state: State<'_, AppState>) -> Result<bool, ()> {
    let seed_lock = state.encrypted_seed.lock().unwrap();
    let xpub_lock = state.owner_xpub.lock().unwrap();
    Ok(seed_lock.is_some() || xpub_lock.is_some())
}

/// Unlock (decrypt) the seed with password
#[tauri::command]
pub async fn unlock_seed(
    mut password: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<bool>, ()> {
    let seed_lock = state.encrypted_seed.lock().unwrap();

    let result = match &*seed_lock {
        None => Ok(CommandResult::err("No seed loaded")),
        Some(encrypted_bytes) => {
            let encrypted = match EncryptedSeed::from_bytes(encrypted_bytes) {
                Ok(e) => e,
                Err(_) => return Ok(CommandResult::err("Corrupted seed data")),
            };

            match decrypt_seed(&encrypted, &password) {
                // Decrypted seed is wrapped in Zeroizing — auto-zeroed on drop
                Ok(_decrypted_seed) => {
                    drop(seed_lock);
                    let mut unlocked = state.unlocked.lock().unwrap();
                    *unlocked = true;
                    Ok(CommandResult::ok(true))
                }
                Err(_) => Ok(CommandResult::err("Incorrect password")),
            }
        }
    };

    // Zeroize password after use
    password.zeroize();

    result
}

/// Lock the wallet (clear unlocked state — ephemeral only, no DB change)
#[tauri::command]
pub async fn lock_wallet(state: State<'_, AppState>) -> Result<(), ()> {
    let mut unlocked = state.unlocked.lock().unwrap();
    *unlocked = false;
    Ok(())
}

// ============================================================================
// Service Key Commands (Notification Identity)
// ============================================================================

/// Generate a random Nostr keypair for sending check-in reminders.
/// Persisted to SQLite so it survives restarts.
#[tauri::command]
pub async fn generate_service_key(state: State<'_, AppState>) -> Result<CommandResult<String>, ()> {
    use nostr_sdk::prelude::*;

    let keys = Keys::generate();
    let secret_hex = keys.secret_key().to_secret_hex();
    let npub = keys.public_key().to_bech32().unwrap_or_default();

    // Write-through: memory + SQLite
    state.set_service_key(&secret_hex, &npub);

    Ok(CommandResult::ok(npub))
}

/// Get the service key's npub.
#[tauri::command]
pub async fn get_service_npub(state: State<'_, AppState>) -> Result<Option<String>, ()> {
    let npub = state.service_npub.lock().unwrap();
    Ok(npub.clone())
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
    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    let client = match ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Failed to connect to Electrum: {}",
                e
            )))
        }
    };

    let current_block = match client.get_height() {
        Ok(h) => h as u64,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Failed to get block height: {}",
                e
            )))
        }
    };

    let config_lock = state.inheritance_config.lock().unwrap();
    let (expiry_block, blocks_remaining, days_remaining, urgency) =
        if let Some(config) = &*config_lock {
            let timelock = config.timelock_blocks as u64;
            let expiry = current_block + timelock;
            let remaining = expiry.saturating_sub(current_block) as i64;
            let days = remaining as f64 * 10.0 / 60.0 / 24.0;

            let urgency = if remaining > 4320 {
                "ok"
            } else if remaining > 1008 {
                "warning"
            } else {
                "critical"
            };

            (expiry, remaining, days, urgency.to_string())
        } else {
            (current_block + 26280, 26280, 182.5, "ok".to_string())
        };
    drop(config_lock);

    // Get last check-in from DB
    let last_checkin = {
        let conn = state.db.lock().unwrap();
        crate::db::checkin_last(&conn).ok().flatten()
    };

    let status = PolicyStatus {
        current_block,
        expiry_block,
        blocks_remaining,
        days_remaining,
        urgency,
        last_checkin,
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

    let config = {
        let config_lock = state.inheritance_config.lock().unwrap();
        match &*config_lock {
            Some(c) => c.clone(),
            None => {
                return Ok(CommandResult::err(
                    "No heirs configured yet. Add at least one heir in the Heirs tab to create your inheritance policy.",
                ))
            }
        }
    };

    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    let client = match ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Failed to connect to Electrum: {}",
                e
            )))
        }
    };

    use miniscript::descriptor::DescriptorPublicKey;
    use miniscript::Descriptor;
    use std::str::FromStr;

    let descriptor: Descriptor<DescriptorPublicKey> = match Descriptor::from_str(&config.descriptor)
    {
        Ok(d) => d,
        Err(e) => return Ok(CommandResult::err(format!("Invalid descriptor: {}", e))),
    };

    use miniscript::descriptor::DefiniteDescriptorKey;
    let derived: Descriptor<DefiniteDescriptorKey> = match descriptor.at_derivation_index(0) {
        Ok(d) => d,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Failed to derive script: {}",
                e
            )))
        }
    };
    let script = derived.script_pubkey();

    let utxos = match client.get_utxos_for_script(&script) {
        Ok(u) => u,
        Err(e) => return Ok(CommandResult::err(format!("Failed to get UTXOs: {}", e))),
    };

    if utxos.is_empty() {
        return Ok(CommandResult::err(
            "No UTXOs found for inheritance address. Please deposit funds first.",
        ));
    }

    let utxo = &utxos[0];

    use nostring_inherit::checkin::{CheckinTxBuilder, InheritanceUtxo as InhUtxo};

    let inheritance_utxo = InhUtxo::new(utxo.outpoint, utxo.value, utxo.height, script.to_owned());

    let fee_rate = 10;
    let builder = CheckinTxBuilder::new(inheritance_utxo, descriptor, fee_rate, 0);

    match builder.build_psbt_base64() {
        Ok(psbt_base64) => Ok(CommandResult::ok(psbt_base64)),
        Err(e) => Ok(CommandResult::err(format!("Failed to build PSBT: {}", e))),
    }
}

/// Complete a check-in with signed PSBT
#[tauri::command]
pub async fn complete_checkin(
    signed_psbt: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<String>, ()> {
    broadcast_signed_psbt(signed_psbt, state).await
}

/// Broadcast a signed PSBT and log the check-in
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

    use base64::prelude::*;
    let psbt_bytes = match BASE64_STANDARD.decode(&signed_psbt) {
        Ok(b) => b,
        Err(e) => return Ok(CommandResult::err(format!("Invalid base64: {}", e))),
    };

    let psbt: Psbt = match Psbt::deserialize(&psbt_bytes) {
        Ok(p) => p,
        Err(e) => return Ok(CommandResult::err(format!("Invalid PSBT: {}", e))),
    };

    let tx = match psbt.extract_tx() {
        Ok(t) => t,
        Err(e) => return Ok(CommandResult::err(format!("PSBT not fully signed: {}", e))),
    };

    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    let client = match ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Failed to connect to Electrum: {}",
                e
            )))
        }
    };

    match client.broadcast(&tx) {
        Ok(txid) => {
            log::info!("Check-in broadcast successful: {}", txid);

            // Log the check-in to SQLite
            state.log_checkin(&txid.to_string());

            // Invalidate all pre-signed check-ins — manual check-in
            // spends the UTXO they were built to spend
            {
                let conn = state.db.lock().unwrap();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let invalidated = crate::db::presigned_checkin_invalidate_all(
                    &conn,
                    now,
                    "Manual check-in broadcast — UTXO spent",
                );
                if let Ok(count) = invalidated {
                    if count > 0 {
                        log::info!(
                            "Invalidated {} pre-signed check-ins after manual check-in",
                            count
                        );
                    }
                }
            }

            Ok(CommandResult::ok(txid.to_string()))
        }
        Err(e) => Ok(CommandResult::err(format!("Broadcast failed: {}", e))),
    }
}

// ============================================================================
// Spend Type Detection Commands
// ============================================================================

/// Spend event info for the frontend
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpendEventInfo {
    pub id: i64,
    pub timestamp: u64,
    pub txid: String,
    pub spend_type: String,
    pub confidence: f64,
    pub method: String,
    pub policy_id: Option<String>,
    pub outpoint: Option<String>,
}

/// Detect the spend type of a transaction by analyzing its witness data.
///
/// Fetches the transaction via Electrum and analyzes the witness to determine
/// whether the owner or heir spent the funds.
#[tauri::command]
pub async fn detect_spend_type(
    txid: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<SpendEventInfo>, ()> {
    use nostring_watch::spend_analysis;
    use std::str::FromStr;

    let tx_id = match bitcoin::Txid::from_str(&txid) {
        Ok(t) => t,
        Err(e) => return Ok(CommandResult::err(format!("Invalid txid: {}", e))),
    };

    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    let client = match ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Failed to connect to Electrum: {}",
                e
            )))
        }
    };

    // Fetch the transaction
    let tx = match client.get_transaction(&tx_id) {
        Ok(t) => t,
        Err(e) => return Ok(CommandResult::err(format!("Transaction not found: {}", e))),
    };

    // Analyze the first input's witness (the one that spends the inheritance UTXO)
    let analysis = if !tx.input.is_empty() {
        spend_analysis::analyze_witness(&tx.input[0].witness)
    } else {
        return Ok(CommandResult::err("Transaction has no inputs"));
    };

    let spend_type_str = match analysis.spend_type {
        nostring_watch::SpendType::OwnerCheckin => "owner_checkin",
        nostring_watch::SpendType::HeirClaim => "heir_claim",
        nostring_watch::SpendType::Unknown => "unknown",
    };

    let method_str = match analysis.method {
        spend_analysis::DetectionMethod::WitnessAnalysis => "witness_analysis",
        spend_analysis::DetectionMethod::TimelockTiming => "timelock_timing",
        spend_analysis::DetectionMethod::Indeterminate => "indeterminate",
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Log the spend event to DB
    {
        let conn = state.db.lock().unwrap();
        let _ = crate::db::spend_event_insert(
            &conn,
            now,
            &txid,
            spend_type_str,
            analysis.confidence,
            method_str,
            None,
            None,
        );
    }

    Ok(CommandResult::ok(SpendEventInfo {
        id: 0,
        timestamp: now,
        txid,
        spend_type: spend_type_str.to_string(),
        confidence: analysis.confidence,
        method: method_str.to_string(),
        policy_id: None,
        outpoint: None,
    }))
}

/// Get all spend events from the database.
#[tauri::command]
pub async fn get_spend_events(state: State<'_, AppState>) -> Result<Vec<SpendEventInfo>, ()> {
    let conn = state.db.lock().unwrap();
    let rows = crate::db::spend_event_list(&conn).unwrap_or_default();

    Ok(rows
        .into_iter()
        .map(|r| SpendEventInfo {
            id: r.id,
            timestamp: r.timestamp,
            txid: r.txid,
            spend_type: r.spend_type,
            confidence: r.confidence,
            method: r.method,
            policy_id: r.policy_id,
            outpoint: r.outpoint,
        })
        .collect())
}

/// Check if any heir claims have been detected (for alert display).
#[tauri::command]
pub async fn check_heir_claims(state: State<'_, AppState>) -> Result<bool, ()> {
    let conn = state.db.lock().unwrap();
    Ok(crate::db::has_heir_claims(&conn).unwrap_or(false))
}

// ============================================================================
// Settings Commands
// ============================================================================

/// Get the current Bitcoin network as a string.
#[tauri::command]
pub async fn get_network(state: State<'_, AppState>) -> Result<String, ()> {
    let network = *state.network.lock().unwrap();
    let s = match network {
        bitcoin::Network::Bitcoin => "bitcoin",
        bitcoin::Network::Testnet => "testnet",
        bitcoin::Network::Signet => "signet",
        bitcoin::Network::Regtest => "regtest",
        _ => "bitcoin",
    };
    Ok(s.to_string())
}

/// Set the Bitcoin network. Validates input, updates state, persists to SQLite,
/// and auto-sets the default Electrum server URL for the chosen network.
#[tauri::command]
pub async fn set_network(
    network: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<String>, ()> {
    let net = match network.as_str() {
        "bitcoin" | "mainnet" => bitcoin::Network::Bitcoin,
        "testnet" | "testnet3" => bitcoin::Network::Testnet,
        "signet" => bitcoin::Network::Signet,
        "regtest" => bitcoin::Network::Regtest,
        _ => {
            return Ok(CommandResult::err(format!(
                "Invalid network '{}'. Use: bitcoin, testnet, signet, regtest",
                network
            )))
        }
    };

    // Write-through: memory + SQLite
    state.set_network(net);

    // Auto-set default Electrum URL for the network
    let default_url = nostring_electrum::default_server(net);
    state.set_electrum_url(default_url);

    let label = match net {
        bitcoin::Network::Bitcoin => "bitcoin",
        bitcoin::Network::Testnet => "testnet",
        bitcoin::Network::Signet => "signet",
        bitcoin::Network::Regtest => "regtest",
        _ => "bitcoin",
    };
    log::info!("Network switched to {} (Electrum: {})", label, default_url);

    Ok(CommandResult::ok(label.to_string()))
}

/// Get Electrum server URL
#[tauri::command]
pub async fn get_electrum_url(state: State<'_, AppState>) -> Result<String, ()> {
    let url = state.electrum_url.lock().unwrap();
    Ok(url.clone())
}

/// Set Electrum server URL (persisted to SQLite)
#[tauri::command]
pub async fn set_electrum_url(url: String, state: State<'_, AppState>) -> Result<(), ()> {
    state.set_electrum_url(&url);
    Ok(())
}

// ============================================================================
// Heir Management Commands
// ============================================================================

use bitcoin::bip32::{DerivationPath, Fingerprint, Xpub};
use nostring_inherit::heir::HeirKey;
use std::str::FromStr;

/// Serializable heir info for frontend
#[derive(Debug, Serialize, Deserialize)]
pub struct HeirInfo {
    pub label: String,
    pub fingerprint: String,
    pub xpub: String,
    pub derivation_path: String,
    /// Nostr npub for descriptor delivery (optional, v0.2)
    pub npub: Option<String>,
    /// Email address for descriptor delivery (optional, v0.2)
    pub email: Option<String>,
    /// Per-heir timelock in months (optional, v0.4)
    pub timelock_months: Option<u32>,
}

impl From<&HeirKey> for HeirInfo {
    fn from(heir: &HeirKey) -> Self {
        Self {
            label: heir.label.clone(),
            fingerprint: heir.fingerprint.to_string(),
            xpub: heir.xpub.to_string(),
            derivation_path: heir.derivation_path.to_string(),
            npub: None,
            email: None,
            timelock_months: None,
        }
    }
}

impl HeirInfo {
    /// Create from HeirKey + contact info + timelock from DB
    fn from_key_with_contact(
        heir: &HeirKey,
        npub: Option<String>,
        email: Option<String>,
        timelock_months: Option<u32>,
    ) -> Self {
        Self {
            label: heir.label.clone(),
            fingerprint: heir.fingerprint.to_string(),
            xpub: heir.xpub.to_string(),
            derivation_path: heir.derivation_path.to_string(),
            npub,
            email,
            timelock_months,
        }
    }
}

/// Add a new heir (persisted to SQLite)
#[tauri::command]
pub async fn add_heir(
    label: String,
    xpub_or_descriptor: String,
    timelock_months: Option<u32>,
    npub: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandResult<HeirInfo>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    let heir = if xpub_or_descriptor.starts_with('[') {
        match HeirKey::from_descriptor_str(&label, &xpub_or_descriptor) {
            Ok(h) => h,
            Err(e) => return Ok(CommandResult::err(format!("Invalid descriptor: {}", e))),
        }
    } else {
        let xpub = match Xpub::from_str(&xpub_or_descriptor) {
            Ok(x) => x,
            Err(e) => return Ok(CommandResult::err(format!("Invalid xpub: {}", e))),
        };

        let fingerprint = xpub.fingerprint();
        let derivation_path = DerivationPath::from_str("m/84'/0'/0'").unwrap();

        HeirKey::new(&label, fingerprint, xpub, Some(derivation_path))
    };

    let mut heir_info = HeirInfo::from(&heir);
    heir_info.timelock_months = timelock_months;

    // Write-through: memory + SQLite
    state.persist_heir(&heir, timelock_months);
    let fp = heir.fingerprint.to_string();
    let mut registry = state.heir_registry.lock().unwrap();
    registry.add(heir);
    drop(registry);

    // Persist npub if provided
    if let Some(ref n) = npub {
        if !n.is_empty() {
            state.update_heir_contact(&fp, Some(n), None);
            heir_info.npub = Some(n.clone());
        }
    }

    Ok(CommandResult::ok(heir_info))
}

/// List all heirs (with contact info from DB)
#[tauri::command]
pub async fn list_heirs(state: State<'_, AppState>) -> Result<Vec<HeirInfo>, ()> {
    let registry = state.heir_registry.lock().unwrap();
    let conn = state.db.lock().unwrap();

    let heirs: Vec<HeirInfo> = registry
        .list()
        .iter()
        .map(|heir| {
            let fp = heir.fingerprint.to_string();
            let row = crate::db::heir_get(&conn, &fp).ok().flatten();
            let (npub, email, timelock_months) = row
                .map(|r| (r.npub, r.email, r.timelock_months))
                .unwrap_or((None, None, None));
            HeirInfo::from_key_with_contact(heir, npub, email, timelock_months)
        })
        .collect();

    drop(conn);
    Ok(heirs)
}

/// Remove an heir by fingerprint (persisted to SQLite)
#[tauri::command]
pub async fn remove_heir(
    fingerprint: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<bool>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    let fp = match Fingerprint::from_str(&fingerprint) {
        Ok(f) => f,
        Err(e) => return Ok(CommandResult::err(format!("Invalid fingerprint: {}", e))),
    };

    // Write-through: memory + SQLite
    state.remove_heir_db(&fingerprint);
    let mut registry = state.heir_registry.lock().unwrap();
    match registry.remove(&fp) {
        Some(_) => Ok(CommandResult::ok(true)),
        None => Ok(CommandResult::err("Heir not found")),
    }
}

/// Get a single heir by fingerprint
#[tauri::command]
pub async fn get_heir(
    fingerprint: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<HeirInfo>, ()> {
    let fp = match Fingerprint::from_str(&fingerprint) {
        Ok(f) => f,
        Err(e) => return Ok(CommandResult::err(format!("Invalid fingerprint: {}", e))),
    };

    let registry = state.heir_registry.lock().unwrap();
    match registry.get(&fp) {
        Some(heir) => Ok(CommandResult::ok(HeirInfo::from(heir))),
        None => Ok(CommandResult::err("Heir not found")),
    }
}

/// Set contact info (npub and/or email) for an heir, used for descriptor delivery.
#[tauri::command]
pub async fn set_heir_contact(
    fingerprint: String,
    npub: Option<String>,
    email: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandResult<bool>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    // Validate npub format if provided
    if let Some(ref npub_str) = npub {
        if !npub_str.starts_with("npub1") {
            return Ok(CommandResult::err(
                "Invalid npub format. Must start with 'npub1'.",
            ));
        }
        if nostr_sdk::prelude::PublicKey::parse(npub_str).is_err() {
            return Ok(CommandResult::err("Invalid npub: failed to decode."));
        }
    }

    // Validate email format if provided (basic check)
    if let Some(ref email_str) = email {
        if !email_str.contains('@') || !email_str.contains('.') {
            return Ok(CommandResult::err("Invalid email format."));
        }
    }

    let updated = state.update_heir_contact(&fingerprint, npub.as_deref(), email.as_deref());

    if updated {
        Ok(CommandResult::ok(true))
    } else {
        Ok(CommandResult::err("Heir not found with that fingerprint."))
    }
}

/// Get contact info for an heir.
#[tauri::command]
pub async fn get_heir_contact(
    fingerprint: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<HeirContactInfo>, ()> {
    let conn = state.db.lock().unwrap();
    let row = crate::db::heir_get(&conn, &fingerprint).ok().flatten();
    drop(conn);

    match row {
        Some(r) => Ok(CommandResult::ok(HeirContactInfo {
            fingerprint: r.fingerprint,
            label: r.label,
            npub: r.npub,
            email: r.email,
        })),
        None => Ok(CommandResult::err("Heir not found")),
    }
}

/// Contact info for an heir
#[derive(Debug, Serialize, Deserialize)]
pub struct HeirContactInfo {
    pub fingerprint: String,
    pub label: String,
    pub npub: Option<String>,
    pub email: Option<String>,
}

/// Validate an xpub string
#[tauri::command]
pub async fn validate_xpub(xpub: String) -> CommandResult<bool> {
    if xpub.starts_with('[') {
        match HeirKey::from_descriptor_str("test", &xpub) {
            Ok(_) => return CommandResult::ok(true),
            Err(e) => return CommandResult::err(format!("Invalid descriptor: {}", e)),
        }
    }

    match Xpub::from_str(&xpub) {
        Ok(_) => CommandResult::ok(true),
        Err(e) => CommandResult::err(format!("Invalid xpub: {}", e)),
    }
}

// ============================================================================
// Shamir Share Commands
// ============================================================================

use nostring_shamir::codex32::{parse_share, Codex32Config, Codex32Share};

// ============================================================================
// nsec Shamir Inheritance Commands
// ============================================================================

/// Per-heir share info returned to the frontend.
#[derive(Debug, Serialize, Deserialize)]
pub struct HeirShareInfo {
    pub heir_label: String,
    pub heir_fingerprint: String,
    pub share: String,
}

/// Result of splitting an nsec.
#[derive(Debug, Serialize, Deserialize)]
pub struct NsecSplitResult {
    /// The owner's npub (so heirs know which identity they're recovering)
    pub owner_npub: String,
    /// One share per heir — give to each heir for safekeeping
    pub pre_distributed: Vec<HeirShareInfo>,
    /// Locked shares — included in the descriptor backup
    pub locked_shares: Vec<String>,
    /// Threshold needed to reconstruct
    pub threshold: u8,
    /// Total shares generated
    pub total_shares: u8,
    /// Whether this was a re-split (previous config existed)
    pub was_resplit: bool,
    /// The previous npub if re-splitting (may differ if owner changed identity)
    pub previous_npub: Option<String>,
}

/// Revoke nsec inheritance — clears locked shares and owner npub.
///
/// After revocation, old pre-distributed shares are useless (they can't
/// reconstruct without the locked shares). The owner must re-split if
/// they want to set up inheritance again.
#[tauri::command]
pub async fn revoke_nsec_inheritance(
    state: State<'_, AppState>,
) -> Result<CommandResult<bool>, ()> {
    // Require wallet to be unlocked
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked. Unlock first."));
    }
    drop(unlocked);

    // Clear nsec inheritance data from SQLite
    state.delete_config("nsec_locked_shares");
    state.delete_config("nsec_owner_npub");

    log::info!("nsec inheritance revoked — locked shares and owner npub cleared");

    Ok(CommandResult::ok(true))
}

/// Split an nsec into Shamir shares for identity inheritance.
///
/// The nsec is held in memory ONLY during this call, then zeroed.
/// Pre-distributed shares go to heirs (1 each).
/// Locked shares go into the descriptor backup.
///
/// If inheritance is already configured, this acts as a **re-split**:
/// old locked shares are replaced and old pre-distributed shares become
/// useless.  The caller is warned via the `was_resplit` field.
///
/// Formula: N heirs → (N+1)-of-(2N+1) split
///   - Pre-distributed: N (one per heir)
///   - Locked: N+1
///   - All heirs colluding have N shares but need N+1 → blocked
///   - After inheritance: heir has 1 + (N+1) locked = N+2 > threshold ✓
#[tauri::command]
pub async fn split_nsec(
    nsec_input: String,
    state: State<'_, AppState>,
) -> Result<CommandResult<NsecSplitResult>, ()> {
    use nostr_sdk::prelude::*;
    use nostring_shamir::codex32::generate_shares;

    // Require wallet to be unlocked
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked. Unlock first."));
    }
    drop(unlocked);

    // Detect existing nsec inheritance (re-split scenario)
    let previous_npub = {
        let conn = state.db.lock().unwrap();
        crate::db::config_get(&conn, "nsec_owner_npub")
            .ok()
            .flatten()
    };
    let was_resplit = previous_npub.is_some();

    if was_resplit {
        log::info!(
            "Re-splitting nsec inheritance (previous npub: {})",
            previous_npub.as_deref().unwrap_or("unknown")
        );
    }

    // Parse and validate the nsec
    let keys = match Keys::parse(&nsec_input) {
        Ok(k) => k,
        Err(e) => return Ok(CommandResult::err(format!("Invalid nsec: {}", e))),
    };

    let owner_npub = keys.public_key().to_bech32().unwrap_or_default();

    // Get the raw 32-byte secret
    let mut secret_bytes = keys.secret_key().as_secret_bytes().to_vec();

    // Count heirs
    let heir_count = {
        let registry = state.heir_registry.lock().unwrap();
        registry.list().len()
    };

    if heir_count == 0 {
        secret_bytes.zeroize();
        return Ok(CommandResult::err(
            "Add at least one heir before splitting your nsec.",
        ));
    }

    let n = heir_count as u8;
    let threshold = n + 1;
    let total_shares = 2 * n + 1;

    // Validate Codex32 limits (threshold 2-9, total ≤ 31)
    if threshold > 9 {
        secret_bytes.zeroize();
        return Ok(CommandResult::err(
            "Too many heirs for Codex32 (max 8 heirs → threshold 9). Use fewer heirs or contact support.",
        ));
    }

    // Generate Codex32 shares
    // Use lowercase bech32-safe identifier
    let config = match Codex32Config::new(threshold, "nsec", total_shares) {
        Ok(c) => c,
        Err(e) => {
            secret_bytes.zeroize();
            return Ok(CommandResult::err(format!("Shamir config error: {}", e)));
        }
    };

    let shares = match generate_shares(&secret_bytes, &config) {
        Ok(s) => s,
        Err(e) => {
            secret_bytes.zeroize();
            return Ok(CommandResult::err(format!(
                "Share generation failed: {}",
                e
            )));
        }
    };

    // ZERO the raw nsec from memory immediately
    secret_bytes.zeroize();

    // Split into pre-distributed (first N) and locked (remaining N+1)
    let heir_labels: Vec<(String, String)> = {
        let registry = state.heir_registry.lock().unwrap();
        registry
            .list()
            .iter()
            .map(|h| (h.label.clone(), h.fingerprint.to_string()))
            .collect()
    };

    let pre_distributed: Vec<HeirShareInfo> = heir_labels
        .iter()
        .enumerate()
        .map(|(i, (label, fp))| HeirShareInfo {
            heir_label: label.clone(),
            heir_fingerprint: fp.clone(),
            share: shares[i].encoded.clone(),
        })
        .collect();

    let locked_shares: Vec<String> = shares[n as usize..]
        .iter()
        .map(|s| s.encoded.clone())
        .collect();

    // Persist locked shares + owner npub to SQLite
    // (locked shares alone can't reconstruct — they need heir shares too...
    //  unless threshold equals locked count, which is the resilience design)
    let locked_json = serde_json::to_string(&locked_shares).unwrap_or_default();
    state.persist_config("nsec_locked_shares", &locked_json);
    state.persist_config("nsec_owner_npub", &owner_npub);

    if was_resplit {
        log::info!("nsec re-split complete — old shares are now invalid");
    }

    Ok(CommandResult::ok(NsecSplitResult {
        owner_npub,
        pre_distributed,
        locked_shares,
        threshold,
        total_shares,
        was_resplit,
        previous_npub,
    }))
}

/// Get nsec inheritance status (is it configured? what npub?).
#[derive(Debug, Serialize, Deserialize)]
pub struct NsecInheritanceStatus {
    pub configured: bool,
    pub owner_npub: Option<String>,
    pub locked_share_count: usize,
}

#[tauri::command]
pub async fn get_nsec_inheritance_status(
    state: State<'_, AppState>,
) -> Result<NsecInheritanceStatus, ()> {
    let conn = state.db.lock().unwrap();
    let owner_npub = crate::db::config_get(&conn, "nsec_owner_npub")
        .ok()
        .flatten();
    let locked_json = crate::db::config_get(&conn, "nsec_locked_shares")
        .ok()
        .flatten();
    drop(conn);

    let locked_count = locked_json
        .as_ref()
        .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
        .map(|v| v.len())
        .unwrap_or(0);

    Ok(NsecInheritanceStatus {
        configured: owner_npub.is_some() && locked_count > 0,
        owner_npub,
        locked_share_count: locked_count,
    })
}

/// Get locked shares (for inclusion in descriptor backup).
#[tauri::command]
pub async fn get_locked_shares(state: State<'_, AppState>) -> Result<Option<Vec<String>>, ()> {
    let conn = state.db.lock().unwrap();
    let locked_json = crate::db::config_get(&conn, "nsec_locked_shares")
        .ok()
        .flatten();
    drop(conn);

    Ok(locked_json.and_then(|j| serde_json::from_str(&j).ok()))
}

/// Recover an nsec from Shamir shares (heir recovery tool).
///
/// The heir pastes their pre-distributed share(s) plus locked shares
/// from the descriptor backup. If threshold is met, the nsec is revealed.
#[tauri::command]
pub async fn recover_nsec(shares: Vec<String>) -> CommandResult<RecoveredNsec> {
    use nostring_shamir::codex32::combine_shares;

    if shares.len() < 2 {
        return CommandResult::err("Need at least 2 shares to recover.");
    }

    // Parse all shares
    let mut parsed: Vec<Codex32Share> = Vec::new();
    for (i, share_str) in shares.iter().enumerate() {
        match parse_share(share_str) {
            Ok(s) => parsed.push(s),
            Err(e) => return CommandResult::err(format!("Invalid share #{}: {}", i + 1, e)),
        }
    }

    // Attempt reconstruction
    let mut recovered_bytes = match combine_shares(&parsed) {
        Ok(bytes) => bytes,
        Err(e) => {
            return CommandResult::err(format!(
                "Could not reconstruct. Need more shares or shares are from different splits. Error: {}",
                e
            ))
        }
    };

    // Verify it's a valid Nostr secret key
    let recovered_hex = hex::encode(&recovered_bytes);
    let keys = match nostr_sdk::prelude::Keys::parse(&recovered_hex) {
        Ok(k) => k,
        Err(e) => {
            recovered_bytes.zeroize();
            return CommandResult::err(format!(
                "Shares reconstructed but result is not a valid Nostr key: {}",
                e
            ));
        }
    };

    use nostr_sdk::ToBech32;
    let nsec = keys
        .secret_key()
        .to_bech32()
        .unwrap_or_else(|_| recovered_hex.clone());
    let npub = keys.public_key().to_bech32().unwrap_or_default();

    // Zero the intermediate bytes
    recovered_bytes.zeroize();

    CommandResult::ok(RecoveredNsec { nsec, npub })
}

/// Recovered nsec result.
#[derive(Debug, Serialize, Deserialize)]
pub struct RecoveredNsec {
    /// The recovered nsec (bech32)
    pub nsec: String,
    /// The corresponding npub (for verification)
    pub npub: String,
}

// ============================================================================
// Notification Commands
// ============================================================================

/// Configure notification settings for the owner's npub/email.
///
/// The service key (generated in `generate_service_key`) sends DMs.
/// The `owner_npub` is who receives them.
#[tauri::command]
pub async fn configure_notifications(
    owner_npub: Option<String>,
    email_address: Option<String>,
    email_smtp_host: Option<String>,
    email_smtp_user: Option<String>,
    email_smtp_password: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandResult<bool>, ()> {
    // Persist notification settings
    if let Some(ref npub) = owner_npub {
        state.persist_config("notify_owner_npub", npub);
    }
    if let Some(ref email) = email_address {
        state.persist_config("notify_email_address", email);
    }
    if let Some(ref host) = email_smtp_host {
        state.persist_config("notify_email_smtp_host", host);
    }
    if let Some(ref user) = email_smtp_user {
        state.persist_config("notify_email_smtp_user", user);
    }
    if let Some(ref pass) = email_smtp_password {
        state.persist_config("notify_email_smtp_password", pass);
    }

    Ok(CommandResult::ok(true))
}

/// Get current notification settings.
#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub owner_npub: Option<String>,
    pub email_address: Option<String>,
    pub email_smtp_host: Option<String>,
    pub service_npub: Option<String>,
}

#[tauri::command]
pub async fn get_notification_settings(
    state: State<'_, AppState>,
) -> Result<NotificationSettings, ()> {
    let conn = state.db.lock().unwrap();
    let owner_npub = crate::db::config_get(&conn, "notify_owner_npub")
        .ok()
        .flatten();
    let email_address = crate::db::config_get(&conn, "notify_email_address")
        .ok()
        .flatten();
    let email_smtp_host = crate::db::config_get(&conn, "notify_email_smtp_host")
        .ok()
        .flatten();
    drop(conn);
    let service_npub = state.service_npub.lock().unwrap().clone();

    Ok(NotificationSettings {
        owner_npub,
        email_address,
        email_smtp_host,
        service_npub,
    })
}

/// Send a test notification via Nostr DM and/or email.
///
/// Uses the service key to send a DM to the owner's npub.
#[tauri::command]
pub async fn send_test_notification(
    state: State<'_, AppState>,
) -> Result<CommandResult<String>, ()> {
    // Get the service key (sender)
    let service_secret = {
        let sk = state.service_key.lock().unwrap();
        match &*sk {
            Some(s) => s.clone(),
            None => {
                return Ok(CommandResult::err(
                    "No service key generated. Go to Settings → Notifications to set up.",
                ))
            }
        }
    };

    // Get the owner's npub (recipient)
    let owner_npub = {
        let conn = state.db.lock().unwrap();
        crate::db::config_get(&conn, "notify_owner_npub")
            .ok()
            .flatten()
    };

    let Some(owner_npub) = owner_npub else {
        return Ok(CommandResult::err(
            "No owner npub configured. Enter your Nostr npub in Settings → Notifications.",
        ));
    };

    // Build the nostr config
    let nostr_config = nostring_notify::NostrConfig {
        enabled: true,
        recipient_pubkey: owner_npub,
        relays: vec![
            "wss://relay.damus.io".into(),
            "wss://relay.nostr.band".into(),
            "wss://nos.lol".into(),
        ],
        secret_key: Some(service_secret),
    };

    // Create a test message
    let test_msg = nostring_notify::NotificationLevel::Reminder;
    let message = nostring_notify::templates::generate_message(test_msg, 30.0, 4320, 0);

    // Send it
    match nostring_notify::nostr_dm::send_dm(&nostr_config, &message).await {
        Ok(_) => Ok(CommandResult::ok(
            "Test DM sent! Check your Nostr client.".to_string(),
        )),
        Err(e) => Ok(CommandResult::err(format!("Failed to send DM: {}", e))),
    }
}

/// Check the inheritance timelock and send notifications if thresholds are hit.
///
/// This should be called periodically (e.g., on app open, on refresh).
///
/// **Escalation logic (v0.2):**
/// - Warning/Reminder levels → notify the OWNER only (existing behavior)
/// - Critical level (timelock expired or <1 day) → deliver descriptor backup
///   to HEIRS via their configured npub/email channels
///
/// Rate limiting: heirs won't be spammed — a 24h cooldown prevents re-delivery.
#[tauri::command]
pub async fn check_and_notify(state: State<'_, AppState>) -> Result<CommandResult<String>, ()> {
    // Need policy status
    let status = {
        let s = state.policy_status.lock().unwrap();
        match &*s {
            Some(st) => st.clone(),
            None => {
                return Ok(CommandResult::err(
                    "No policy status. Refresh status first.",
                ))
            }
        }
    };

    // Get service key
    let service_secret = {
        let sk = state.service_key.lock().unwrap();
        match &*sk {
            Some(s) => s.clone(),
            None => {
                return Ok(CommandResult::ok(
                    "No service key — skipping notifications.".to_string(),
                ))
            }
        }
    };

    // Get owner npub
    let owner_npub = {
        let conn = state.db.lock().unwrap();
        crate::db::config_get(&conn, "notify_owner_npub")
            .ok()
            .flatten()
    };

    // Build notification config
    let nostr_config = owner_npub.map(|npub| nostring_notify::NostrConfig {
        enabled: true,
        recipient_pubkey: npub,
        relays: vec![
            "wss://relay.damus.io".into(),
            "wss://relay.nostr.band".into(),
            "wss://nos.lol".into(),
        ],
        secret_key: Some(service_secret.clone()),
    });

    // Get email config
    let email_config = {
        let conn = state.db.lock().unwrap();
        let address = crate::db::config_get(&conn, "notify_email_address")
            .ok()
            .flatten();
        let host = crate::db::config_get(&conn, "notify_email_smtp_host")
            .ok()
            .flatten();
        let user = crate::db::config_get(&conn, "notify_email_smtp_user")
            .ok()
            .flatten();
        let pass = crate::db::config_get(&conn, "notify_email_smtp_password")
            .ok()
            .flatten();
        match (address, host, user, pass) {
            (Some(addr), Some(h), Some(u), Some(p)) => Some(nostring_notify::EmailConfig {
                enabled: true,
                smtp_host: h,
                smtp_port: 587,
                smtp_user: u.clone(),
                smtp_password: p,
                from_address: u,
                to_address: addr,
                plaintext: false,
            }),
            _ => None,
        }
    };

    let mut results = Vec::new();

    // ── Phase 1: Owner notifications (existing behavior) ──
    if nostr_config.is_some() || email_config.is_some() {
        let config = nostring_notify::NotifyConfig {
            thresholds: nostring_notify::NotifyConfig::default().thresholds,
            email: email_config.clone(),
            nostr: nostr_config,
        };

        let service = nostring_notify::NotificationService::new(config);

        match service
            .check_and_notify(status.blocks_remaining, status.current_block as u32)
            .await
        {
            Ok(Some(level)) => results.push(format!("Owner notification sent: {:?}", level)),
            Ok(None) => {
                results.push("No owner notification needed — timelock healthy.".to_string())
            }
            Err(e) => results.push(format!("Owner notification error: {}", e)),
        }
    } else {
        results.push("No owner notification channels configured.".to_string());
    }

    // ── Phase 2: Heir descriptor delivery (v0.2 escalation) ──
    // Only trigger when timelock is critical (≤1 day / ≤144 blocks)
    let is_critical = status.blocks_remaining <= 144;

    if is_critical {
        let heir_delivery_result =
            deliver_descriptor_to_heirs(&state, &service_secret, email_config.as_ref()).await;
        results.push(heir_delivery_result);
    }

    Ok(CommandResult::ok(results.join(" | ")))
}

/// Deliver the descriptor backup to all heirs with configured contact info.
///
/// This is the core inheritance mechanism — when the owner hasn't checked in
/// and the timelock is critical, heirs receive everything they need.
///
/// Rate limited: 24h cooldown per heir per channel to prevent spam.
async fn deliver_descriptor_to_heirs(
    state: &State<'_, AppState>,
    service_secret: &str,
    email_config: Option<&nostring_notify::EmailConfig>,
) -> String {
    // 24-hour cooldown between deliveries to the same heir on the same channel
    const DELIVERY_COOLDOWN_SECS: u64 = 86400;

    // Get the descriptor backup data
    let backup_data = {
        let config = {
            let config_lock = state.inheritance_config.lock().unwrap();
            match &*config_lock {
                Some(c) => c.clone(),
                None => {
                    return "Heir delivery skipped: no inheritance policy configured.".to_string()
                }
            }
        };

        let heirs: Vec<DescriptorBackupHeir> = {
            let registry = state.heir_registry.lock().unwrap();
            registry
                .list()
                .iter()
                .map(|h| DescriptorBackupHeir {
                    label: h.label.clone(),
                    xpub: h.xpub.to_string(),
                    timelock_months: config.timelock_blocks as f64 * 10.0 / 60.0 / 24.0 / 30.0,
                })
                .collect()
        };

        let address = {
            use miniscript::descriptor::DescriptorPublicKey;
            use miniscript::Descriptor;
            let desc: Result<Descriptor<DescriptorPublicKey>, _> = config.descriptor.parse();
            desc.ok()
                .and_then(|d| d.at_derivation_index(0).ok())
                .and_then(|d| {
                    let network = *state.network.lock().unwrap();
                    d.address(network).map(|a| a.to_string()).ok()
                })
        };

        let conn = state.db.lock().unwrap();
        let nsec_owner_npub = crate::db::config_get(&conn, "nsec_owner_npub")
            .ok()
            .flatten();
        let locked_shares = crate::db::config_get(&conn, "nsec_locked_shares")
            .ok()
            .flatten()
            .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok());
        drop(conn);

        DescriptorBackupData {
            descriptor: config.descriptor,
            network: config.network,
            timelock_blocks: config.timelock_blocks,
            address,
            heirs,
            nsec_owner_npub,
            locked_shares,
        }
    };

    let backup_json = match serde_json::to_string_pretty(&backup_data) {
        Ok(j) => j,
        Err(e) => return format!("Heir delivery failed: could not serialize backup: {}", e),
    };

    // Get heirs with contact info from DB
    let heir_contacts = {
        let conn = state.db.lock().unwrap();
        crate::db::heir_list(&conn).unwrap_or_default()
    };

    let relays = vec![
        "wss://relay.damus.io".into(),
        "wss://relay.nostr.band".into(),
        "wss://nos.lol".into(),
    ];

    let mut delivered = 0u32;
    let mut skipped = 0u32;
    let mut failed = 0u32;

    for heir in &heir_contacts {
        let message =
            nostring_notify::templates::generate_heir_delivery_message(&heir.label, &backup_json);

        // Nostr DM delivery
        if let Some(ref npub) = heir.npub {
            if state.can_deliver_to_heir(&heir.fingerprint, "nostr", DELIVERY_COOLDOWN_SECS) {
                match nostring_notify::nostr_dm::send_dm_to_recipient(
                    service_secret,
                    npub,
                    &relays,
                    &message,
                )
                .await
                {
                    Ok(_) => {
                        log::info!("Descriptor delivered to heir {} via Nostr DM", heir.label);
                        state.log_delivery(&heir.fingerprint, "nostr", true, None);
                        delivered += 1;
                    }
                    Err(e) => {
                        let err_msg = format!("{}", e);
                        log::error!(
                            "Failed to deliver descriptor to heir {} via Nostr: {}",
                            heir.label,
                            err_msg
                        );
                        state.log_delivery(&heir.fingerprint, "nostr", false, Some(&err_msg));
                        failed += 1;
                    }
                }
            } else {
                log::info!(
                    "Skipping Nostr delivery to heir {} (cooldown active)",
                    heir.label
                );
                skipped += 1;
            }
        }

        // Email delivery
        if let (Some(ref heir_email), Some(smtp_config)) = (&heir.email, email_config) {
            if state.can_deliver_to_heir(&heir.fingerprint, "email", DELIVERY_COOLDOWN_SECS) {
                match nostring_notify::smtp::send_email_to_recipient(
                    smtp_config,
                    heir_email,
                    &message,
                )
                .await
                {
                    Ok(_) => {
                        log::info!("Descriptor delivered to heir {} via email", heir.label);
                        state.log_delivery(&heir.fingerprint, "email", true, None);
                        delivered += 1;
                    }
                    Err(e) => {
                        let err_msg = format!("{}", e);
                        log::error!(
                            "Failed to deliver descriptor to heir {} via email: {}",
                            heir.label,
                            err_msg
                        );
                        state.log_delivery(&heir.fingerprint, "email", false, Some(&err_msg));
                        failed += 1;
                    }
                }
            } else {
                log::info!(
                    "Skipping email delivery to heir {} (cooldown active)",
                    heir.label
                );
                skipped += 1;
            }
        }
    }

    format!(
        "Heir descriptor delivery: {} sent, {} skipped (cooldown), {} failed",
        delivered, skipped, failed
    )
}

// ============================================================================
// Descriptor Backup Commands
// ============================================================================

/// Descriptor backup data returned to the frontend for file generation.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DescriptorBackupData {
    pub descriptor: String,
    pub network: String,
    pub timelock_blocks: u16,
    pub address: Option<String>,
    pub heirs: Vec<DescriptorBackupHeir>,
    pub nsec_owner_npub: Option<String>,
    pub locked_shares: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DescriptorBackupHeir {
    pub label: String,
    pub xpub: String,
    pub timelock_months: f64,
}

/// Get all data needed to generate the descriptor backup file.
///
/// Returns the inheritance descriptor, heir info, and any locked
/// Shamir shares for nsec inheritance.
#[tauri::command]
pub async fn get_descriptor_backup(
    state: State<'_, AppState>,
) -> Result<CommandResult<DescriptorBackupData>, ()> {
    let config = {
        let config_lock = state.inheritance_config.lock().unwrap();
        match &*config_lock {
            Some(c) => c.clone(),
            None => {
                return Ok(CommandResult::err(
                    "No inheritance policy configured. Add heirs first.",
                ))
            }
        }
    };

    // Build heir list
    let heirs: Vec<DescriptorBackupHeir> = {
        let registry = state.heir_registry.lock().unwrap();
        registry
            .list()
            .iter()
            .map(|h| DescriptorBackupHeir {
                label: h.label.clone(),
                xpub: h.xpub.to_string(),
                timelock_months: config.timelock_blocks as f64 * 10.0 / 60.0 / 24.0 / 30.0,
            })
            .collect()
    };

    // Derive inheritance address (index 0)
    let address = {
        use miniscript::descriptor::DescriptorPublicKey;
        use miniscript::Descriptor;
        let desc: Result<Descriptor<DescriptorPublicKey>, _> = config.descriptor.parse();
        desc.ok()
            .and_then(|d| d.at_derivation_index(0).ok())
            .and_then(|d| {
                let network = *state.network.lock().unwrap();
                d.address(network).map(|a| a.to_string()).ok()
            })
    };

    // Get nsec inheritance data
    let conn = state.db.lock().unwrap();
    let nsec_owner_npub = crate::db::config_get(&conn, "nsec_owner_npub")
        .ok()
        .flatten();
    let locked_shares = crate::db::config_get(&conn, "nsec_locked_shares")
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok());
    drop(conn);

    Ok(CommandResult::ok(DescriptorBackupData {
        descriptor: config.descriptor,
        network: config.network,
        timelock_blocks: config.timelock_blocks,
        address,
        heirs,
        nsec_owner_npub,
        locked_shares,
    }))
}

/// Generate Codex32 shares for a seed
///
/// Requires the wallet password to decrypt the seed for splitting.
/// The decrypted seed is held in memory only during share generation,
/// then zeroized.
#[tauri::command]
pub async fn generate_codex32_shares(
    threshold: u8,
    total_shares: u8,
    mut password: String,
    identifier: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandResult<Vec<String>>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        password.zeroize();
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    if !(2..=9).contains(&threshold) {
        password.zeroize();
        return Ok(CommandResult::err("Threshold must be 2-9"));
    }
    if total_shares < threshold {
        password.zeroize();
        return Ok(CommandResult::err("Total shares must be >= threshold"));
    }
    if total_shares > 31 {
        password.zeroize();
        return Ok(CommandResult::err("Maximum 31 shares supported"));
    }

    let id = identifier.unwrap_or_else(|| "SEED".to_string());

    let config = match Codex32Config::new(threshold, &id, total_shares) {
        Ok(c) => c,
        Err(e) => {
            password.zeroize();
            return Ok(CommandResult::err(format!("Invalid config: {}", e)));
        }
    };

    // Decrypt the seed using the provided password
    let encrypted_bytes = {
        let seed_lock = state.encrypted_seed.lock().unwrap();
        match &*seed_lock {
            Some(bytes) => bytes.clone(),
            None => {
                password.zeroize();
                return Ok(CommandResult::err(
                    "No seed loaded. This feature requires a seed-based wallet (not watch-only).",
                ));
            }
        }
    };

    let encrypted = match EncryptedSeed::from_bytes(&encrypted_bytes) {
        Ok(e) => e,
        Err(_) => {
            password.zeroize();
            return Ok(CommandResult::err("Corrupted seed data"));
        }
    };

    let decrypted_seed = match decrypt_seed(&encrypted, &password) {
        Ok(seed) => seed,
        Err(_) => {
            password.zeroize();
            return Ok(CommandResult::err("Incorrect password"));
        }
    };

    // Password no longer needed
    password.zeroize();

    // Use first 32 bytes of the 64-byte derived seed for Codex32
    // (Codex32 supports 16 or 32 byte secrets)
    let seed_bytes = &decrypted_seed[..32];

    use nostring_shamir::codex32::generate_shares;

    let result = match generate_shares(seed_bytes, &config) {
        Ok(shares) => {
            let share_strings: Vec<String> = shares.iter().map(|s| s.encoded.clone()).collect();
            Ok(CommandResult::ok(share_strings))
        }
        Err(e) => Ok(CommandResult::err(format!(
            "Failed to generate shares: {}",
            e
        ))),
    };

    // decrypted_seed is Zeroizing<[u8; 64]> — auto-zeroized on drop
    result
}

/// Combine Codex32 shares to recover a seed
#[tauri::command]
pub async fn combine_codex32_shares(shares: Vec<String>) -> CommandResult<String> {
    if shares.len() < 2 {
        return CommandResult::err("Need at least 2 shares to recover");
    }

    let mut parsed_shares: Vec<Codex32Share> = Vec::new();
    for share_str in &shares {
        match parse_share(share_str) {
            Ok(share) => parsed_shares.push(share),
            Err(e) => return CommandResult::err(format!("Invalid share '{}': {}", share_str, e)),
        }
    }

    use nostring_shamir::codex32::combine_shares;

    match combine_shares(&parsed_shares) {
        Ok(mut seed_bytes) => {
            let hex_str = hex::encode(&seed_bytes);
            // Zeroize recovered seed bytes from memory
            seed_bytes.zeroize();
            CommandResult::ok(hex_str)
        }
        Err(e) => CommandResult::err(format!("Failed to combine shares: {}", e)),
    }
}

// ============================================================================
// Relay Storage Commands (v0.3.1 — locked share relay backup)
// ============================================================================

/// Result of publishing locked shares to relays
#[derive(Debug, Serialize, Deserialize)]
pub struct RelayPublishStatus {
    pub shares_published: usize,
    pub heirs_targeted: usize,
    pub split_id: String,
    pub heir_results: Vec<RelayHeirStatus>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RelayHeirStatus {
    pub label: String,
    pub npub: String,
    pub shares_published: usize,
    pub event_ids: Vec<String>,
    pub error: Option<String>,
}

/// Publish locked shares to Nostr relays as encrypted backup.
///
/// Each locked share is NIP-44 encrypted to each heir's npub and published
/// to multiple relays (damus, nostr.band, nos.lol). This provides redundancy
/// beyond the descriptor backup file.
///
/// The encrypted shares are useless without threshold — this is defense in depth.
#[tauri::command]
pub async fn publish_locked_shares_to_relays(
    state: State<'_, AppState>,
) -> Result<CommandResult<RelayPublishStatus>, ()> {
    // Require wallet to be unlocked
    {
        let unlocked = state.unlocked.lock().unwrap();
        if !*unlocked {
            return Ok(CommandResult::err("Wallet is locked. Unlock first."));
        }
    }

    // Get the service key (sender)
    let service_secret = {
        let sk = state.service_key.lock().unwrap();
        match &*sk {
            Some(s) => s.clone(),
            None => {
                return Ok(CommandResult::err(
                    "No service key generated. Go to Settings → Notifications to set up.",
                ))
            }
        }
    };

    // Get locked shares from DB
    let locked_shares = {
        let conn = state.db.lock().unwrap();
        crate::db::config_get(&conn, "nsec_locked_shares")
            .ok()
            .flatten()
            .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
    };

    let Some(locked_shares) = locked_shares else {
        return Ok(CommandResult::err(
            "No locked shares found. Split your nsec first in the Inheritance tab.",
        ));
    };

    if locked_shares.is_empty() {
        return Ok(CommandResult::err("Locked shares list is empty."));
    }

    // Get heirs with npub from DB
    let heir_contacts: Vec<(String, String, String)> = {
        let conn = state.db.lock().unwrap();
        let heirs = crate::db::heir_list(&conn).unwrap_or_default();
        heirs
            .into_iter()
            .filter_map(|h| h.npub.map(|npub| (h.fingerprint, h.label, npub)))
            .collect()
    };

    if heir_contacts.is_empty() {
        return Ok(CommandResult::err(
            "No heirs have npub configured. Set heir npub in the Heirs tab.",
        ));
    }

    // Generate a split_id
    let split_id = nostring_notify::nostr_relay::generate_split_id();

    // Build heir list for publish
    let heirs: Vec<(String, String)> = heir_contacts
        .iter()
        .map(|(_, label, npub)| (npub.clone(), label.clone()))
        .collect();

    // Publish to relays
    let result = nostring_notify::nostr_relay::publish_all_shares(
        &service_secret,
        &heirs,
        &locked_shares,
        &split_id,
        None, // use default relays
    )
    .await;

    match result {
        Ok(publish_result) => {
            // Log publications to SQLite
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let conn = state.db.lock().unwrap();
            for hr in &publish_result.heir_results {
                // Find the fingerprint for this heir
                let fp = heir_contacts
                    .iter()
                    .find(|(_, _, npub)| npub == &hr.heir_npub)
                    .map(|(fp, _, _)| fp.as_str())
                    .unwrap_or("unknown");

                for (i, eid) in hr.event_ids.iter().enumerate() {
                    for relay in &publish_result.successful_relays {
                        let _ = crate::db::relay_publication_insert(
                            &conn,
                            &split_id,
                            fp,
                            &hr.heir_npub,
                            relay,
                            Some(eid),
                            i as i32,
                            locked_shares.len() as i32,
                            now,
                            true,
                            None,
                        );
                    }
                }

                if let Some(ref err) = hr.error {
                    for relay in &publish_result.failed_relays {
                        let _ = crate::db::relay_publication_insert(
                            &conn,
                            &split_id,
                            fp,
                            &hr.heir_npub,
                            relay,
                            None,
                            0,
                            locked_shares.len() as i32,
                            now,
                            false,
                            Some(err),
                        );
                    }
                }
            }

            // Persist the split_id for later reference
            let _ = crate::db::config_set(&conn, "last_relay_split_id", &split_id);
            drop(conn);

            let status = RelayPublishStatus {
                shares_published: publish_result.shares_published,
                heirs_targeted: heir_contacts.len(),
                split_id,
                heir_results: publish_result
                    .heir_results
                    .into_iter()
                    .map(|hr| RelayHeirStatus {
                        label: hr.heir_label,
                        npub: hr.heir_npub,
                        shares_published: hr.shares_published,
                        event_ids: hr.event_ids,
                        error: hr.error,
                    })
                    .collect(),
            };

            Ok(CommandResult::ok(status))
        }
        Err(e) => Ok(CommandResult::err(format!(
            "Failed to publish shares: {}",
            e
        ))),
    }
}

/// Fetch locked shares from Nostr relays (heir recovery tool).
///
/// The heir provides their nsec and the service key's npub to find
/// and decrypt the encrypted shares published to relays.
#[tauri::command]
pub async fn fetch_locked_shares_from_relays(
    heir_nsec: String,
    sender_npub: String,
    split_id: Option<String>,
) -> CommandResult<FetchedSharesResult> {
    use nostring_notify::nostr_relay;

    let result = nostr_relay::fetch_shares_from_relays(
        &heir_nsec,
        &sender_npub,
        None, // use default relays
        split_id.as_deref(),
    )
    .await;

    match result {
        Ok(fetch_result) => {
            let shares: Vec<String> = fetch_result
                .shares
                .iter()
                .map(|s| s.share.clone())
                .collect();

            CommandResult::ok(FetchedSharesResult {
                shares,
                events_found: fetch_result.events_found,
                relays_queried: fetch_result.responding_relays,
            })
        }
        Err(e) => CommandResult::err(format!("Failed to fetch shares: {}", e)),
    }
}

/// Result of fetching shares from relays
#[derive(Debug, Serialize, Deserialize)]
pub struct FetchedSharesResult {
    pub shares: Vec<String>,
    pub events_found: usize,
    pub relays_queried: Vec<String>,
}

/// Get relay publication status (last publish info).
#[tauri::command]
pub async fn get_relay_publication_status(
    state: State<'_, AppState>,
) -> Result<CommandResult<RelayPublicationInfo>, ()> {
    let conn = state.db.lock().unwrap();

    let split_id = crate::db::config_get(&conn, "last_relay_split_id")
        .ok()
        .flatten();

    let Some(split_id) = split_id else {
        return Ok(CommandResult::ok(RelayPublicationInfo {
            published: false,
            split_id: None,
            total_published: 0,
            last_published_at: None,
            publications: Vec::new(),
        }));
    };

    let count = crate::db::relay_publication_success_count(&conn, &split_id).unwrap_or(0);
    let last_at = crate::db::relay_publication_last(&conn, &split_id)
        .ok()
        .flatten();
    let publications =
        crate::db::relay_publication_list_by_split(&conn, &split_id).unwrap_or_default();
    drop(conn);

    let pub_info: Vec<RelayPubEntry> = publications
        .into_iter()
        .map(|p| RelayPubEntry {
            heir_npub: p.heir_npub,
            relay_url: p.relay_url,
            event_id: p.event_id,
            share_index: p.share_index,
            success: p.success,
            published_at: p.published_at,
        })
        .collect();

    Ok(CommandResult::ok(RelayPublicationInfo {
        published: count > 0,
        split_id: Some(split_id),
        total_published: count as usize,
        last_published_at: last_at,
        publications: pub_info,
    }))
}

/// Relay publication info for the frontend
#[derive(Debug, Serialize, Deserialize)]
pub struct RelayPublicationInfo {
    pub published: bool,
    pub split_id: Option<String>,
    pub total_published: usize,
    pub last_published_at: Option<u64>,
    pub publications: Vec<RelayPubEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RelayPubEntry {
    pub heir_npub: String,
    pub relay_url: String,
    pub event_id: Option<String>,
    pub share_index: i32,
    pub success: bool,
    pub published_at: u64,
}

// ============================================================================
// Pre-signed Check-in Stack (v0.3 — Auto Check-in)
// ============================================================================

use crate::db::PresignedCheckinRow;

/// Summary of the pre-signed check-in stack.
#[derive(Debug, Serialize, Deserialize)]
pub struct PresignedCheckinStatus {
    /// Number of active (ready to broadcast) pre-signed PSBTs
    pub active_count: i64,
    /// Total PSBTs ever added (including broadcast/invalidated)
    pub total_count: usize,
    /// Whether the stack is running low (< 2 active)
    pub low_warning: bool,
    /// Whether the stack is empty
    pub empty: bool,
    /// The active PSBTs
    pub active: Vec<PresignedCheckinInfo>,
}

/// Info about a single pre-signed check-in PSBT.
#[derive(Debug, Serialize, Deserialize)]
pub struct PresignedCheckinInfo {
    pub id: i64,
    pub sequence_index: i64,
    pub spending_txid: Option<String>,
    pub spending_vout: Option<i64>,
    pub created_at: u64,
    pub broadcast_at: Option<u64>,
    pub txid: Option<String>,
    pub invalidated_at: Option<u64>,
    pub invalidation_reason: Option<String>,
}

impl From<&PresignedCheckinRow> for PresignedCheckinInfo {
    fn from(row: &PresignedCheckinRow) -> Self {
        Self {
            id: row.id,
            sequence_index: row.sequence_index,
            spending_txid: row.spending_txid.clone(),
            spending_vout: row.spending_vout,
            created_at: row.created_at,
            broadcast_at: row.broadcast_at,
            txid: row.txid.clone(),
            invalidated_at: row.invalidated_at,
            invalidation_reason: row.invalidation_reason.clone(),
        }
    }
}

/// Add a pre-signed (already signed) check-in PSBT to the stack.
///
/// The user signs multiple sequential check-in PSBTs on their hardware wallet,
/// then imports them here. The app will broadcast them automatically when needed.
///
/// **Security note:** Each PSBT in the sequence spends the output of the previous one.
/// PSBT 0 spends the current inheritance UTXO. PSBT 1 spends PSBT 0's output, etc.
#[tauri::command]
pub async fn add_presigned_checkin(
    signed_psbt_base64: String,
    sequence_index: i64,
    spending_txid: Option<String>,
    spending_vout: Option<i64>,
    state: State<'_, AppState>,
) -> Result<CommandResult<i64>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    // Validate the PSBT is parseable
    use base64::prelude::*;
    let psbt_bytes = match BASE64_STANDARD.decode(&signed_psbt_base64) {
        Ok(b) => b,
        Err(e) => return Ok(CommandResult::err(format!("Invalid base64: {}", e))),
    };

    let psbt: Psbt = match Psbt::deserialize(&psbt_bytes) {
        Ok(p) => p,
        Err(e) => return Ok(CommandResult::err(format!("Invalid PSBT: {}", e))),
    };

    // Verify the PSBT can extract a transaction (i.e., it's signed)
    match psbt.extract_tx() {
        Ok(_) => {}
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "PSBT is not fully signed: {}. Sign it on your hardware wallet first.",
                e
            )))
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let conn = state.db.lock().unwrap();
    match crate::db::presigned_checkin_add(
        &conn,
        &signed_psbt_base64,
        sequence_index,
        spending_txid.as_deref(),
        spending_vout,
        now,
    ) {
        Ok(id) => {
            log::info!(
                "Added pre-signed check-in #{} (sequence {})",
                id,
                sequence_index
            );
            Ok(CommandResult::ok(id))
        }
        Err(e) => Ok(CommandResult::err(format!("Database error: {}", e))),
    }
}

/// List the pre-signed check-in stack status.
#[tauri::command]
pub async fn get_presigned_checkin_status(
    state: State<'_, AppState>,
) -> Result<PresignedCheckinStatus, ()> {
    let conn = state.db.lock().unwrap();

    let active = crate::db::presigned_checkin_list_active(&conn).unwrap_or_default();
    let all = crate::db::presigned_checkin_list_all(&conn).unwrap_or_default();
    let active_count = active.len() as i64;

    let status = PresignedCheckinStatus {
        active_count,
        total_count: all.len(),
        low_warning: active_count > 0 && active_count < 2,
        empty: active_count == 0,
        active: active.iter().map(PresignedCheckinInfo::from).collect(),
    };

    Ok(status)
}

/// Automatically broadcast the next pre-signed check-in if the timelock
/// is approaching the threshold.
///
/// **Logic:**
/// 1. Check if timelock is within the auto-broadcast threshold
/// 2. Get the next active pre-signed PSBT
/// 3. Extract and broadcast the transaction
/// 4. Mark the PSBT as broadcast
/// 5. Log the check-in
///
/// Returns the broadcast txid if a check-in was broadcast, or a status message.
#[tauri::command]
pub async fn auto_broadcast_checkin(
    threshold_blocks: Option<i64>,
    state: State<'_, AppState>,
) -> Result<CommandResult<String>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    // Default threshold: 30 days (4320 blocks)
    let threshold = threshold_blocks.unwrap_or(4320);

    // Check current policy status
    let status = {
        let s = state.policy_status.lock().unwrap();
        match &*s {
            Some(st) => st.clone(),
            None => {
                return Ok(CommandResult::err(
                    "No policy status available. Call refresh_policy_status first.",
                ))
            }
        }
    };

    // Only broadcast if timelock is within threshold
    if status.blocks_remaining > threshold {
        return Ok(CommandResult::ok(format!(
            "No check-in needed. {} blocks remaining (threshold: {})",
            status.blocks_remaining, threshold
        )));
    }

    // Get next pre-signed PSBT
    let next_psbt = {
        let conn = state.db.lock().unwrap();
        crate::db::presigned_checkin_next(&conn).unwrap_or(None)
    };

    let psbt_row = match next_psbt {
        Some(row) => row,
        None => {
            return Ok(CommandResult::err(
                "No pre-signed check-ins available! Add signed PSBTs to the stack.",
            ))
        }
    };

    // Decode and extract transaction
    use base64::prelude::*;
    let psbt_bytes = match BASE64_STANDARD.decode(&psbt_row.psbt_base64) {
        Ok(b) => b,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Stored PSBT has invalid base64: {}",
                e
            )))
        }
    };

    let psbt: Psbt = match Psbt::deserialize(&psbt_bytes) {
        Ok(p) => p,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Stored PSBT is corrupted: {}",
                e
            )))
        }
    };

    let tx = match psbt.extract_tx() {
        Ok(t) => t,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Stored PSBT cannot extract tx: {}",
                e
            )))
        }
    };

    // Broadcast
    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    let client = match ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Failed to connect to Electrum: {}",
                e
            )))
        }
    };

    match client.broadcast(&tx) {
        Ok(txid) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let txid_str = txid.to_string();

            // Mark PSBT as broadcast
            {
                let conn = state.db.lock().unwrap();
                let _ = crate::db::presigned_checkin_mark_broadcast(
                    &conn,
                    psbt_row.id,
                    now,
                    &txid_str,
                );
            }

            // Log the check-in
            state.log_checkin(&txid_str);

            // Check remaining stack and warn
            let remaining = {
                let conn = state.db.lock().unwrap();
                crate::db::presigned_checkin_count_active(&conn).unwrap_or(0)
            };

            log::info!(
                "Auto check-in broadcast: {} (sequence {}). {} pre-signed PSBTs remaining.",
                txid_str,
                psbt_row.sequence_index,
                remaining
            );

            if remaining < 2 {
                log::warn!(
                    "Pre-signed check-in stack is running low! {} remaining. Generate more PSBTs.",
                    remaining
                );
            }

            Ok(CommandResult::ok(txid_str))
        }
        Err(e) => Ok(CommandResult::err(format!(
            "Broadcast failed: {}. The UTXO may have been spent (manual check-in?). Consider invalidating stale PSBTs.",
            e
        ))),
    }
}

/// Invalidate all active pre-signed check-ins.
///
/// Call this after a manual check-in, which spends the UTXO that
/// pre-signed PSBTs were built to spend. The chain is broken.
#[tauri::command]
pub async fn invalidate_presigned_checkins(
    reason: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandResult<usize>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let reason = reason.unwrap_or_else(|| "Manual invalidation".to_string());

    let conn = state.db.lock().unwrap();
    match crate::db::presigned_checkin_invalidate_all(&conn, now, &reason) {
        Ok(count) => {
            log::info!("Invalidated {} pre-signed check-ins: {}", count, reason);
            Ok(CommandResult::ok(count))
        }
        Err(e) => Ok(CommandResult::err(format!("Database error: {}", e))),
    }
}

/// Delete a specific pre-signed check-in by ID (only if not yet broadcast).
#[tauri::command]
pub async fn delete_presigned_checkin(
    id: i64,
    state: State<'_, AppState>,
) -> Result<CommandResult<bool>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    let conn = state.db.lock().unwrap();
    match crate::db::presigned_checkin_delete(&conn, id) {
        Ok(deleted) => {
            if deleted {
                log::info!("Deleted pre-signed check-in #{}", id);
            }
            Ok(CommandResult::ok(deleted))
        }
        Err(e) => Ok(CommandResult::err(format!("Database error: {}", e))),
    }
}

/// Generate multiple unsigned check-in PSBTs for sequential signing.
///
/// This creates a chain of PSBTs where each one spends the output of the previous:
/// - PSBT 0: spends current inheritance UTXO → creates new UTXO
/// - PSBT 1: spends PSBT 0's output → creates new UTXO
/// - PSBT N: spends PSBT (N-1)'s output → creates new UTXO
///
/// The user exports these to their hardware wallet, signs them all,
/// then imports the signed versions via `add_presigned_checkin`.
///
/// Returns base64-encoded unsigned PSBTs.
#[tauri::command]
pub async fn generate_checkin_psbt_chain(
    count: usize,
    state: State<'_, AppState>,
) -> Result<CommandResult<Vec<String>>, ()> {
    let unlocked = state.unlocked.lock().unwrap();
    if !*unlocked {
        return Ok(CommandResult::err("Wallet is locked"));
    }
    drop(unlocked);

    if count == 0 || count > 12 {
        return Ok(CommandResult::err(
            "Count must be 1-12 (more than 12 sequential check-ins is impractical)",
        ));
    }

    let config = {
        let config_lock = state.inheritance_config.lock().unwrap();
        match &*config_lock {
            Some(c) => c.clone(),
            None => {
                return Ok(CommandResult::err(
                    "No inheritance policy configured. Add heirs first.",
                ))
            }
        }
    };

    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    let client = match ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Failed to connect to Electrum: {}",
                e
            )))
        }
    };

    use miniscript::descriptor::DescriptorPublicKey;
    use miniscript::Descriptor;

    let descriptor: Descriptor<DescriptorPublicKey> = match config.descriptor.parse() {
        Ok(d) => d,
        Err(e) => return Ok(CommandResult::err(format!("Invalid descriptor: {}", e))),
    };

    let derived = match descriptor.at_derivation_index(0) {
        Ok(d) => d,
        Err(e) => {
            return Ok(CommandResult::err(format!(
                "Failed to derive script: {}",
                e
            )))
        }
    };
    let script = derived.script_pubkey();

    // Get current UTXOs
    let utxos = match client.get_utxos_for_script(&script) {
        Ok(u) => u,
        Err(e) => return Ok(CommandResult::err(format!("Failed to get UTXOs: {}", e))),
    };

    if utxos.is_empty() {
        return Ok(CommandResult::err(
            "No UTXOs found for inheritance address. Deposit funds first.",
        ));
    }

    let utxo = &utxos[0];
    let fee_rate = 10u64;

    use nostring_inherit::checkin::{CheckinTxBuilder, InheritanceUtxo as InhUtxo};

    let mut psbts: Vec<String> = Vec::with_capacity(count);
    let mut current_utxo = InhUtxo::new(utxo.outpoint, utxo.value, utxo.height, script.to_owned());

    for i in 0..count {
        let builder = CheckinTxBuilder::new(current_utxo.clone(), descriptor.clone(), fee_rate, 0);

        let psbt = match builder.build_psbt() {
            Ok(p) => p,
            Err(e) => {
                return Ok(CommandResult::err(format!(
                    "Failed to build PSBT #{}: {}",
                    i, e
                )))
            }
        };

        // The output of this PSBT becomes the input for the next one
        let tx = match psbt.clone().extract_tx() {
            Ok(t) => t,
            Err(_) => {
                // For unsigned PSBTs, build the unsigned tx directly
                match builder.build_unsigned_tx() {
                    Ok(t) => t,
                    Err(e) => {
                        return Ok(CommandResult::err(format!(
                            "Failed to build tx #{}: {}",
                            i, e
                        )))
                    }
                }
            }
        };

        let txid = tx.compute_txid();

        // Find the output that goes back to our script (the check-in output)
        let (vout, value) = tx
            .output
            .iter()
            .enumerate()
            .find(|(_, o)| o.script_pubkey == script)
            .map(|(i, o)| (i as u32, o.value))
            .unwrap_or((0, tx.output[0].value));

        let next_outpoint = bitcoin::OutPoint { txid, vout };

        // For the next iteration, assume it confirms at a reasonable height
        // (the exact height doesn't matter for PSBT construction)
        current_utxo = InhUtxo::new(
            next_outpoint,
            value,
            current_utxo.confirmation_height + 1,
            script.to_owned(),
        );

        use base64::prelude::*;
        psbts.push(BASE64_STANDARD.encode(psbt.serialize()));
    }

    log::info!(
        "Generated {} unsigned check-in PSBTs for sequential signing",
        count
    );
    Ok(CommandResult::ok(psbts))
}
