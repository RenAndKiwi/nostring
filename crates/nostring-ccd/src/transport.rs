//! Nostr transport layer for CCD tweak exchange.
//!
//! Uses NIP-44 encrypted direct messages to send tweaks between
//! owner and co-signer. No HTTPS server required.

use crate::types::*;
use bitcoin::secp256k1::{PublicKey, Scalar};

/// Encode a TweakDisclosure into a serializable TweakRequest.
pub fn encode_tweak_request(
    disclosure: &TweakDisclosure,
    outpoint: Option<&str>,
) -> Result<TweakRequest, CcdError> {
    Ok(TweakRequest {
        version: 1,
        msg_type: "tweak_request".into(),
        tweak: hex::encode(&disclosure.tweak.to_be_bytes()),
        derived_pubkey: hex::encode(&disclosure.derived_pubkey.serialize()),
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
        derived_pubkey: hex::encode(&derived_pubkey.serialize()),
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

// Private dependency — hex is only in dev-dependencies for tests,
// but transport needs it for encode/decode. Add to main deps.
mod hex {
    /// Encode bytes to hex string.
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Decode hex string to bytes.
    pub fn decode(s: &str) -> Result<Vec<u8>, String> {
        if !s.len().is_multiple_of(2) {
            return Err("odd-length hex string".into());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16)
                    .map_err(|e| format!("invalid hex at position {}: {}", i, e))
            })
            .collect()
    }
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
}
