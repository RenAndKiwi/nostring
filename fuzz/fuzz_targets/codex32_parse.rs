#![no_main]

use libfuzzer_sys::fuzz_target;
use nostring_shamir::codex32::parse_share;

fuzz_target!(|data: &[u8]| {
    // Try parsing arbitrary bytes as a UTF-8 string, then as a codex32 share.
    // parse_share must never panic — it should always return Ok or Err.
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = parse_share(s);
    }

    // Also try with the "ms1" prefix prepended to exercise deeper parsing paths
    if let Ok(s) = std::str::from_utf8(data) {
        let prefixed = format!("ms1{}", s);
        let _ = parse_share(&prefixed);
    }
});
