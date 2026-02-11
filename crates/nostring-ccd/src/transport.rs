//! Nostr transport layer for CCD protocol messages.
//!
//! Uses NIP-44 encrypted direct messages to exchange tweak disclosures
//! and blind signing messages between owner and co-signer.
//! No HTTPS server required.
//!
//! # Message Types
//!
//! All CCD protocol messages are wrapped in a [`CcdMessage`] envelope
//! for unified serialization and dispatch:
//!
//! - `TweakRequest` / `TweakAck` — tweak exchange (Phase 1)
//! - `NonceRequest` / `NonceResponse` — blind signing Round 1 (Phase 5a)
//! - `SignChallenge` / `PartialSignatures` — blind signing Round 2 (Phase 5a)

use crate::blind;
use crate::types::*;
use bitcoin::secp256k1::{PublicKey, Scalar};
use serde::{Deserialize, Serialize};

// ─── Unified Message Envelope ───────────────────────────────────────────────

/// Unified CCD protocol message envelope.
///
/// All messages serialize to JSON with a `"type"` tag for dispatch.
/// The JSON is then NIP-44 encrypted and sent as a Nostr DM.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "ccd_type")]
pub enum CcdMessage {
    // Tweak exchange (Phase 1)
    #[serde(rename = "tweak_request")]
    TweakRequest(TweakRequest),
    #[serde(rename = "tweak_ack")]
    TweakAck(TweakAck),
    // Blind signing Round 1 (Phase 5a)
    #[serde(rename = "nonce_request")]
    NonceRequest(blind::NonceRequest),
    #[serde(rename = "nonce_response")]
    NonceResponse(blind::NonceResponse),
    // Blind signing Round 2 (Phase 5a)
    #[serde(rename = "sign_challenge")]
    SignChallenge(blind::SignChallenge),
    #[serde(rename = "partial_signatures")]
    PartialSignatures(blind::PartialSignatures),
}

/// Serialize any CCD message to JSON for NIP-44 encryption.
pub fn serialize_message(msg: &CcdMessage) -> Result<String, CcdError> {
    serde_json::to_string(msg).map_err(|e| CcdError::SerializationError(e.to_string()))
}

/// Deserialize a JSON string into a CCD message.
pub fn deserialize_message(json: &str) -> Result<CcdMessage, CcdError> {
    serde_json::from_str(json).map_err(|e| CcdError::SerializationError(e.to_string()))
}

/// Decode a decrypted NIP-44 DM content string into a CcdMessage.
///
/// The caller is responsible for:
/// 1. Subscribing to Nostr DM events (kind 4 or gift-wrap)
/// 2. Filtering by sender pubkey
/// 3. Decrypting the NIP-44 content
/// 4. Passing the plaintext JSON to this function
///
/// # Example (with nostr-sdk)
/// ```ignore
/// // After receiving and decrypting a DM:
/// let plaintext = nip44::decrypt(sender_pk, &encrypted_content)?;
/// let msg = decode_dm_content(&plaintext)?;
/// match msg {
///     CcdMessage::NonceRequest(req) => handle_nonce_request(req),
///     CcdMessage::SignChallenge(ch) => handle_sign_challenge(ch),
///     _ => { /* ... */ }
/// }
/// ```
pub fn decode_dm_content(content: &str) -> Result<CcdMessage, CcdError> {
    deserialize_message(content)
}

/// Prepare a CCD message for sending as a NIP-44 DM.
///
/// Returns the JSON string to be NIP-44 encrypted and sent.
/// The caller handles the actual Nostr event construction and sending.
///
/// # Example (with nostr-sdk)
/// ```ignore
/// let json = prepare_dm_content(&CcdMessage::NonceRequest(req))?;
/// let encrypted = nip44::encrypt(recipient_pk, &json)?;
/// client.send_event_builder(EventBuilder::new(Kind::EncryptedDirectMessage, encrypted, &[])).await?;
/// ```
pub fn prepare_dm_content(message: &CcdMessage) -> Result<String, CcdError> {
    serialize_message(message)
}

// ─── Legacy Functions (backward compatible) ─────────────────────────────────

/// Encode a TweakDisclosure into a serializable TweakRequest.
pub fn encode_tweak_request(
    disclosure: &TweakDisclosure,
    outpoint: Option<&str>,
) -> Result<TweakRequest, CcdError> {
    Ok(TweakRequest {
        version: 1,
        msg_type: "tweak_request".into(),
        tweak: hex::encode(disclosure.tweak.to_be_bytes()),
        derived_pubkey: hex::encode(disclosure.derived_pubkey.serialize()),
        child_index: disclosure.child_index,
        outpoint: outpoint.map(|s| s.to_string()),
    })
}

