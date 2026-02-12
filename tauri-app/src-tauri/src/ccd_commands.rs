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
// Keyless MuSig2 Signing Ceremony
// ============================================================================

/// Start a keyless MuSig2 signing session.
///
/// Creates a session ID and nonce request for the co-signer.
/// The owner's signing device generates nonces externally (not in this app).
///
/// Returns JSON containing the NonceRequest (to send to co-signer) and session_id.
#[tauri::command]
pub async fn start_signing_session(
    state: State<'_, AppState>,
) -> Result<CcdResult<serde_json::Value>, ()> {
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

    // Get cosigner delegated key for tweak computation
    let delegated = {
        let ccd = state.ccd.lock().unwrap();
        match &ccd.cosigner {
            Some(d) => d.clone(),
            None => return Ok(CcdResult::err("No co-signer registered")),
        }
    };

    // Compute tweaks for the vault's address index
    let tweak_disclosure = match nostring_ccd::compute_tweak(&delegated, vault.address_index) {
        Ok(t) => t,
        Err(e) => return Ok(CcdResult::err(format!("Tweak computation failed: {}", e))),
    };

    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    // Find vault UTXOs to determine num_inputs
    let client = match nostring_electrum::ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => return Ok(CcdResult::err(format!("Electrum connection failed: {}", e))),
    };

    let script = vault.address.script_pubkey();
    let utxos = match client.get_utxos_for_script(&script) {
        Ok(u) => u,
        Err(e) => return Ok(CcdResult::err(format!("Failed to get UTXOs: {}", e))),
    };

    if utxos.is_empty() {
        return Ok(CcdResult::err("No UTXOs found at vault address"));
    }

    let num_inputs = utxos.len();

    // Build the check-in PSBT (needed for sighash computation later)
    let fee_rate = client
        .estimate_fee_rate(6)
        .unwrap_or(10.0)
        .clamp(1.0, 500.0);

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

    let checkin = match build_taproot_checkin_psbt(&config) {
        Ok(c) => c,
        Err(e) => return Ok(CcdResult::err(format!("Failed to build PSBT: {}", e))),
    };

    // Start orchestrator session (NO secret key)
    let tweaks = vec![tweak_disclosure];
    let (nonce_request, session_id) =
        match nostring_ccd::blind::orchestrator_start_session(num_inputs, &tweaks) {
            Ok(r) => r,
            Err(e) => return Ok(CcdResult::err(format!("Session start failed: {}", e))),
        };

    // Store session state
    {
        let mut ccd = state.ccd.lock().unwrap();
        ccd.signing_session = Some(crate::state::SigningSession {
            session_id: session_id.clone(),
            psbt: checkin.psbt,
            owner_pubnonces_hex: None,
            agg_nonces_hex: None,
            sighashes_hex: None,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });
    }

    // Return nonce request + session ID for the frontend
    let result = serde_json::json!({
        "session_id": session_id,
        "nonce_request": nonce_request,
    });

    Ok(CcdResult::ok(result))
}

