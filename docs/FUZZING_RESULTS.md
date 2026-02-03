# Fuzzing Results

## Overview

Cargo-fuzz (libFuzzer) infrastructure was set up for security-critical input parsing code in NoString. Three fuzz targets were created covering the primary attack surface for user-provided input.

## Setup

- **Tool:** `cargo-fuzz` 0.13.1 with `libfuzzer-sys` 0.4
- **Toolchain:** Rust nightly (required by cargo-fuzz)
- **Location:** `fuzz/` directory at workspace root

## Fuzz Targets

### 1. `codex32_parse` — Codex32 Share Parsing

- **File:** `fuzz/fuzz_targets/codex32_parse.rs`
- **Function under test:** `nostring_shamir::codex32::parse_share`
- **Strategy:** Feeds arbitrary bytes as UTF-8 strings, also prepends `"ms1"` prefix to exercise deeper parsing paths (checksum verification, bech32 decoding, threshold/identifier extraction)
- **Result:** ✅ **No crashes**
  - 10,515,509 runs in 31 seconds (~339K exec/s)
  - Coverage: 164 edges, 290 features
  - Corpus: 32 inputs

### 2. `mnemonic_parse` — BIP-39 Mnemonic Parsing

- **File:** `fuzz/fuzz_targets/mnemonic_parse.rs`
- **Function under test:** `nostring_core::seed::parse_mnemonic`
- **Strategy:** Feeds arbitrary bytes as UTF-8 strings to the BIP-39 mnemonic parser (word lookup, checksum validation)
- **Result:** ✅ **No crashes**
  - 1,593,538 runs in 31 seconds
  - The `bip39` crate's parser correctly returns `Err` for all malformed inputs

### 3. `encrypted_seed_parse` — Encrypted Seed Deserialization

- **File:** `fuzz/fuzz_targets/encrypted_seed_parse.rs`
- **Function under test:** `nostring_core::crypto::EncryptedSeed::from_bytes`
- **Strategy:** Feeds arbitrary bytes to deserialization; on success, verifies round-trip (serialize → deserialize) doesn't panic
- **Result:** ✅ **No crashes**
  - 1,789,460 runs in 31 seconds (~57K exec/s)
  - Coverage: 72 edges, 99 features
  - Corpus: 13 inputs

## Summary

| Target | Runs | Duration | Crashes |
|--------|------|----------|---------|
| `codex32_parse` | 10,515,509 | 31s | 0 |
| `mnemonic_parse` | 1,593,538 | 31s | 0 |
| `encrypted_seed_parse` | 1,789,460 | 31s | 0 |

**No crashes or panics were found.** All parsing functions correctly return `Result`/`Option` types for malformed inputs rather than panicking.

## Running the Fuzzers

```bash
# Install cargo-fuzz (one-time)
cargo install cargo-fuzz

# Run a specific target (30 seconds)
cargo +nightly fuzz run codex32_parse -- -max_total_time=30

# Run indefinitely (Ctrl+C to stop)
cargo +nightly fuzz run codex32_parse

# List all targets
cargo +nightly fuzz list
```

## Future Work

- Add PSBT parsing fuzz target if/when base64 → PSBT parsing is implemented
- Run fuzzers for longer durations (hours/overnight) for deeper coverage
- Add structure-aware fuzzing with `arbitrary` derive for typed inputs
- Consider adding `afl` as an alternative fuzzing engine
- Set up CI to run fuzzers on PRs (e.g., OSS-Fuzz integration)