/// Encode a TweakRequest to JSON string (for NIP-44 encryption).
pub fn serialize_request(request: &TweakRequest) -> Result<String, CcdError> {
    serde_json::to_string(request).map_err(|e| CcdError::SerializationError(e.to_string()))
}

/// Decode a JSON string into a TweakRequest.
pub fn deserialize_request(json: &str) -> Result<TweakRequest, CcdError> {
    serde_json::from_str(json).map_err(|e| CcdError::SerializationError(e.to_string()))
}

/// Parse a TweakRequest back into a TweakDisclosure.
pub fn decode_tweak_request(request: &TweakRequest) -> Result<TweakDisclosure, CcdError> {
    if request.version != 1 {
        return Err(CcdError::TransportError(format!(
            "unsupported version: {}",
            request.version
        )));
    }
    if request.msg_type != "tweak_request" {
        return Err(CcdError::TransportError(format!(
            "unexpected message type: {}",
            request.msg_type
        )));
    }

    // Parse tweak scalar from hex
    let tweak_bytes = hex::decode(&request.tweak)
        .map_err(|e| CcdError::SerializationError(format!("invalid tweak hex: {}", e)))?;
    if tweak_bytes.len() != 32 {
        return Err(CcdError::SerializationError(
            "tweak must be 32 bytes".into(),
        ));
    }
    let mut tweak_arr = [0u8; 32];
    tweak_arr.copy_from_slice(&tweak_bytes);
    let tweak = Scalar::from_be_bytes(tweak_arr).map_err(|_| CcdError::TweakOutOfRange)?;

    // Parse derived pubkey from hex
    let pk_bytes = hex::decode(&request.derived_pubkey)
        .map_err(|e| CcdError::SerializationError(format!("invalid pubkey hex: {}", e)))?;
    let derived_pubkey = PublicKey::from_slice(&pk_bytes)
        .map_err(|e| CcdError::SerializationError(format!("invalid pubkey: {}", e)))?;

    Ok(TweakDisclosure {
        tweak,
        derived_pubkey,
        child_index: request.child_index,
    })
}

/// Create a tweak acknowledgment message.
pub fn encode_tweak_ack(derived_pubkey: &PublicKey, accepted: bool) -> TweakAck {
    TweakAck {
        version: 1,
        msg_type: "tweak_ack".into(),
        derived_pubkey: hex::encode(derived_pubkey.serialize()),
        accepted,
    }
}

/// Serialize a TweakAck to JSON.
pub fn serialize_ack(ack: &TweakAck) -> Result<String, CcdError> {
    serde_json::to_string(ack).map_err(|e| CcdError::SerializationError(e.to_string()))
}