/// Submit nonces from owner's signing device and co-signer.
///
/// The owner's device provides PubNonces (via QR/Nostr).
/// The co-signer provides a NonceResponse (via Nostr DM).
/// The orchestrator computes aggregate nonces and sighashes,
/// then returns sign challenges for both parties.
#[tauri::command]
pub async fn submit_nonces(
    owner_pubnonces_hex: Vec<String>,
    cosigner_response_json: String,
    state: State<'_, AppState>,
) -> Result<CcdResult<serde_json::Value>, ()> {
    use nostring_ccd::blind::{orchestrator_create_challenges, NonceResponse};
    use nostring_ccd::PubNonce;

    // Get session (with expiry check)
    let (session_id, psbt) = {
        let mut ccd = state.ccd.lock().unwrap();
        match &ccd.signing_session {
            Some(s) if s.is_expired() => {
                ccd.signing_session = None;
                return Ok(CcdResult::err(
                    "Signing session expired (1 hour timeout). Start a new session.",
                ));
            }
            Some(s) => (s.session_id.clone(), s.psbt.clone()),
            None => return Ok(CcdResult::err("No active signing session")),
        }
    };

    // Parse owner's PubNonces
    let owner_pubnonces: Vec<PubNonce> = match owner_pubnonces_hex
        .iter()
        .map(|h| {
            let bytes = hex::decode(h).map_err(|e| format!("Invalid nonce hex: {}", e))?;
            PubNonce::from_bytes(&bytes).map_err(|e| format!("Invalid nonce: {}", e))
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(n) => n,
        Err(e) => return Ok(CcdResult::err(e)),
    };

    // Parse cosigner response
    let cosigner_response: NonceResponse = match serde_json::from_str(&cosigner_response_json) {
        Ok(r) => r,
        Err(e) => return Ok(CcdResult::err(format!("Invalid cosigner response: {}", e))),
    };

    // Create challenges (NO secret key)
    let (sign_challenge, agg_nonces, sighashes) = match orchestrator_create_challenges(
        &owner_pubnonces,
        &cosigner_response,
        &psbt,
        &session_id,
    ) {
        Ok(r) => r,
        Err(e) => return Ok(CcdResult::err(format!("Challenge creation failed: {}", e))),
    };

    // Store intermediate state
    let agg_nonces_hex: Vec<String> = agg_nonces
        .iter()
        .map(|n| hex::encode(n.serialize()))
        .collect();
    let sighashes_hex: Vec<String> = sighashes.iter().map(hex::encode).collect();

    {
        let mut ccd = state.ccd.lock().unwrap();
        if let Some(session) = &mut ccd.signing_session {
            session.owner_pubnonces_hex = Some(owner_pubnonces_hex);
            session.agg_nonces_hex = Some(agg_nonces_hex);
            session.sighashes_hex = Some(sighashes_hex);
        }
    }

    // Return challenges for both signing devices
    let result = serde_json::json!({
        "sign_challenge": sign_challenge,
        "owner_challenges": sighashes.iter().zip(agg_nonces.iter()).map(|(sh, an)| {
            serde_json::json!({
                "sighash": hex::encode(sh),
                "agg_nonce": hex::encode(an.serialize()),
            })
        }).collect::<Vec<_>>(),
    });

    Ok(CcdResult::ok(result))
}

/// Finalize the signing ceremony and broadcast.
///
/// Takes partial signatures from both the owner's signing device and
/// the co-signer. Aggregates them into final Schnorr signatures.
/// **No secret key is used.** Then broadcasts the signed transaction.
#[tauri::command]
pub async fn finalize_and_broadcast(
    owner_partial_sigs_hex: Vec<String>,
    cosigner_partials_json: String,
    state: State<'_, AppState>,
) -> Result<CcdResult<String>, ()> {
    use nostring_ccd::blind::{orchestrator_finalize, PartialSignatures};

    // Get session state (with expiry check)
    let (session_id, psbt, agg_nonces_hex, sighashes_hex) = {
        let mut ccd = state.ccd.lock().unwrap();
        match &ccd.signing_session {
            Some(s) if s.is_expired() => {
                ccd.signing_session = None;
                return Ok(CcdResult::err(
                    "Signing session expired (1 hour timeout). Start a new session.",
                ));
            }
            Some(s) => {
                let an = match &s.agg_nonces_hex {
                    Some(v) => v.clone(),
                    None => {
                        return Ok(CcdResult::err(
                            "Nonces not yet submitted. Call submit_nonces first.",
                        ))
                    }
                };
                let sh = match &s.sighashes_hex {
                    Some(v) => v.clone(),
                    None => {
                        return Ok(CcdResult::err(
                            "Sighashes not computed. Call submit_nonces first.",
                        ))
                    }
                };
                (s.session_id.clone(), s.psbt.clone(), an, sh)
            }
            None => return Ok(CcdResult::err("No active signing session")),
        }
    };

    // Reconstruct AggNonces
    let agg_nonces: Vec<nostring_ccd::AggNonce> = match agg_nonces_hex
        .iter()
        .map(|h| {
            let bytes = hex::decode(h).map_err(|e| format!("Invalid agg nonce: {}", e))?;
            nostring_ccd::AggNonce::from_bytes(&bytes)
                .map_err(|e| format!("Parse agg nonce: {}", e))
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(n) => n,
        Err(e) => return Ok(CcdResult::err(e)),
    };

    // Reconstruct sighashes
    let sighashes: Vec<[u8; 32]> = match sighashes_hex
        .iter()
        .map(|h| {
            let bytes = hex::decode(h).map_err(|e| format!("Invalid sighash: {}", e))?;
            if bytes.len() != 32 {
                return Err(format!("Sighash must be 32 bytes, got {}", bytes.len()));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(arr)
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(s) => s,
        Err(e) => return Ok(CcdResult::err(e)),
    };

    // Parse cosigner partials
    let cosigner_partials: PartialSignatures = match serde_json::from_str(&cosigner_partials_json) {
        Ok(p) => p,
        Err(e) => return Ok(CcdResult::err(format!("Invalid cosigner partials: {}", e))),
    };

    // Get KeyAggContext from vault
    let key_agg_ctx = {
        let ccd = state.ccd.lock().unwrap();
        match &ccd.vault {
            Some(v) => match v.key_agg_context() {
                Ok((ctx, _)) => ctx,
                Err(e) => return Ok(CcdResult::err(format!("Key context error: {}", e))),
            },
            None => return Ok(CcdResult::err("No vault available")),
        }
    };

    // Finalize (NO secret key)
    let signed_tx = match orchestrator_finalize(
        &owner_partial_sigs_hex,
        &cosigner_partials,
        &agg_nonces,
        &key_agg_ctx,
        &psbt,
        &session_id,
        &sighashes,
    ) {
        Ok(tx) => tx,
        Err(e) => return Ok(CcdResult::err(format!("Finalization failed: {}", e))),
    };

    // Broadcast
    let electrum_url = state.electrum_url.lock().unwrap().clone();
    let network = *state.network.lock().unwrap();

    let client = match nostring_electrum::ElectrumClient::new(&electrum_url, network) {
        Ok(c) => c,
        Err(e) => return Ok(CcdResult::err(format!("Electrum connection failed: {}", e))),
    };

    let txid = match client.broadcast(&signed_tx) {
        Ok(id) => id,
        Err(e) => return Ok(CcdResult::err(format!("Broadcast failed: {}", e))),
    };

    // Clear signing session
    {
        let mut ccd = state.ccd.lock().unwrap();
        ccd.signing_session = None;
    }

    // Log the check-in
    state.log_checkin(&txid.to_string());

    log::info!("MuSig2 check-in broadcast: {}", txid);

    Ok(CcdResult::ok(txid.to_string()))
}

/// Clear/abandon an active signing session.
#[tauri::command]
pub async fn cancel_signing_session(state: State<'_, AppState>) -> Result<CcdResult<bool>, ()> {
    let mut ccd = state.ccd.lock().unwrap();
    let had_session = ccd.signing_session.is_some();
    ccd.signing_session = None;
    Ok(CcdResult::ok(had_session))
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

    #[test]
    fn test_signing_session_not_expired() {
        use crate::state::SigningSession;
        let session = SigningSession {
            session_id: "test".to_string(),
            psbt: bitcoin::psbt::Psbt::deserialize(&[
                0x70, 0x73, 0x62, 0x74, 0xff, // "psbt" + separator
                // Minimal valid PSBT: global unsigned tx
                0x00, // global separator
            ])
            .unwrap_or_else(|_| {
                // Build a minimal PSBT from a transaction
                let tx = bitcoin::Transaction {
                    version: bitcoin::transaction::Version(2),
                    lock_time: bitcoin::locktime::absolute::LockTime::ZERO,
                    input: vec![],
                    output: vec![],
                };
                bitcoin::psbt::Psbt::from_unsigned_tx(tx).unwrap()
            }),
            owner_pubnonces_hex: None,
            agg_nonces_hex: None,
            sighashes_hex: None,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        assert!(!session.is_expired());
    }

    #[test]
    fn test_signing_session_boundary() {
        use crate::state::SigningSession;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let make = |offset: u64| {
            let tx = bitcoin::Transaction {
                version: bitcoin::transaction::Version(2),
                lock_time: bitcoin::locktime::absolute::LockTime::ZERO,
                input: vec![],
                output: vec![],
            };
            SigningSession {
                session_id: "test".to_string(),
                psbt: bitcoin::psbt::Psbt::from_unsigned_tx(tx).unwrap(),
                owner_pubnonces_hex: None,
                agg_nonces_hex: None,
                sighashes_hex: None,
                created_at: offset,
            }
        };
        // 3599 seconds ago = not expired (within 1 hour)
        let recent = make(now - 3599);
        assert!(!recent.is_expired());
        // 3601 seconds ago = expired (past 1 hour)
        let old = make(now - 3601);
        assert!(old.is_expired());
        // Exactly 3600 = not expired (boundary is >)
        let exact = make(now - 3600);
        assert!(!exact.is_expired());
    }

    #[test]
    fn test_signing_session_expired() {
        use crate::state::SigningSession;
        let tx = bitcoin::Transaction {
            version: bitcoin::transaction::Version(2),
            lock_time: bitcoin::locktime::absolute::LockTime::ZERO,
            input: vec![],
            output: vec![],
        };
        let session = SigningSession {
            session_id: "test".to_string(),
            psbt: bitcoin::psbt::Psbt::from_unsigned_tx(tx).unwrap(),
            owner_pubnonces_hex: None,
            agg_nonces_hex: None,
            sighashes_hex: None,
            created_at: 0, // epoch = definitely expired
        };
        assert!(session.is_expired());
    }
}
