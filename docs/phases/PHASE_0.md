# Phase 0: Foundation

**Goal:** Working Rust workspace with all crates building, upstream analysis complete.

**Started:** 2026-02-01 22:50 CST

---

## Feature 0.1: Dev Environment Setup

### Research & Plan

**Objective:** Establish a reproducible development environment.

**Requirements:**
- Rust stable toolchain
- System dependencies for crypto crates (secp256k1)
- System dependencies for Tauri (webkit, etc.)
- Verification that everything builds

**Approach:**
1. Document required tools
2. Create setup script
3. Verify builds work
4. Document any platform-specific notes

### Security Review

Low security impact — this is dev environment setup. However:
- Ensure we're using official Rust toolchain (rustup)
- Pin dependency versions in Cargo.lock once stable
- No secrets in dev setup

### Implementation

*See below for work log*

### Testing

- [ ] `cargo build` succeeds for all crates
- [ ] No warnings in build output
- [ ] Tests run (even if few exist yet)

### Reflection

*To be filled after completion*

---

## Feature 0.2: Workspace Configuration

### Research & Plan

**Objective:** Cargo workspace properly configured with shared dependencies.

**Current State:**
- Workspace Cargo.toml exists
- Individual crate Cargo.toml files exist
- Dependencies declared but not verified

**Approach:**
1. Verify all dependencies resolve
2. Fix version conflicts
3. Ensure workspace inheritance works
4. Add missing build files (build.rs for Tauri)

### Security Review

- Use exact versions for crypto dependencies (no wildcards)
- Audit critical dependencies (bip39, bitcoin, secp256k1, nostr-sdk)
- Document dependency tree

### Implementation

*See below for work log*

### Testing

- [ ] `cargo check --workspace` passes
- [ ] `cargo build --workspace` passes
- [ ] Dependencies resolve without conflicts

### Reflection

*To be filled after completion*

---

## Feature 0.3: nostr-mail Analysis

### Research & Plan

**Objective:** Understand nostr-mail codebase, identify what to port.

**Questions to Answer:**
1. What does nostr-mail actually implement?
2. Which modules are core vs. UI-specific?
3. What's the encryption flow?
4. How does it handle SMTP/IMAP?
5. What can we reuse vs. rewrite?

**Approach:**
1. Build nostr-mail from source
2. Read through each module
3. Document the architecture
4. Identify core functionality to port

### Security Review

- How does nostr-mail handle private keys?
- Where is encryption/decryption performed?
- Are there any known vulnerabilities?
- What's the key storage mechanism?

### Implementation

*Analysis document to be created*

### Reflection

*To be filled after completion*

---

## Feature 0.4: Liana Core Analysis

### Research & Plan

**Objective:** Understand Liana's miniscript/descriptor implementation.

**Questions to Answer:**
1. How does Liana construct policies?
2. What miniscript features does it use?
3. How does the timelock mechanism work?
4. What's the UTXO management approach?
5. How does it handle multiple recovery paths?

**Approach:**
1. Clone Liana repo
2. Study the descriptor module
3. Document policy construction
4. Identify integration points

### Security Review

- How does Liana protect keys?
- Timelock verification methodology
- Recovery path security

### Implementation

*Analysis document to be created*

### Reflection

*To be filled after completion*

---

## Work Log

### 2026-02-01 22:50 CST — Starting Phase 0

Beginning with Feature 0.1: Dev Environment Setup.

### 2026-02-02 00:00 CST — Feature 0.1 Complete

**Dev Environment Setup - DONE**

Installed:
- Rust 1.93.0 (stable-aarch64-apple-darwin)
- Cargo 1.93.0

System already had ImageMagick for generating placeholder icons.

### 2026-02-02 00:10 CST — Feature 0.2 Complete  

**Workspace Configuration - DONE**

Issues encountered and resolved:
1. `imap` crate is still in alpha (3.0.0-alpha.15) — had to specify exact version
2. `bip39` crate needed `rand` feature for `generate_in()` function
3. Tauri needed `tauri.conf.json` and `build.rs`
4. Tauri icons must be RGBA PNGs — created placeholder navy blue icons

Final dependency versions locked:
- bip39 2.2.2 (with rand feature)
- bitcoin 0.32.8
- secp256k1 0.29.1
- nostr-sdk 0.39.0
- miniscript 12.3.5
- tauri 2.9.5
- imap 3.0.0-alpha.15
- lettre 0.11.19

**Test Results:**
```
running 1 test
test keys::tests::test_key_derivation ... ok
```

The key derivation test passes — we can generate a mnemonic, derive the seed, and produce both Nostr keys and Bitcoin keys from the same seed.

### Reflections on 0.1 and 0.2

**What worked well:**
- Rust's error messages are excellent — they pointed directly to the API changes
- The modular crate structure made it easy to isolate issues
- Starting with `cargo check` before full build saved time

**What I learned:**
- Always check if features need to be enabled for dependencies (bip39 rand)
- Alpha versions need exact version specs
- Tauri has specific requirements for icons (RGBA PNGs)

**What could be better:**
- Should have researched bip39 2.x API changes before writing the code
- Could have used `cargo add` to get latest compatible versions automatically

**Code quality:**
- The seed.rs and keys.rs code is clean and minimal
- Test proves the core derivation works
- No shortcuts taken — the test actually generates and derives keys