/// Deserialize a TweakAck from JSON.
pub fn deserialize_ack(json: &str) -> Result<TweakAck, CcdError> {
    serde_json::from_str(json).map_err(|e| CcdError::SerializationError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        SerializedInputTweak, SerializedPartialSig, SigningResponseMessage, SigningSessionMessage,
    };
    use crate::{compute_tweak, register_cosigner};
    use bitcoin::secp256k1::{Secp256k1, SecretKey};

    fn test_keypair() -> (SecretKey, PublicKey) {
        let secp = Secp256k1::new();
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01;
        bytes[31] = 42;
        let sk = SecretKey::from_slice(&bytes).unwrap();
        let pk = sk.public_key(&secp);
        (sk, pk)
    }

    #[test]
    fn test_transport_roundtrip() {
        let (_sk, pk) = test_keypair();
        let delegated = register_cosigner(pk, "test");
        let disclosure = compute_tweak(&delegated, 7).unwrap();

        // Encode
        let request = encode_tweak_request(&disclosure, Some("abc123:0")).unwrap();
        assert_eq!(request.version, 1);
        assert_eq!(request.msg_type, "tweak_request");
        assert_eq!(request.child_index, 7);
        assert_eq!(request.outpoint, Some("abc123:0".to_string()));

        // Serialize to JSON
        let json = serialize_request(&request).unwrap();
        assert!(json.contains("tweak_request"));

        // Deserialize
        let parsed = deserialize_request(&json).unwrap();
        assert_eq!(parsed.tweak, request.tweak);
        assert_eq!(parsed.child_index, request.child_index);

        // Decode back to TweakDisclosure
        let decoded = decode_tweak_request(&parsed).unwrap();
        assert_eq!(decoded.tweak, disclosure.tweak);
        assert_eq!(decoded.derived_pubkey, disclosure.derived_pubkey);
        assert_eq!(decoded.child_index, 7);
    }

    #[test]
    fn test_ack_roundtrip() {
        let (_sk, pk) = test_keypair();

        let ack = encode_tweak_ack(&pk, true);
        let json = serialize_ack(&ack).unwrap();
        let parsed = deserialize_ack(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.msg_type, "tweak_ack");
        assert!(parsed.accepted);
    }

    #[test]
    fn test_invalid_version_rejected() {
        let request = TweakRequest {
            version: 99,
            msg_type: "tweak_request".into(),
            tweak: "00".repeat(32),
            derived_pubkey: "00".repeat(33),
            child_index: 0,
            outpoint: None,
        };

        let result = decode_tweak_request(&request);
        assert!(matches!(result, Err(CcdError::TransportError(_))));
    }

    #[test]
    fn test_malformed_json_rejected() {
        // Truncated JSON
        let result = deserialize_request("{\"version\": 1, \"type\":");
        assert!(result.is_err());

        // Empty string
        let result = deserialize_request("");
        assert!(result.is_err());

        // Valid JSON but missing fields
        let result = deserialize_request(r#"{"version": 1}"#);
        assert!(result.is_err());

        // Wrong type for field
        let result = deserialize_request(
            r#"{"version": "one", "type": "tweak_request", "tweak": "aa", "derived_pubkey": "bb", "child_index": 0}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_hex_in_request_rejected() {
        let request = TweakRequest {
            version: 1,
            msg_type: "tweak_request".into(),
            tweak: "not_hex_at_all!".into(),
            derived_pubkey: "00".repeat(33),
            child_index: 0,
            outpoint: None,
        };
        let result = decode_tweak_request(&request);
        assert!(matches!(result, Err(CcdError::SerializationError(_))));

        // Valid hex but wrong length for tweak
        let request2 = TweakRequest {
            version: 1,
            msg_type: "tweak_request".into(),
            tweak: "abcd".into(), // 2 bytes, not 32
            derived_pubkey: "00".repeat(33),
            child_index: 0,
            outpoint: None,
        };
        let result2 = decode_tweak_request(&request2);
        assert!(matches!(result2, Err(CcdError::SerializationError(_))));
    }

    #[test]
    fn test_invalid_pubkey_in_request_rejected() {
        let request = TweakRequest {
            version: 1,
            msg_type: "tweak_request".into(),
            tweak: "00".repeat(32),
            derived_pubkey: "00".repeat(33), // all zeros is not a valid point
            child_index: 0,
            outpoint: None,
        };
        let result = decode_tweak_request(&request);
        assert!(matches!(result, Err(CcdError::SerializationError(_))));
    }

    #[test]
    fn test_signing_session_roundtrip() {
        let session = SigningSessionMessage {
            version: 1,
            msg_type: "signing_request".into(),
            psbt: "cHNidP8B...base64...".into(),
            input_tweaks: vec![
                SerializedInputTweak {
                    input_index: 0,
                    tweak: "aa".repeat(32),
                    derived_pubkey: "02".to_string() + &"bb".repeat(32),
                },
                SerializedInputTweak {
                    input_index: 1,
                    tweak: "cc".repeat(32),
                    derived_pubkey: "03".to_string() + &"dd".repeat(32),
                },
            ],
        };

        let json = serde_json::to_string(&session).unwrap();
        let parsed: SigningSessionMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.msg_type, "signing_request");
        assert_eq!(parsed.input_tweaks.len(), 2);
        assert_eq!(parsed.input_tweaks[0].input_index, 0);
        assert_eq!(parsed.input_tweaks[1].input_index, 1);
    }

    #[test]
    fn test_signing_response_roundtrip() {
        let response = SigningResponseMessage {
            version: 1,
            msg_type: "signing_response".into(),
            partial_sigs: vec![SerializedPartialSig {
                input_index: 0,
                signature: "ee".repeat(64),
            }],
            accepted: true,
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: SigningResponseMessage = serde_json::from_str(&json).unwrap();

        assert!(parsed.accepted);
        assert_eq!(parsed.partial_sigs.len(), 1);
        assert_eq!(parsed.partial_sigs[0].signature.len(), 128); // 64 bytes hex
    }

    #[test]
    fn test_signing_response_rejected() {
        let response = SigningResponseMessage {
            version: 1,
            msg_type: "signing_response".into(),
            partial_sigs: vec![],
            accepted: false,
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: SigningResponseMessage = serde_json::from_str(&json).unwrap();

        assert!(!parsed.accepted);
        assert!(parsed.partial_sigs.is_empty());
    }

    #[test]
    fn test_invalid_message_type_rejected() {
        let request = TweakRequest {
            version: 1,
            msg_type: "something_else".into(),
            tweak: "00".repeat(32),
            derived_pubkey: "00".repeat(33),
            child_index: 0,
            outpoint: None,
        };

        let result = decode_tweak_request(&request);
        assert!(matches!(result, Err(CcdError::TransportError(_))));
    }

    // ─── CcdMessage envelope tests ──────────────────────────────────────

    #[test]
    fn test_ccd_message_tweak_request_roundtrip() {
        let (_sk, pk) = test_keypair();
        let delegated = register_cosigner(pk, "test");
        let disclosure = compute_tweak(&delegated, 3).unwrap();
        let tweak_req = encode_tweak_request(&disclosure, None).unwrap();

        let msg = CcdMessage::TweakRequest(tweak_req.clone());
        let json = serialize_message(&msg).unwrap();
        assert!(json.contains("\"ccd_type\":\"tweak_request\""));

        let parsed = deserialize_message(&json).unwrap();
        match parsed {
            CcdMessage::TweakRequest(req) => {
                assert_eq!(req.child_index, 3);
                assert_eq!(req.tweak, tweak_req.tweak);
            }
            _ => panic!("expected TweakRequest"),
        }
    }

    #[test]
    fn test_ccd_message_tweak_ack_roundtrip() {
        let (_sk, pk) = test_keypair();
        let ack = encode_tweak_ack(&pk, true);
        let msg = CcdMessage::TweakAck(ack);
        let json = serialize_message(&msg).unwrap();
        assert!(json.contains("\"ccd_type\":\"tweak_ack\""));

        let parsed = deserialize_message(&json).unwrap();
        match parsed {
            CcdMessage::TweakAck(a) => assert!(a.accepted),
            _ => panic!("expected TweakAck"),
        }
    }

    #[test]
    fn test_ccd_message_nonce_request_roundtrip() {
        let req = blind::NonceRequest {
            session_id: "abc123".into(),
            num_inputs: 2,
            tweaks: vec![blind::SerializedTweak {
                tweak: "aa".repeat(32),
                derived_pubkey: "02".to_string() + &"bb".repeat(32),
                child_index: 0,
            }],
        };
        let msg = CcdMessage::NonceRequest(req);
        let json = serialize_message(&msg).unwrap();
        assert!(json.contains("\"ccd_type\":\"nonce_request\""));
        assert!(json.contains("\"session_id\":\"abc123\""));

        let parsed = deserialize_message(&json).unwrap();
        match parsed {
            CcdMessage::NonceRequest(r) => {
                assert_eq!(r.session_id, "abc123");
                assert_eq!(r.num_inputs, 2);
            }
            _ => panic!("expected NonceRequest"),
        }
    }

    #[test]
    fn test_ccd_message_nonce_response_roundtrip() {
        let resp = blind::NonceResponse {
            session_id: "def456".into(),
            pubnonces: vec!["ff".repeat(66)],
        };
        let msg = CcdMessage::NonceResponse(resp);
        let json = serialize_message(&msg).unwrap();
        assert!(json.contains("\"ccd_type\":\"nonce_response\""));

        let parsed = deserialize_message(&json).unwrap();
        match parsed {
            CcdMessage::NonceResponse(r) => {
                assert_eq!(r.session_id, "def456");
                assert_eq!(r.pubnonces.len(), 1);
            }
            _ => panic!("expected NonceResponse"),
        }
    }

    #[test]
    fn test_ccd_message_sign_challenge_roundtrip() {
        let ch = blind::SignChallenge {
            session_id: "sess789".into(),
            challenges: vec![blind::InputChallenge {
                agg_nonce: "aa".repeat(66),
                sighash: "bb".repeat(32),
            }],
        };
        let msg = CcdMessage::SignChallenge(ch);
        let json = serialize_message(&msg).unwrap();
        assert!(json.contains("\"ccd_type\":\"sign_challenge\""));

        let parsed = deserialize_message(&json).unwrap();
        match parsed {
            CcdMessage::SignChallenge(c) => {
                assert_eq!(c.session_id, "sess789");
                assert_eq!(c.challenges.len(), 1);
                assert_eq!(c.challenges[0].sighash.len(), 64); // 32 bytes hex
            }
            _ => panic!("expected SignChallenge"),
        }
    }

    #[test]
    fn test_ccd_message_partial_signatures_roundtrip() {
        let ps = blind::PartialSignatures {
            session_id: "sigtest".into(),
            partial_sigs: vec!["cc".repeat(32), "dd".repeat(32)],
        };
        let msg = CcdMessage::PartialSignatures(ps);
        let json = serialize_message(&msg).unwrap();
        assert!(json.contains("\"ccd_type\":\"partial_signatures\""));

        let parsed = deserialize_message(&json).unwrap();
        match parsed {
            CcdMessage::PartialSignatures(p) => {
                assert_eq!(p.session_id, "sigtest");
                assert_eq!(p.partial_sigs.len(), 2);
            }
            _ => panic!("expected PartialSignatures"),
        }
    }

    #[test]
    fn test_ccd_message_unknown_type_rejected() {
        let json = r#"{"type": "unknown_type", "data": 42}"#;
        let result = deserialize_message(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_ccd_message_dispatch_all_variants() {
        // Ensure we can match on all 6 variants
        let messages = vec![
            serialize_message(&CcdMessage::TweakRequest(TweakRequest {
                version: 1,
                msg_type: "tweak_request".into(),
                tweak: "00".repeat(32),
                derived_pubkey: "02".to_string() + &"00".repeat(32),
                child_index: 0,
                outpoint: None,
            }))
            .unwrap(),
            serialize_message(&CcdMessage::TweakAck(TweakAck {
                version: 1,
                msg_type: "tweak_ack".into(),
                derived_pubkey: "02".to_string() + &"00".repeat(32),
                accepted: true,
            }))
            .unwrap(),
            serialize_message(&CcdMessage::NonceRequest(blind::NonceRequest {
                session_id: "s1".into(),
                num_inputs: 1,
                tweaks: vec![],
            }))
            .unwrap(),
            serialize_message(&CcdMessage::NonceResponse(blind::NonceResponse {
                session_id: "s1".into(),
                pubnonces: vec![],
            }))
            .unwrap(),
            serialize_message(&CcdMessage::SignChallenge(blind::SignChallenge {
                session_id: "s1".into(),
                challenges: vec![],
            }))
            .unwrap(),
            serialize_message(&CcdMessage::PartialSignatures(blind::PartialSignatures {
                session_id: "s1".into(),
                partial_sigs: vec![],
            }))
            .unwrap(),
        ];

        let types: Vec<&str> = messages
            .iter()
            .map(|json| {
                let msg = deserialize_message(json).unwrap();
                match msg {
                    CcdMessage::TweakRequest(_) => "tweak_request",
                    CcdMessage::TweakAck(_) => "tweak_ack",
                    CcdMessage::NonceRequest(_) => "nonce_request",
                    CcdMessage::NonceResponse(_) => "nonce_response",
                    CcdMessage::SignChallenge(_) => "sign_challenge",
                    CcdMessage::PartialSignatures(_) => "partial_signatures",
                }
            })
            .collect();

        assert_eq!(
            types,
            vec![
                "tweak_request",
                "tweak_ack",
                "nonce_request",
                "nonce_response",
                "sign_challenge",
                "partial_signatures"
            ]
        );
    }

    #[test]
    fn test_prepare_and_decode_dm_content() {
        let req = blind::NonceRequest {
            session_id: "dm_test".into(),
            num_inputs: 3,
            tweaks: vec![],
        };
        let msg = CcdMessage::NonceRequest(req);

        // Prepare for sending (would be NIP-44 encrypted in practice)
        let content = prepare_dm_content(&msg).unwrap();

        // Decode on receiving end (after NIP-44 decryption)
        let decoded = decode_dm_content(&content).unwrap();
        match decoded {
            CcdMessage::NonceRequest(r) => {
                assert_eq!(r.session_id, "dm_test");
                assert_eq!(r.num_inputs, 3);
            }
            _ => panic!("expected NonceRequest"),
        }
    }
}
