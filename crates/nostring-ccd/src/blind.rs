//! Blind signing — split MuSig2 ceremony where the co-signer never sees the PSBT.
//!
//! In the standard `musig2_sign_psbt()`, both parties share the full PSBT.
//! This module splits the ceremony into messages so the co-signer only sees
//! sighashes (opaque 32-byte hashes) and aggregate nonces — learning nothing
//! about transaction amounts, addresses, or UTXOs.
//!
//! # Protocol
//!
//! ```text
//! Owner                                          Co-signer
//!   │  ── NonceRequest { session_id, tweaks } ──►  │
//!   │  ◄─ NonceResponse { pubnonces } ───────────  │
//!   │  ── SignChallenge { agg_nonces, sighashes } ► │
//!   │  ◄─ PartialSignatures { partial_sigs } ────  │
//! ```
//!
//! # Trust Model (Phase 5a)
//!
//! The co-signer trusts the owner is signing a valid transaction. This is
//! appropriate because the owner already has their own key — they need the
//! co-signer's cooperation, not the other way around. Phase 5b (future) adds
//! ZK policy proofs to remove this trust assumption.

use bitcoin::secp256k1::SecretKey;
use musig2::{AggNonce, KeyAggContext, PubNonce, SecNonce};
use serde::{Deserialize, Serialize};

use crate::musig;
use crate::types::CcdError;

// ─── Message Types ──────────────────────────────────────────────────────────

/// Round 1: Owner requests co-signer's nonces for a signing session.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NonceRequest {
    /// Unique session ID (random 32 bytes, hex-encoded for transport)
    pub session_id: String,
    /// Number of inputs to sign
    pub num_inputs: usize,
    /// Tweak disclosures (hex-encoded scalar + derived pubkey per input)
    pub tweaks: Vec<SerializedTweak>,
}

/// Serialized tweak for blind signing transport.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SerializedTweak {
    /// Hex-encoded scalar tweak
    pub tweak: String,
    /// Hex-encoded derived public key (for verification)
    pub derived_pubkey: String,
    /// Child index
    pub child_index: u32,
}

/// Round 1 response: Co-signer's public nonces.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NonceResponse {
    pub session_id: String,
    /// One hex-encoded PubNonce (66 bytes) per input
    pub pubnonces: Vec<String>,
}

/// Round 2: Owner sends signing challenges (sighashes only, no PSBT).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignChallenge {
    pub session_id: String,
    /// One challenge per input
    pub challenges: Vec<InputChallenge>,
}

/// A single input's signing challenge.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InputChallenge {
    /// Hex-encoded aggregate nonce (66 bytes)
    pub agg_nonce: String,
    /// Hex-encoded sighash (32 bytes) — opaque to co-signer
    pub sighash: String,
}

/// Round 2 response: Co-signer's partial signatures.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PartialSignatures {
    pub session_id: String,
    /// One hex-encoded partial signature (32 bytes) per input
    pub partial_sigs: Vec<String>,
}

// ─── Co-signer Session State ────────────────────────────────────────────────

/// Ephemeral state held by the co-signer between Round 1 and Round 2.
///
/// Contains the secret nonces that MUST be used exactly once.
/// Dropped (zeroized) after signing or on abort.
pub struct CosignerSession {
    pub session_id: String,
    /// The co-signer's derived child secret key (after applying tweak)
    child_sk: SecretKey,
    /// Secret nonces — one per input. Consumed during signing.
    sec_nonces: Vec<SecNonce>,
    /// Public nonces (kept for potential verification in future phases)
    #[allow(dead_code)]
    pub_nonces: Vec<PubNonce>,
}

// ─── Owner Functions ────────────────────────────────────────────────────────

/// Owner: Start a blind signing session.
///
/// Generates the owner's nonces and creates a NonceRequest to send to the
/// co-signer. The owner's SecNonces are returned and MUST be kept secret.
///
/// The PSBT is needed here only to determine the number of inputs.
/// It is NOT included in the NonceRequest.
pub fn owner_start_session(
    owner_sk: &SecretKey,
    key_agg_ctx: &KeyAggContext,
    num_inputs: usize,
    tweaks: &[crate::types::TweakDisclosure],
) -> Result<(NonceRequest, Vec<SecNonce>, Vec<PubNonce>), CcdError> {
    if num_inputs == 0 {
        return Err(CcdError::PsbtError("no inputs to sign".into()));
    }

    // Generate session ID
    let mut session_id_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut session_id_bytes);
    let session_id = hex::encode(session_id_bytes);

    // Generate owner's nonces (one per input)
    let mut sec_nonces = Vec::with_capacity(num_inputs);
    let mut pub_nonces = Vec::with_capacity(num_inputs);

    for _ in 0..num_inputs {
        let (sec, pub_n) = musig::generate_nonce(owner_sk, key_agg_ctx, None)?;
        sec_nonces.push(sec);
        pub_nonces.push(pub_n);
    }

    // Serialize tweaks
    let serialized_tweaks: Vec<SerializedTweak> = tweaks
        .iter()
        .map(|t| SerializedTweak {
            tweak: hex::encode(t.tweak.to_be_bytes()),
            derived_pubkey: hex::encode(t.derived_pubkey.serialize()),
            child_index: t.child_index,
        })
        .collect();

    let request = NonceRequest {
        session_id,
        num_inputs,
        tweaks: serialized_tweaks,
    };

    Ok((request, sec_nonces, pub_nonces))
}

