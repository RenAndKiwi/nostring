#![no_main]

use libfuzzer_sys::fuzz_target;
use nostring_core::seed::parse_mnemonic;

fuzz_target!(|data: &[u8]| {
    // Try parsing arbitrary bytes as a UTF-8 string, then as a BIP-39 mnemonic.
    // parse_mnemonic must never panic — it should always return Ok or Err.
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = parse_mnemonic(s);
    }
});
