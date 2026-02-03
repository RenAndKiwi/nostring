#![no_main]

use libfuzzer_sys::fuzz_target;
use nostring_core::crypto::EncryptedSeed;

fuzz_target!(|data: &[u8]| {
    // Try deserializing arbitrary bytes as an EncryptedSeed.
    // EncryptedSeed::from_bytes must never panic — it should always return Ok or Err.
    let _ = EncryptedSeed::from_bytes(data);

    // If deserialization succeeds, verify round-trip serialization doesn't panic
    if let Ok(seed) = EncryptedSeed::from_bytes(data) {
        let bytes = seed.to_bytes();
        // Re-deserialize the serialized bytes — this should also never panic
        let _ = EncryptedSeed::from_bytes(&bytes);
    }
});