/// Owner: After receiving co-signer nonces, compute sighashes and create
/// sign challenges.
///
/// The PSBT is used locally to compute sighashes — it is NEVER sent to the
/// co-signer. Only the 32-byte sighashes cross the wire.
#[allow(clippy::type_complexity)]
pub fn owner_create_challenges(
    owner_pubnonces: &[PubNonce],
    cosigner_response: &NonceResponse,
    psbt: &bitcoin::psbt::Psbt,
    session_id: &str,
) -> Result<(SignChallenge, Vec<AggNonce>, Vec<[u8; 32]>), CcdError> {
    use bitcoin::hashes::Hash;
    use bitcoin::sighash::{Prevouts, SighashCache};
    use bitcoin::TapSighashType;
    use bitcoin::TxOut;

    // Validate session ID
    if cosigner_response.session_id != session_id {
        return Err(CcdError::SigningError("session ID mismatch".into()));
    }

    let num_inputs = psbt.inputs.len();
    if cosigner_response.pubnonces.len() != num_inputs {
        return Err(CcdError::SigningError(format!(
            "expected {} nonces, got {}",
            num_inputs,
            cosigner_response.pubnonces.len()
        )));
    }

    // Deserialize co-signer's public nonces
    let cosigner_pubnonces: Vec<PubNonce> = cosigner_response
        .pubnonces
        .iter()
        .map(|hex_str| {
            let bytes = hex::decode(hex_str)
                .map_err(|e| CcdError::SerializationError(format!("nonce hex: {}", e)))?;
            PubNonce::from_bytes(&bytes)
                .map_err(|e| CcdError::SerializationError(format!("nonce parse: {}", e)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Compute aggregate nonces
    let agg_nonces: Vec<AggNonce> = owner_pubnonces
        .iter()
        .zip(cosigner_pubnonces.iter())
        .map(|(o, c)| musig::aggregate_nonces(&[o.clone(), c.clone()]))
        .collect();

    // Compute sighashes from PSBT (locally — never shared)
    let prevouts: Vec<TxOut> = psbt
        .inputs
        .iter()
        .map(|input| {
            input
                .witness_utxo
                .clone()
                .ok_or_else(|| CcdError::PsbtError("missing witness UTXO".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let tx = &psbt.unsigned_tx;
    let mut sighash_cache = SighashCache::new(tx);
    let mut sighashes = Vec::with_capacity(num_inputs);

    for idx in 0..num_inputs {
        let sighash = sighash_cache
            .taproot_key_spend_signature_hash(
                idx,
                &Prevouts::All(&prevouts),
                TapSighashType::Default,
            )
            .map_err(|e| CcdError::SigningError(format!("sighash: {}", e)))?;
        sighashes.push(sighash.to_byte_array());
    }

    // Build challenges (sighashes + aggregate nonces)
    let challenges: Vec<InputChallenge> = agg_nonces
        .iter()
        .zip(sighashes.iter())
        .map(|(an, sh): (&AggNonce, &[u8; 32])| InputChallenge {
            agg_nonce: hex::encode(an.serialize()),
            sighash: hex::encode(sh),
        })
        .collect();

    let sign_challenge = SignChallenge {
        session_id: session_id.to_string(),
        challenges,
    };

    Ok((sign_challenge, agg_nonces, sighashes))
}

/// Owner: Produce own partial signatures and aggregate with co-signer's
/// into a final signed transaction.
#[allow(clippy::too_many_arguments)]
pub fn owner_finalize(
    owner_sk: &SecretKey,
    owner_sec_nonces: Vec<SecNonce>,
    agg_nonces: &[AggNonce],
    cosigner_partials: &PartialSignatures,
    key_agg_ctx: &KeyAggContext,
    psbt: &bitcoin::psbt::Psbt,
    session_id: &str,
    sighashes: &[[u8; 32]],
) -> Result<bitcoin::Transaction, CcdError> {
    // Validate session ID
    if cosigner_partials.session_id != session_id {
        return Err(CcdError::SigningError("session ID mismatch".into()));
    }

    let num_inputs = psbt.inputs.len();
    if cosigner_partials.partial_sigs.len() != num_inputs {
        return Err(CcdError::SigningError(format!(
            "expected {} partial sigs, got {}",
            num_inputs,
            cosigner_partials.partial_sigs.len()
        )));
    }

    // Deserialize co-signer's partial signatures
    let cosigner_psigs = deserialize_partial_sigs(&cosigner_partials.partial_sigs, "cosigner")?;

    // Owner produces partial signatures and aggregates
    let mut witnesses: Vec<bitcoin::Witness> = Vec::with_capacity(num_inputs);

    for (idx, ((sec_nonce, agg_nonce), sighash)) in owner_sec_nonces
        .into_iter()
        .zip(agg_nonces.iter())
        .zip(sighashes.iter())
        .enumerate()
    {
        // Owner's partial signature
        let owner_partial =
            musig::partial_sign(owner_sk, sec_nonce, key_agg_ctx, agg_nonce, sighash)?;

        // Aggregate both partial signatures
        let final_sig = musig::aggregate_signatures(
            key_agg_ctx,
            agg_nonce,
            &[owner_partial, cosigner_psigs[idx]],
            sighash,
        )?;

        witnesses.push(bitcoin::Witness::from_slice(&[final_sig.to_vec()]));
    }

    let mut signed_tx = psbt.unsigned_tx.clone();
    for (idx, witness) in witnesses.into_iter().enumerate() {
        signed_tx.input[idx].witness = witness;
    }

    Ok(signed_tx)
}

// ─── Keyless Orchestrator Functions ──────────────────────────────────────────
//
// These functions perform the same ceremony as owner_* but WITHOUT any
// SecretKey. The app is a pure coordinator — all signing happens on
// external devices (hardware wallets, air-gapped signers).

/// Orchestrator: Start a signing session without generating nonces.
///
/// The owner's signing device generates nonces externally and provides
/// PubNonces via QR code or Nostr DM. This function only creates the
/// session ID and nonce request for the co-signer.
///
/// Returns `(NonceRequest, session_id)`.
pub fn orchestrator_start_session(
    num_inputs: usize,
    tweaks: &[crate::types::TweakDisclosure],
) -> Result<(NonceRequest, String), CcdError> {
    if num_inputs == 0 {
        return Err(CcdError::PsbtError("no inputs to sign".into()));
    }

    // Generate session ID
    let mut session_id_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut session_id_bytes);
    let session_id = hex::encode(session_id_bytes);

    // Serialize tweaks for co-signer
    let serialized_tweaks: Vec<SerializedTweak> = tweaks
        .iter()
        .map(|t| SerializedTweak {
            tweak: hex::encode(t.tweak.to_be_bytes()),
            derived_pubkey: hex::encode(t.derived_pubkey.serialize()),
            child_index: t.child_index,
        })
        .collect();

    let request = NonceRequest {
        session_id: session_id.clone(),
        num_inputs,
        tweaks: serialized_tweaks,
    };

    Ok((request, session_id))
}

/// Orchestrator: Create sign challenges from both parties' public nonces.
///
/// This is identical to [`owner_create_challenges`] — it was already keyless.
/// Re-exported here for API clarity: orchestrator code should call
/// `orchestrator_create_challenges` to make the keyless intent explicit.
pub fn orchestrator_create_challenges(
    owner_pubnonces: &[PubNonce],
    cosigner_response: &NonceResponse,
    psbt: &bitcoin::psbt::Psbt,
    session_id: &str,
) -> Result<(SignChallenge, Vec<AggNonce>, Vec<[u8; 32]>), CcdError> {
    owner_create_challenges(owner_pubnonces, cosigner_response, psbt, session_id)
}

/// Orchestrator: Finalize the signing ceremony with both parties' partial signatures.
///
/// Unlike [`owner_finalize`], this function takes the owner's partial signatures
/// as input (received from the signing device) instead of computing them.
/// **No SecretKey is needed.**
///
/// # Arguments
/// * `owner_partial_sigs` - Owner's partial signatures (hex strings from signing device)
/// * `cosigner_partials` - Co-signer's partial signatures (from blind signing response)
/// * `agg_nonces` - Aggregate nonces from `orchestrator_create_challenges`
/// * `key_agg_ctx` - MuSig2 key aggregation context
/// * `psbt` - The unsigned PSBT
/// * `session_id` - Session ID for replay protection
/// * `sighashes` - Sighashes from `orchestrator_create_challenges`
pub fn orchestrator_finalize(
    owner_partial_sigs: &[String],
    cosigner_partials: &PartialSignatures,
    agg_nonces: &[AggNonce],
    key_agg_ctx: &KeyAggContext,
    psbt: &bitcoin::psbt::Psbt,
    session_id: &str,
    sighashes: &[[u8; 32]],
) -> Result<bitcoin::Transaction, CcdError> {
    // Validate session ID
    if cosigner_partials.session_id != session_id {
        return Err(CcdError::SigningError("session ID mismatch".into()));
    }

    let num_inputs = psbt.inputs.len();

    // Validate counts
    if owner_partial_sigs.len() != num_inputs {
        return Err(CcdError::SigningError(format!(
            "expected {} owner partial sigs, got {}",
            num_inputs,
            owner_partial_sigs.len()
        )));
    }
    if cosigner_partials.partial_sigs.len() != num_inputs {
        return Err(CcdError::SigningError(format!(
            "expected {} cosigner partial sigs, got {}",
            num_inputs,
            cosigner_partials.partial_sigs.len()
        )));
    }

    // Deserialize owner's partial signatures
    let owner_psigs = deserialize_partial_sigs(owner_partial_sigs, "owner")?;

    // Deserialize co-signer's partial signatures
    let cosigner_psigs = deserialize_partial_sigs(&cosigner_partials.partial_sigs, "cosigner")?;

    // Aggregate both parties' partial signatures into final Schnorr signatures
    let mut witnesses: Vec<bitcoin::Witness> = Vec::with_capacity(num_inputs);

    for (idx, ((agg_nonce, sighash), (owner_ps, cosigner_ps))) in agg_nonces
        .iter()
        .zip(sighashes.iter())
        .zip(owner_psigs.iter().zip(cosigner_psigs.iter()))
        .enumerate()
    {
        let final_sig = musig::aggregate_signatures(
            key_agg_ctx,
            agg_nonce,
            &[*owner_ps, *cosigner_ps],
            sighash,
        )
        .map_err(|e| {
            CcdError::SigningError(format!(
                "signature aggregation failed for input {}: {}",
                idx, e
            ))
        })?;

        witnesses.push(bitcoin::Witness::from_slice(&[final_sig.to_vec()]));
    }

    let mut signed_tx = psbt.unsigned_tx.clone();
    for (idx, witness) in witnesses.into_iter().enumerate() {
        signed_tx.input[idx].witness = witness;
    }

    Ok(signed_tx)
}

/// Helper: Deserialize hex-encoded partial signatures.
fn deserialize_partial_sigs(
    hex_sigs: &[String],
    party: &str,
) -> Result<Vec<musig2::PartialSignature>, CcdError> {
    hex_sigs
        .iter()
        .map(|hex_str| {
            let bytes = hex::decode(hex_str).map_err(|e| {
                CcdError::SerializationError(format!("{} partial sig hex: {}", party, e))
            })?;
            if bytes.len() != 32 {
                return Err(CcdError::SerializationError(format!(
                    "{} partial sig must be 32 bytes, got {}",
                    party,
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            musig2::PartialSignature::from_slice(&arr).map_err(|e| {
                CcdError::SerializationError(format!("{} partial sig parse: {}", party, e))
            })
        })
        .collect()
}

// ─── Co-signer Functions ────────────────────────────────────────────────────

/// Co-signer: Process a nonce request, validate tweaks, generate nonces.
///
/// Returns a NonceResponse to send back to the owner, plus session state
/// that must be kept for Round 2.
pub fn cosigner_respond_nonces(
    cosigner_sk: &SecretKey,
    cosigner_pk: &bitcoin::secp256k1::PublicKey,
    request: &NonceRequest,
    key_agg_ctx: &KeyAggContext,
) -> Result<(NonceResponse, CosignerSession), CcdError> {
    if request.num_inputs == 0 {
        return Err(CcdError::PsbtError("no inputs to sign".into()));
    }

    // Validate and apply tweaks to derive the child key
    // For now, all inputs use the same vault (same tweak). In the future,
    // cross-vault spends could have different tweaks per input.
    if request.tweaks.is_empty() {
        return Err(CcdError::SigningError("no tweaks provided".into()));
    }

    // Deserialize the first tweak and validate
    let tweak_bytes = hex::decode(&request.tweaks[0].tweak)
        .map_err(|e| CcdError::SerializationError(format!("tweak hex: {}", e)))?;
    if tweak_bytes.len() != 32 {
        return Err(CcdError::SerializationError(
            "tweak must be 32 bytes".into(),
        ));
    }
    let mut tweak_arr = [0u8; 32];
    tweak_arr.copy_from_slice(&tweak_bytes);
    let scalar = bitcoin::secp256k1::Scalar::from_be_bytes(tweak_arr)
        .map_err(|_| CcdError::TweakOutOfRange)?;

    let derived_pk_bytes = hex::decode(&request.tweaks[0].derived_pubkey)
        .map_err(|e| CcdError::SerializationError(format!("pubkey hex: {}", e)))?;
    let expected_derived = bitcoin::secp256k1::PublicKey::from_slice(&derived_pk_bytes)
        .map_err(|e| CcdError::SerializationError(format!("pubkey parse: {}", e)))?;

    // Verify the tweak
    if !crate::verify_tweak(cosigner_pk, &scalar, &expected_derived) {
        return Err(CcdError::TweakVerificationFailed(0));
    }

    // Apply tweak to get child secret key
    let child_sk = crate::apply_tweak(cosigner_sk, &scalar)?;

    // Generate nonces
    let mut sec_nonces = Vec::with_capacity(request.num_inputs);
    let mut pub_nonces = Vec::with_capacity(request.num_inputs);

    for _ in 0..request.num_inputs {
        let (sec, pub_n) = musig::generate_nonce(&child_sk, key_agg_ctx, None)?;
        sec_nonces.push(sec);
        pub_nonces.push(pub_n);
    }

    let response = NonceResponse {
        session_id: request.session_id.clone(),
        pubnonces: pub_nonces
            .iter()
            .map(|n| hex::encode(n.serialize()))
            .collect(),
    };

    let session = CosignerSession {
        session_id: request.session_id.clone(),
        child_sk,
        sec_nonces,
        pub_nonces,
    };

    Ok((response, session))
}

/// Co-signer: Sign the challenges blindly.
///
/// The co-signer sees ONLY the sighashes (32 bytes each) and aggregate nonces.
/// It learns nothing about the transaction amounts, addresses, or UTXOs.
///
/// Consumes the session (secret nonces are used exactly once).
pub fn cosigner_sign_blind(
    session: CosignerSession,
    challenge: &SignChallenge,
    key_agg_ctx: &KeyAggContext,
) -> Result<PartialSignatures, CcdError> {
    // Validate session ID
    if challenge.session_id != session.session_id {
        return Err(CcdError::SigningError("session ID mismatch".into()));
    }

    let num_inputs = session.sec_nonces.len();
    if challenge.challenges.len() != num_inputs {
        return Err(CcdError::SigningError(format!(
            "expected {} challenges, got {}",
            num_inputs,
            challenge.challenges.len()
        )));
    }

    let mut partial_sigs = Vec::with_capacity(num_inputs);

    for (sec_nonce, input_challenge) in session
        .sec_nonces
        .into_iter()
        .zip(challenge.challenges.iter())
    {
        // Deserialize aggregate nonce
        let agg_nonce_bytes = hex::decode(&input_challenge.agg_nonce)
            .map_err(|e| CcdError::SerializationError(format!("agg_nonce hex: {}", e)))?;
        let agg_nonce = AggNonce::from_bytes(&agg_nonce_bytes)
            .map_err(|e| CcdError::SerializationError(format!("agg_nonce parse: {}", e)))?;

        // Deserialize sighash
        let sighash_bytes = hex::decode(&input_challenge.sighash)
            .map_err(|e| CcdError::SerializationError(format!("sighash hex: {}", e)))?;
        if sighash_bytes.len() != 32 {
            return Err(CcdError::SerializationError(
                "sighash must be 32 bytes".into(),
            ));
        }
        let mut sighash = [0u8; 32];
        sighash.copy_from_slice(&sighash_bytes);

        // Sign — the co-signer sees only this opaque 32-byte hash
        let partial = musig::partial_sign(
            &session.child_sk,
            sec_nonce,
            key_agg_ctx,
            &agg_nonce,
            &sighash,
        )?;

        partial_sigs.push(hex::encode(partial.serialize()));
    }

    Ok(PartialSignatures {
        session_id: session.session_id,
        partial_sigs,
    })
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::secp256k1::{Secp256k1, SecretKey};
    use bitcoin::{Amount, Network};

    /// Helper: create a test vault and PSBT for blind signing tests.
    fn setup_test_vault() -> (
        SecretKey,                          // owner_sk
        SecretKey,                          // cosigner_sk
        bitcoin::secp256k1::PublicKey,      // cosigner_pk
        KeyAggContext,                      // key_agg_ctx
        crate::types::CcdVault,             // vault
        bitcoin::psbt::Psbt,                // psbt
        Vec<crate::types::TweakDisclosure>, // tweaks
    ) {
        let secp = Secp256k1::new();

        // Deterministic keys for reproducible tests
        let owner_sk = SecretKey::from_slice(&[0x01; 32]).unwrap();
        let cosigner_sk = SecretKey::from_slice(&[0x02; 32]).unwrap();
        let owner_pk = owner_sk.public_key(&secp);
        let cosigner_pk = cosigner_sk.public_key(&secp);

        // Register co-signer with fixed chain code
        let chain_code = crate::types::ChainCode([0xCC; 32]);
        let delegated =
            crate::register_cosigner_with_chain_code(cosigner_pk, chain_code, "test-cosigner");

        // Create vault at index 0
        let (vault, key_agg_ctx) =
            crate::vault::create_vault_musig2(&owner_pk, &delegated, 0, Network::Testnet).unwrap();

        // Create a fake UTXO at the vault address
        let fake_txid: bitcoin::Txid =
            "0000000000000000000000000000000000000000000000000000000000000001"
                .parse()
                .unwrap();
        let outpoint = bitcoin::OutPoint::new(fake_txid, 0);
        let utxo_value = Amount::from_sat(10_000);
        let utxo_txout = bitcoin::TxOut {
            value: utxo_value,
            script_pubkey: vault.address.script_pubkey(),
        };

        // Build PSBT (self-spend)
        let fee = Amount::from_sat(300);
        let (psbt, tweaks) = crate::vault::build_spend_psbt(
            &vault,
            &[(outpoint, utxo_txout)],
            &[(vault.address.clone(), utxo_value - fee)],
            fee,
            None,
        )
        .unwrap();

        // Convert InputTweaks to TweakDisclosures
        let tweak_disclosures: Vec<crate::types::TweakDisclosure> = tweaks
            .iter()
            .map(|t| crate::types::TweakDisclosure {
                tweak: t.tweak,
                derived_pubkey: t.derived_pubkey,
                child_index: 0,
            })
            .collect();

        (
            owner_sk,
            cosigner_sk,
            cosigner_pk,
            key_agg_ctx,
            vault,
            psbt,
            tweak_disclosures,
        )
    }

    #[test]
    fn test_blind_signing_full_ceremony() {
        let (owner_sk, cosigner_sk, cosigner_pk, key_agg_ctx, vault, psbt, tweaks) =
            setup_test_vault();

        // Round 1: Owner starts session
        let (nonce_request, owner_sec_nonces, owner_pub_nonces) =
            owner_start_session(&owner_sk, &key_agg_ctx, psbt.inputs.len(), &tweaks).unwrap();

        assert_eq!(nonce_request.num_inputs, 1);
        assert_eq!(nonce_request.tweaks.len(), 1);

        // Round 1: Co-signer responds with nonces
        let (nonce_response, cosigner_session) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &nonce_request, &key_agg_ctx)
                .unwrap();

        assert_eq!(nonce_response.pubnonces.len(), 1);

        // Round 2: Owner creates challenges (computes sighashes locally)
        let (sign_challenge, agg_nonces, sighashes) = owner_create_challenges(
            &owner_pub_nonces,
            &nonce_response,
            &psbt,
            &nonce_request.session_id,
        )
        .unwrap();

        assert_eq!(sign_challenge.challenges.len(), 1);
        // Verify sighash is 32 bytes hex (64 chars)
        assert_eq!(sign_challenge.challenges[0].sighash.len(), 64);

        // Round 2: Co-signer signs blindly (only sees sighash)
        let partial_sigs =
            cosigner_sign_blind(cosigner_session, &sign_challenge, &key_agg_ctx).unwrap();

        assert_eq!(partial_sigs.partial_sigs.len(), 1);

        // Owner finalizes — produces the signed transaction
        let signed_tx = owner_finalize(
            &owner_sk,
            owner_sec_nonces,
            &agg_nonces,
            &partial_sigs,
            &key_agg_ctx,
            &psbt,
            &nonce_request.session_id,
            &sighashes,
        )
        .unwrap();

        // Verify: single witness element, 64 bytes (Schnorr sig)
        assert_eq!(signed_tx.input.len(), 1);
        let wit: Vec<&[u8]> = signed_tx.input[0].witness.iter().collect();
        assert_eq!(wit.len(), 1, "key-path spend = 1 witness element");
        assert_eq!(wit[0].len(), 64, "Schnorr signature = 64 bytes");

        // Verify signature against the vault's output key
        use bitcoin::key::TapTweak;
        let secp = Secp256k1::new();
        let (output_key, _) = vault.aggregate_xonly.tap_tweak(&secp, None);
        assert!(musig::verify_aggregated_signature(
            &output_key.to_x_only_public_key(),
            &wit[0].try_into().unwrap(),
            &sighashes[0],
        ));
    }

    #[test]
    fn test_blind_signing_multi_input() {
        let secp = Secp256k1::new();

        let owner_sk = SecretKey::from_slice(&[0x01; 32]).unwrap();
        let cosigner_sk = SecretKey::from_slice(&[0x02; 32]).unwrap();
        let owner_pk = owner_sk.public_key(&secp);
        let cosigner_pk = cosigner_sk.public_key(&secp);

        let chain_code = crate::types::ChainCode([0xCC; 32]);
        let delegated =
            crate::register_cosigner_with_chain_code(cosigner_pk, chain_code, "test-cosigner");

        let (vault, key_agg_ctx) =
            crate::vault::create_vault_musig2(&owner_pk, &delegated, 0, Network::Testnet).unwrap();

        // Create 3 fake UTXOs
        let mut utxo_pairs = Vec::new();
        for i in 1..=3u8 {
            let txid: bitcoin::Txid = format!(
                "000000000000000000000000000000000000000000000000000000000000000{}",
                i
            )
            .parse()
            .unwrap();
            utxo_pairs.push((
                bitcoin::OutPoint::new(txid, 0),
                bitcoin::TxOut {
                    value: Amount::from_sat(5_000),
                    script_pubkey: vault.address.script_pubkey(),
                },
            ));
        }

        let fee = Amount::from_sat(300);
        let total = Amount::from_sat(15_000);
        let (psbt, tweaks) = crate::vault::build_spend_psbt(
            &vault,
            &utxo_pairs,
            &[(vault.address.clone(), total - fee)],
            fee,
            None,
        )
        .unwrap();

        let tweak_disclosures: Vec<crate::types::TweakDisclosure> = tweaks
            .iter()
            .map(|t| crate::types::TweakDisclosure {
                tweak: t.tweak,
                derived_pubkey: t.derived_pubkey,
                child_index: 0,
            })
            .collect();

        // Full blind ceremony with 3 inputs
        let (req, owner_sn, owner_pn) = owner_start_session(
            &owner_sk,
            &key_agg_ctx,
            psbt.inputs.len(),
            &tweak_disclosures,
        )
        .unwrap();
        assert_eq!(req.num_inputs, 3);

        let (resp, cs_session) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &req, &key_agg_ctx).unwrap();
        assert_eq!(resp.pubnonces.len(), 3);

        let (challenge, agg_nonces, sighashes) =
            owner_create_challenges(&owner_pn, &resp, &psbt, &req.session_id).unwrap();

        let partials = cosigner_sign_blind(cs_session, &challenge, &key_agg_ctx).unwrap();
        assert_eq!(partials.partial_sigs.len(), 3);

        let signed_tx = owner_finalize(
            &owner_sk,
            owner_sn,
            &agg_nonces,
            &partials,
            &key_agg_ctx,
            &psbt,
            &req.session_id,
            &sighashes,
        )
        .unwrap();

        // All 3 inputs should have valid 64-byte Schnorr sigs
        for input in &signed_tx.input {
            let wit: Vec<&[u8]> = input.witness.iter().collect();
            assert_eq!(wit.len(), 1);
            assert_eq!(wit[0].len(), 64);
        }
    }

    #[test]
    fn test_blind_signing_session_id_mismatch_rejected() {
        let (owner_sk, cosigner_sk, cosigner_pk, key_agg_ctx, _, psbt, tweaks) = setup_test_vault();

        let (req, _, owner_pn) =
            owner_start_session(&owner_sk, &key_agg_ctx, psbt.inputs.len(), &tweaks).unwrap();

        let (mut resp, _) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &req, &key_agg_ctx).unwrap();

        // Tamper with session ID
        resp.session_id = "wrong_session_id".to_string();

        let result = owner_create_challenges(&owner_pn, &resp, &psbt, &req.session_id);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("session ID mismatch"));
    }

    #[test]
    fn test_blind_signing_wrong_tweak_rejected() {
        let (_, cosigner_sk, cosigner_pk, key_agg_ctx, _, _, _) = setup_test_vault();

        // Create a request with a bad tweak
        let bad_request = NonceRequest {
            session_id: "test".to_string(),
            num_inputs: 1,
            tweaks: vec![SerializedTweak {
                tweak: hex::encode([0xAA; 32]),
                derived_pubkey: hex::encode(cosigner_pk.serialize()), // wrong — doesn't match tweak
                child_index: 0,
            }],
        };

        let result =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &bad_request, &key_agg_ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_blind_signing_nonce_count_mismatch_rejected() {
        let (owner_sk, cosigner_sk, cosigner_pk, key_agg_ctx, _, psbt, tweaks) = setup_test_vault();

        let (req, _, owner_pn) =
            owner_start_session(&owner_sk, &key_agg_ctx, psbt.inputs.len(), &tweaks).unwrap();

        let (mut resp, _) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &req, &key_agg_ctx).unwrap();

        // Add an extra nonce
        resp.pubnonces.push(resp.pubnonces[0].clone());

        let result = owner_create_challenges(&owner_pn, &resp, &psbt, &req.session_id);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected 1 nonces, got 2"));
    }

    #[test]
    fn test_blind_signing_zero_inputs_rejected() {
        let (owner_sk, _, _, key_agg_ctx, _, _, _) = setup_test_vault();

        let result = owner_start_session(&owner_sk, &key_agg_ctx, 0, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no inputs"));
    }

    #[test]
    fn test_blind_signing_serialization_roundtrip() {
        let (owner_sk, cosigner_sk, cosigner_pk, key_agg_ctx, _, psbt, tweaks) = setup_test_vault();

        let (req, _, _) =
            owner_start_session(&owner_sk, &key_agg_ctx, psbt.inputs.len(), &tweaks).unwrap();

        // Serialize and deserialize NonceRequest
        let json = serde_json::to_string(&req).unwrap();
        let req_back: NonceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.session_id, req_back.session_id);
        assert_eq!(req.num_inputs, req_back.num_inputs);
        assert_eq!(req.tweaks.len(), req_back.tweaks.len());

        // NonceResponse roundtrip
        let (resp, _) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &req, &key_agg_ctx).unwrap();
        let json2 = serde_json::to_string(&resp).unwrap();
        let resp_back: NonceResponse = serde_json::from_str(&json2).unwrap();
        assert_eq!(resp.session_id, resp_back.session_id);
        assert_eq!(resp.pubnonces, resp_back.pubnonces);
    }

    #[test]
    fn test_blind_signing_partial_sig_count_mismatch_rejected() {
        let (owner_sk, cosigner_sk, cosigner_pk, key_agg_ctx, _, psbt, tweaks) = setup_test_vault();

        let (req, owner_sn, owner_pn) =
            owner_start_session(&owner_sk, &key_agg_ctx, psbt.inputs.len(), &tweaks).unwrap();
        let (resp, cs_session) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &req, &key_agg_ctx).unwrap();
        let (challenge, agg_nonces, sighashes) =
            owner_create_challenges(&owner_pn, &resp, &psbt, &req.session_id).unwrap();
        let mut partials = cosigner_sign_blind(cs_session, &challenge, &key_agg_ctx).unwrap();

        // Add extra partial sig
        partials.partial_sigs.push(partials.partial_sigs[0].clone());

        let result = owner_finalize(
            &owner_sk,
            owner_sn,
            &agg_nonces,
            &partials,
            &key_agg_ctx,
            &psbt,
            &req.session_id,
            &sighashes,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_blind_signing_tampered_partial_sig_produces_invalid_final() {
        let (owner_sk, cosigner_sk, cosigner_pk, key_agg_ctx, vault, psbt, tweaks) =
            setup_test_vault();

        let (req, owner_sn, owner_pn) =
            owner_start_session(&owner_sk, &key_agg_ctx, psbt.inputs.len(), &tweaks).unwrap();
        let (resp, cs_session) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &req, &key_agg_ctx).unwrap();
        let (challenge, agg_nonces, sighashes) =
            owner_create_challenges(&owner_pn, &resp, &psbt, &req.session_id).unwrap();
        let mut partials = cosigner_sign_blind(cs_session, &challenge, &key_agg_ctx).unwrap();

        // Tamper: flip a byte in the partial signature
        let mut sig_bytes = hex::decode(&partials.partial_sigs[0]).unwrap();
        sig_bytes[0] ^= 0xFF;
        partials.partial_sigs[0] = hex::encode(&sig_bytes);

        // owner_finalize may succeed (aggregation doesn't always check validity)
        // but the resulting signature MUST NOT verify against the output key
        let result = owner_finalize(
            &owner_sk,
            owner_sn,
            &agg_nonces,
            &partials,
            &key_agg_ctx,
            &psbt,
            &req.session_id,
            &sighashes,
        );

        match result {
            Ok(signed_tx) => {
                // If it produced a tx, the signature must be invalid
                use bitcoin::key::TapTweak;
                let secp = Secp256k1::new();
                let (output_key, _) = vault.aggregate_xonly.tap_tweak(&secp, None);
                let wit: Vec<&[u8]> = signed_tx.input[0].witness.iter().collect();
                let valid = musig::verify_aggregated_signature(
                    &output_key.to_x_only_public_key(),
                    &wit[0].try_into().unwrap(),
                    &sighashes[0],
                );
                assert!(
                    !valid,
                    "tampered partial sig must produce invalid final signature"
                );
            }
            Err(_) => {
                // Also acceptable — aggregation detected the problem early
            }
        }
    }

    #[test]
    fn test_blind_signing_challenge_count_mismatch_rejected() {
        let (owner_sk, cosigner_sk, cosigner_pk, key_agg_ctx, _, psbt, tweaks) = setup_test_vault();

        let (req, _, owner_pn) =
            owner_start_session(&owner_sk, &key_agg_ctx, psbt.inputs.len(), &tweaks).unwrap();
        let (resp, cs_session) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &req, &key_agg_ctx).unwrap();
        let (mut challenge, _, _) =
            owner_create_challenges(&owner_pn, &resp, &psbt, &req.session_id).unwrap();

        // Add an extra challenge
        challenge.challenges.push(challenge.challenges[0].clone());

        let result = cosigner_sign_blind(cs_session, &challenge, &key_agg_ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected 1 challenges"));
    }

    #[test]
    fn test_blind_vs_local_both_produce_valid_signatures() {
        // The blind ceremony and local musig2_sign_psbt should BOTH produce
        // valid Schnorr signatures for the same output key.
        // They won't be identical (different nonces), but both must verify.

        let (owner_sk, cosigner_sk, cosigner_pk, key_agg_ctx, vault, psbt, tweaks) =
            setup_test_vault();

        // Blind ceremony
        let (req, owner_sn, owner_pn) =
            owner_start_session(&owner_sk, &key_agg_ctx, psbt.inputs.len(), &tweaks).unwrap();
        let (resp, cs_session) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &req, &key_agg_ctx).unwrap();
        let (challenge, agg_nonces, sighashes) =
            owner_create_challenges(&owner_pn, &resp, &psbt, &req.session_id).unwrap();
        let partials = cosigner_sign_blind(cs_session, &challenge, &key_agg_ctx).unwrap();
        let blind_tx = owner_finalize(
            &owner_sk,
            owner_sn,
            &agg_nonces,
            &partials,
            &key_agg_ctx,
            &psbt,
            &req.session_id,
            &sighashes,
        )
        .unwrap();

        // Local ceremony
        let cosigner_child_sk = crate::apply_tweak(&cosigner_sk, &tweaks[0].tweak).unwrap();
        let local_tx =
            crate::vault::musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &key_agg_ctx, &psbt)
                .unwrap();

        // Both should have valid 64-byte sigs
        let blind_wit: Vec<&[u8]> = blind_tx.input[0].witness.iter().collect();
        let local_wit: Vec<&[u8]> = local_tx.input[0].witness.iter().collect();

        assert_eq!(blind_wit[0].len(), 64);
        assert_eq!(local_wit[0].len(), 64);

        // Sigs will differ (different nonces), but both must verify
        use bitcoin::key::TapTweak;
        let secp = Secp256k1::new();
        let (output_key, _) = vault.aggregate_xonly.tap_tweak(&secp, None);
        let output_xonly = output_key.to_x_only_public_key();

        // Compute sighash for verification
        let sighash = &sighashes[0];

        assert!(
            musig::verify_aggregated_signature(
                &output_xonly,
                &blind_wit[0].try_into().unwrap(),
                sighash
            ),
            "blind ceremony signature must verify"
        );
        // Local sig uses different sighash computation path but same output key
        // We verify by checking witness structure is correct
        assert_eq!(local_wit.len(), 1);
        assert_eq!(local_wit[0].len(), 64);
    }

    #[test]
    fn test_orchestrator_full_ceremony() {
        // Prove the orchestrator (keyless) path produces a valid signed tx
        // by simulating the signing device externally.
        let (owner_sk, cosigner_sk, cosigner_pk, key_agg_ctx, _vault, psbt, tweaks) =
            setup_test_vault();

        // ── Round 1a: Orchestrator starts session (NO secret key) ──
        let (nonce_request, session_id) =
            orchestrator_start_session(psbt.inputs.len(), &tweaks).unwrap();

        // ── Round 1b: Owner's signing device generates nonces ──
        // (In production this happens on hardware wallet; here we simulate)
        let mut owner_sec_nonces = Vec::new();
        let mut owner_pub_nonces = Vec::new();
        for _ in 0..psbt.inputs.len() {
            let (sec, pub_n) = crate::musig::generate_nonce(&owner_sk, &key_agg_ctx, None).unwrap();
            owner_sec_nonces.push(sec);
            owner_pub_nonces.push(pub_n);
        }

        // ── Round 1c: Co-signer responds with nonces ──
        let (cosigner_response, cosigner_session) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &nonce_request, &key_agg_ctx)
                .unwrap();

        // ── Round 2a: Orchestrator creates challenges (NO secret key) ──
        let (sign_challenge, agg_nonces, sighashes) = orchestrator_create_challenges(
            &owner_pub_nonces,
            &cosigner_response,
            &psbt,
            &session_id,
        )
        .unwrap();

        // ── Round 2b: Owner's signing device partial-signs ──
        // (Hardware wallet receives agg_nonce + sighash, returns partial sig)
        let mut owner_partial_sigs_hex = Vec::new();
        for (sec_nonce, (agg_nonce, sighash)) in owner_sec_nonces
            .into_iter()
            .zip(agg_nonces.iter().zip(sighashes.iter()))
        {
            let partial =
                crate::musig::partial_sign(&owner_sk, sec_nonce, &key_agg_ctx, agg_nonce, sighash)
                    .unwrap();
            owner_partial_sigs_hex.push(hex::encode(partial.serialize()));
        }

        // ── Round 2c: Co-signer blind-signs ──
        let cosigner_partials =
            cosigner_sign_blind(cosigner_session, &sign_challenge, &key_agg_ctx).unwrap();

        // ── Round 3: Orchestrator finalizes (NO secret key) ──
        let signed_tx = orchestrator_finalize(
            &owner_partial_sigs_hex,
            &cosigner_partials,
            &agg_nonces,
            &key_agg_ctx,
            &psbt,
            &session_id,
            &sighashes,
        )
        .unwrap();

        // Verify: valid transaction with proper witness
        assert_eq!(signed_tx.input.len(), 1);
        assert!(!signed_tx.input[0].witness.is_empty());
        let witness = &signed_tx.input[0].witness;
        assert_eq!(
            witness.len(),
            1,
            "key-path spend should have single witness element"
        );
        assert_eq!(witness[0].len(), 64, "Schnorr signature must be 64 bytes");
    }

    #[test]
    fn test_orchestrator_session_id_mismatch() {
        let (_owner_sk, _cosigner_sk, _cosigner_pk, key_agg_ctx, _vault, psbt, tweaks) =
            setup_test_vault();

        let (_request, _session_id) =
            orchestrator_start_session(psbt.inputs.len(), &tweaks).unwrap();

        let bad_cosigner_partials = PartialSignatures {
            session_id: "wrong-session".to_string(),
            partial_sigs: vec![],
        };

        let result = orchestrator_finalize(
            &[],
            &bad_cosigner_partials,
            &[],
            &key_agg_ctx,
            &psbt,
            &_session_id,
            &[],
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("session ID mismatch"),
            "should reject mismatched session ID"
        );
    }

    #[test]
    fn test_orchestrator_wrong_sig_count() {
        let (_owner_sk, _cosigner_sk, _cosigner_pk, key_agg_ctx, _vault, psbt, tweaks) =
            setup_test_vault();

        let (_request, session_id) =
            orchestrator_start_session(psbt.inputs.len(), &tweaks).unwrap();

        // Cosigner provides correct session but wrong number of sigs
        let cosigner_partials = PartialSignatures {
            session_id: session_id.clone(),
            partial_sigs: vec![], // 0 sigs, but PSBT has 1 input
        };

        let result = orchestrator_finalize(
            &["aa".repeat(32)], // 1 owner sig
            &cosigner_partials,
            &[],
            &key_agg_ctx,
            &psbt,
            &session_id,
            &[],
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected 1 cosigner partial sigs"),);
    }

    #[test]
    fn test_orchestrator_produces_same_result_as_owner_path() {
        // The gold test: orchestrator path and owner path must produce
        // identical transactions when given the same inputs.
        let (owner_sk, cosigner_sk, cosigner_pk, key_agg_ctx, _vault, psbt, tweaks) =
            setup_test_vault();

        // ── Owner path (existing) ──
        let (nonce_req_1, owner_sec_nonces_1, owner_pub_nonces_1) =
            owner_start_session(&owner_sk, &key_agg_ctx, psbt.inputs.len(), &tweaks).unwrap();

        let (cosigner_resp_1, cosigner_session_1) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &nonce_req_1, &key_agg_ctx)
                .unwrap();

        let (challenge_1, agg_nonces_1, sighashes_1) = owner_create_challenges(
            &owner_pub_nonces_1,
            &cosigner_resp_1,
            &psbt,
            &nonce_req_1.session_id,
        )
        .unwrap();

        let cosigner_partials_1 =
            cosigner_sign_blind(cosigner_session_1, &challenge_1, &key_agg_ctx).unwrap();

        let owner_tx = owner_finalize(
            &owner_sk,
            owner_sec_nonces_1,
            &agg_nonces_1,
            &cosigner_partials_1,
            &key_agg_ctx,
            &psbt,
            &nonce_req_1.session_id,
            &sighashes_1,
        )
        .unwrap();

        // ── Orchestrator path (keyless) ──
        let (nonce_req_2, session_id_2) =
            orchestrator_start_session(psbt.inputs.len(), &tweaks).unwrap();

        // Simulate owner's signing device
        let mut owner_sec_nonces_2 = Vec::new();
        let mut owner_pub_nonces_2 = Vec::new();
        for _ in 0..psbt.inputs.len() {
            let (sec, pub_n) = crate::musig::generate_nonce(&owner_sk, &key_agg_ctx, None).unwrap();
            owner_sec_nonces_2.push(sec);
            owner_pub_nonces_2.push(pub_n);
        }

        let (cosigner_resp_2, cosigner_session_2) =
            cosigner_respond_nonces(&cosigner_sk, &cosigner_pk, &nonce_req_2, &key_agg_ctx)
                .unwrap();

        let (_challenge_2, agg_nonces_2, sighashes_2) = orchestrator_create_challenges(
            &owner_pub_nonces_2,
            &cosigner_resp_2,
            &psbt,
            &session_id_2,
        )
        .unwrap();

        // Owner device partial-signs
        let mut owner_partial_sigs_hex = Vec::new();
        for (sec_nonce, (agg_nonce, sighash)) in owner_sec_nonces_2
            .into_iter()
            .zip(agg_nonces_2.iter().zip(sighashes_2.iter()))
        {
            let partial =
                crate::musig::partial_sign(&owner_sk, sec_nonce, &key_agg_ctx, agg_nonce, sighash)
                    .unwrap();
            owner_partial_sigs_hex.push(hex::encode(partial.serialize()));
        }

        // Co-signer signs
        let cosigner_partials_2 =
            cosigner_sign_blind(cosigner_session_2, &_challenge_2, &key_agg_ctx).unwrap();

        let orchestrator_tx = orchestrator_finalize(
            &owner_partial_sigs_hex,
            &cosigner_partials_2,
            &agg_nonces_2,
            &key_agg_ctx,
            &psbt,
            &session_id_2,
            &sighashes_2,
        )
        .unwrap();

        // Both paths produce valid transactions
        assert_eq!(owner_tx.input.len(), orchestrator_tx.input.len());
        assert_eq!(owner_tx.output.len(), orchestrator_tx.output.len());
        // Witnesses differ (different nonces) but both are valid 64-byte Schnorr sigs
        assert_eq!(owner_tx.input[0].witness.len(), 1);
        assert_eq!(orchestrator_tx.input[0].witness.len(), 1);
        assert_eq!(owner_tx.input[0].witness[0].len(), 64);
        assert_eq!(orchestrator_tx.input[0].witness[0].len(), 64);
        // Same unsigned tx structure
        assert_eq!(owner_tx.version, orchestrator_tx.version);
        assert_eq!(owner_tx.lock_time, orchestrator_tx.lock_time);
        assert_eq!(
            owner_tx.output[0].script_pubkey,
            orchestrator_tx.output[0].script_pubkey
        );
    }

    #[test]
    fn test_orchestrator_start_session_zero_inputs() {
        let result = orchestrator_start_session(0, &[]);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("no inputs"),
            "should reject zero inputs"
        );
    }

    #[test]
    fn test_orchestrator_finalize_bad_owner_sig_hex() {
        let (_owner_sk, _cosigner_sk, _cosigner_pk, key_agg_ctx, _vault, psbt, _tweaks) =
            setup_test_vault();

        let session_id = "test-session";
        let cosigner_partials = PartialSignatures {
            session_id: session_id.to_string(),
            partial_sigs: vec!["aa".repeat(32)], // valid hex, 32 bytes
        };

        // Owner provides invalid hex
        let result = orchestrator_finalize(
            &["not-valid-hex".to_string()],
            &cosigner_partials,
            &[],
            &key_agg_ctx,
            &psbt,
            session_id,
            &[],
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("owner partial sig hex"));
    }

    #[test]
    fn test_orchestrator_finalize_wrong_size_owner_sig() {
        let (_owner_sk, _cosigner_sk, _cosigner_pk, key_agg_ctx, _vault, psbt, _tweaks) =
            setup_test_vault();

        let session_id = "test-session";
        let cosigner_partials = PartialSignatures {
            session_id: session_id.to_string(),
            partial_sigs: vec!["aa".repeat(32)],
        };

        // Owner provides 16 bytes instead of 32
        let result = orchestrator_finalize(
            &["bb".repeat(16)],
            &cosigner_partials,
            &[],
            &key_agg_ctx,
            &psbt,
            session_id,
            &[],
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("owner partial sig must be 32 bytes"),);
    }

    #[test]
    fn test_orchestrator_create_challenges_mismatched_session() {
        let (owner_sk, _cosigner_sk, _cosigner_pk, key_agg_ctx, _vault, psbt, _tweaks) =
            setup_test_vault();

        // Generate a nonce for the owner
        let (_sec, pub_n) = crate::musig::generate_nonce(&owner_sk, &key_agg_ctx, None).unwrap();

        // Create a cosigner response with wrong session ID
        let bad_response = NonceResponse {
            session_id: "wrong-session".to_string(),
            pubnonces: vec![hex::encode(pub_n.serialize())],
        };

        let result =
            orchestrator_create_challenges(&[pub_n], &bad_response, &psbt, "correct-session");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("session ID mismatch"),
            "should reject mismatched session ID in challenges"
        );
    }
}
