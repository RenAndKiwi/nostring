# Phase 10: External Security Audit Preparation

**Goal:** Prepare the NoString codebase, documentation, and processes for a professional third-party security audit. Identify and close gaps that would waste auditor time or produce avoidable findings.

**Status:** Planning  
**Created:** 2026-02-03  
**Prerequisites:** Phases 1–9 (core functionality complete, internal security review done)

---

## Table of Contents

1. [Why an External Audit Matters](#1-why-an-external-audit-matters)
2. [Current Security Posture](#2-current-security-posture)
3. [Audit Scope Definition](#3-audit-scope-definition)
4. [Highest-Risk Code Paths](#4-highest-risk-code-paths)
5. [Self-Audit Checklist](#5-self-audit-checklist)
6. [Identified Gaps & Pre-Audit Hardening](#6-identified-gaps--pre-audit-hardening)
7. [Audit Firm Selection](#7-audit-firm-selection)
8. [Cost & Timeline Estimates](#8-cost--timeline-estimates)
9. [Deliverables for Auditors](#9-deliverables-for-auditors)
10. [Post-Audit Process](#10-post-audit-process)
11. [Roadmap](#11-roadmap)

---

## 1. Why an External Audit Matters

NoString handles **Bitcoin inheritance** — if the cryptography is wrong, people lose their savings. If the miniscript policy has an extra spending path, heirs or attackers can steal funds. If the seed encryption is weak, a stolen laptop means total loss.

Internal review found zero `unsafe` blocks, zero `unwrap()` on untrusted input, and 31+ security-specific tests. That's good hygiene. But self-review has blind spots:

- **Author bias** — The person who wrote the code can't see their own assumptions
- **Missing threat scenarios** — Professional auditors bring experience from hundreds of engagements
- **Cryptographic subtleties** — Correct-looking crypto can have timing side-channels, nonce reuse risks, or parameter weaknesses that only specialists catch
- **Trust signal** — For a Bitcoin tool handling real funds, "we audited ourselves" carries zero credibility

An external audit is not optional for production use with real Bitcoin. It's a prerequisite.

---

## 2. Current Security Posture

### What's Done ✅

| Area | Status | Evidence |
|------|--------|----------|
| Threat model | Documented | `docs/THREAT_MODEL.md` — 6 attack scenarios, 4 threat actors |
| Security model | Documented | `docs/SECURITY.md` — defense-in-depth architecture |
| Internal audit prep | Documented | `docs/SECURITY_AUDIT.md` — scope, review points, findings |
| Zero `unsafe` blocks | Verified | Full codebase grep |
| Zero `unwrap()` on user input | Verified | 167 `unwrap()` calls audited, all in safe contexts |
| No secret logging | Verified | Manual review of all log/print paths |
| Dependencies pinned | Verified | `Cargo.lock` committed |
| Test coverage | 115+ tests | BIP-39 vectors, NIP-06 vectors, Codex32 vectors, policy compilation, PSBT generation |
| Code review | Complete | Pre-audit review documented in SECURITY_AUDIT.md |

### What's Not Done ❌

| Gap | Severity | Notes |
|-----|----------|-------|
| `zeroize` not applied to seed data | **HIGH** | Workspace dependency declared but never used in any crate |
| Core dump protection | MEDIUM | No `prctl`/`setrlimit` to prevent core dumps |
| `mlock` for seed pages | MEDIUM | Seed can be swapped to disk |
| PSBT inputs incomplete | MEDIUM | `witness_utxo` and `witness_script` not populated |
| Fuzzing targets | MEDIUM | No fuzzing infrastructure exists |
| Reproducible builds | LOW | Not implemented |
| Signed releases | LOW | Not implemented |
| Multi-server Electrum consensus | LOW | Single server, no cross-validation |
| Password entropy enforcement | LOW | Empty password accepted |

---

## 3. Audit Scope Definition

### Tier 1: Critical (Must Audit)

These components handle private keys, Bitcoin transactions, or secret sharing — bugs here cause fund loss.

| Component | Location | Lines | Why Critical |
|-----------|----------|-------|-------------|
| Seed encryption/decryption | `nostring-core/src/crypto.rs` | 276 | Protects the master seed at rest |
| Key derivation (BIP-32/84, NIP-06) | `nostring-core/src/keys.rs` | 289 | Wrong path = wrong keys = lost funds |
| BIP-39 seed management | `nostring-core/src/seed.rs` | ~170 | Mnemonic → seed conversion |
| Miniscript policy construction | `nostring-inherit/src/policy.rs` | 606 | Extra spending path = theft |
| PSBT/check-in builder | `nostring-inherit/src/checkin.rs` | 462 | Wrong outputs = sent to wrong address |
| GF(256) arithmetic | `nostring-shamir/src/gf256.rs` | ~200 | Wrong math = unrecoverable shares |
| Shamir split/reconstruct | `nostring-shamir/src/shamir.rs` | ~200 | Threshold bypass = unauthorized reconstruction |
| SLIP-39 encoding | `nostring-shamir/src/slip39.rs` | 420 | Checksum failure = corrupted shares |
| RS1024 checksums | `nostring-shamir/src/rs1024.rs` | ~200 | Bad checksum = silent data corruption |
| Codex32 (BIP-93) | `nostring-shamir/src/codex32.rs` | 709 | Same as above |

**Estimated Tier 1 scope: ~3,500 lines of Rust**

### Tier 2: High (Should Audit)

| Component | Location | Lines | Why Important |
|-----------|----------|-------|--------------|
| Electrum client | `nostring-electrum/src/lib.rs` | 323 | Parses untrusted network data |
| Heir management | `nostring-inherit/src/heir.rs` | 256 | Manages heir xpubs |
| Watch service | `nostring-watch/src/lib.rs` | 613 | Monitors timelock status |
| Spend analysis | `nostring-watch/src/spend_analysis.rs` | 473 | Detects unauthorized spending |

**Estimated Tier 2 scope: ~1,700 lines of Rust**

### Tier 3: Lower Priority (Optional)

| Component | Location | Notes |
|-----------|----------|-------|
| Nostr DM relay | `nostring-notify/` | Notification only, no funds at risk |
| SMTP notifications | `nostring-notify/src/smtp.rs` | Availability, not integrity |
| Server daemon | `nostring-server/` | Configuration management |
| Tauri frontend | `src-tauri/` | UI layer, no direct crypto |

### Out of Scope

- Third-party crate internals (`bitcoin`, `miniscript`, `bip39`, `aes-gcm`, `argon2`, `secp256k1`) — these have their own audits
- Tauri framework itself
- Frontend JavaScript/TypeScript
- Operating system security

---

## 4. Highest-Risk Code Paths

An auditor will (and should) focus time proportional to risk. Here are the paths ranked by potential impact:

### 🔴 Risk Level: Critical

**4.1 Seed Encryption Cycle** (`crypto.rs`)
- `encrypt_seed()` → Argon2id KDF → AES-256-GCM encrypt
- `decrypt_seed()` → Argon2id KDF → AES-256-GCM decrypt
- **What could go wrong:** Nonce reuse (catastrophic for GCM), weak KDF parameters, salt truncation (current code slices SaltString to 16 bytes — is the encoding safe?), missing authenticated data, plaintext left in memory after encryption
- **Specific concern:** The `salt` is derived by generating a `SaltString`, converting to `&str`, taking bytes, and slicing to 16. This discards the rest of the salt string. An auditor should verify this doesn't reduce entropy.

**4.2 Miniscript Policy Construction** (`policy.rs`)
- `to_concrete_policy()` → builds `Concrete<DescriptorPublicKey>` tree
- `to_wsh_descriptor()` → compiles to P2WSH
- **What could go wrong:** Policy that compiles but has unintended spending paths, timelock off-by-one allowing early heir spend, missing key in multi-sig threshold, `Or` weighting creating bias
- **Specific concern:** The recursive `Or` construction for cascade policies (multiple recovery paths) — verify no path is accidentally omittable or duplicable.

**4.3 Shamir Secret Sharing** (`gf256.rs`, `shamir.rs`)
- `split_secret()` → polynomial evaluation over GF(256)
- `reconstruct_secret()` → Lagrange interpolation
- **What could go wrong:** Off-by-one in share indices, incorrect polynomial evaluation, Lagrange denominator = 0 (division by zero panics with `assert!`), insufficient randomness in coefficients
- **Specific concern:** `gf_div` uses `assert!(b != 0)` which panics rather than returning an error. In production, a malformed share with x=0 could crash the app.

### 🟡 Risk Level: High

**4.4 PSBT Construction** (`checkin.rs`)
- `build_unsigned_tx()` → fee calculation, change output, sequence numbers
- `build_psbt()` → unsigned PSBT for hardware wallet signing
- **What could go wrong:** Fee underestimation (tx rejected), fee overestimation (overpayment), change sent to wrong address, `witness_utxo` not populated (hardware wallet can't validate), RBF sequence misconfiguration
- **Specific concern:** `witness_utxo` and `witness_script` — ✅ now populated (see Gap 2 resolution below). BIP-32 derivation paths also added.

**4.5 SLIP-39 Bit Packing** (`slip39.rs`)
- `encode_share_to_words()` → bit-level serialization
- `parse_mnemonic()` → bit-level deserialization
- **What could go wrong:** Off-by-one in bit offsets, padding not handled correctly, header field parsing extracts wrong bits, checksum covers wrong data
- **Specific concern:** In `parse_mnemonic()`, the extendable flag bit (bit 15) seems skipped in parsing but is set during encoding. Verify round-trip correctness for all header fields.

### 🟢 Risk Level: Medium

**4.6 Electrum Response Parsing** (`lib.rs`)
- All data from Electrum servers is untrusted
- **What could go wrong:** Malicious server returns negative heights (cast to u32), oversized responses, false UTXOs
- **Specific concern:** `get_balance()` casts `unconfirmed` (which can be negative) using `.max(0) as u64` — verify this handles all edge cases correctly.

**4.7 Key Derivation Path Correctness** (`keys.rs`)
- BIP-84 path for mainnet vs testnet
- NIP-06 path for Nostr
- **What could go wrong:** Wrong coin type, wrong purpose, non-hardened derivation where hardened is required
- **Note:** Test vectors exist and pass — this is well-covered but an auditor should verify the vectors themselves are correct.

---

## 5. Self-Audit Checklist

This checklist mirrors what a professional auditor would check. Complete every item before engaging an audit firm to minimize billable hours spent on known issues.

### 5.1 Cryptographic Implementation

- [ ] **Nonce uniqueness:** Verify `Aes256Gcm::generate_nonce(&mut OsRng)` is called fresh for every encryption. Confirm no nonce reuse is possible.
- [ ] **Salt entropy:** Verify the 16-byte slice from `SaltString::generate()` retains sufficient entropy (SaltString is base64-encoded; slicing raw bytes may include only alphanumeric chars).
- [ ] **KDF parameters:** Confirm Argon2id params (m=64MB, t=3, p=4) meet OWASP 2024+ recommendations.
- [ ] **Constant-time comparison:** Verify AES-GCM tag verification is constant-time (handled by `aes-gcm` crate, but confirm version).
- [ ] **No ECB mode:** Confirm GCM mode is correctly specified (not ECB or CBC).
- [ ] **Key material in memory:** Verify derived AES key is not retained after encrypt/decrypt completes.
- [ ] **PBKDF2 in BIP-39:** Confirm `mnemonic.to_seed()` uses 2048 iterations of HMAC-SHA512 per spec.

### 5.2 Memory Safety

- [ ] **Zeroize seed after use:** Add `Zeroize` derive to seed arrays, use `Zeroizing<>` wrapper for temporary key material.
- [ ] **Zeroize derived keys:** The `key` array in `derive_key()` and the `plaintext` in `decrypt_seed()` must be zeroed.
- [ ] **No core dumps:** Call `setrlimit(RLIMIT_CORE, 0)` at app startup.
- [ ] **mlock sensitive pages:** Use `libc::mlock()` on seed buffer to prevent swap.
- [ ] **No seed in Debug output:** Ensure no `Debug` trait on types containing key material prints actual bytes.
- [ ] **Stack vs heap:** Verify seed arrays are stack-allocated (they are `[u8; 64]`, good) and not accidentally boxed.

### 5.3 Bitcoin Script & Miniscript

- [ ] **Policy round-trip:** Compile policy to descriptor, derive addresses, verify against independent tool (e.g., `bdk`, `bitcoin-cli`).
- [ ] **Timelock boundary:** Test timelock at exactly `blocks - 1` (not spendable) and `blocks` (spendable).
- [ ] **No extra spending paths:** Decompile the compiled miniscript back to spending conditions and verify only the intended paths exist.
- [ ] **Wildcard derivation:** Verify `<0;1>/*` in descriptor keys works correctly for both receive and change paths.
- [ ] **Duplicate key detection:** Confirm `InheritancePolicy::new()` rejects duplicate keys across all paths.
- [ ] **Cascade ordering:** Verify cascade timelocks are enforced in ascending order.

### 5.4 PSBT & Transaction

- [x] **Witness UTXO populated:** ~~Fix TODO~~ — `psbt.inputs[0].witness_utxo` populated with correct TxOut (amount + P2WSH script_pubkey). Tested.
- [x] **Witness script populated:** ~~Fix TODO~~ — `psbt.inputs[0].witness_script` populated via descriptor derivation. Hash-verified against script_pubkey in tests.
- [ ] **Fee sanity check:** Verify fee estimation accounts for worst-case witness size.
- [ ] **Change address validation:** Confirm change goes back to the same policy address (same descriptor).
- [ ] **RBF signaling:** Confirm `Sequence::ENABLE_RBF_NO_LOCKTIME` is appropriate for check-in transactions.
- [ ] **Dust output prevention:** Add check that change amount exceeds dust limit.

### 5.5 Shamir / Secret Sharing

- [ ] **GF(256) tables:** Verify LOG and EXP tables against a reference implementation or generate programmatically and compare.
- [ ] **Lagrange at x=0 only:** Confirm interpolation target is always x=0 (the secret).
- [ ] **Share index range:** Verify share indices are 1..=255 (x=0 is the secret, must never be a share point).
- [ ] **Threshold enforcement:** Confirm `t-1` shares cannot reconstruct (even partially).
- [ ] **Random coefficients:** Verify polynomial coefficients (except constant term = secret) are generated from CSPRNG.
- [ ] **RS1024 checksum:** Verify against SLIP-39 test vectors.
- [ ] **Codex32 BCH checksum:** Verify against BIP-93 test vectors.

### 5.6 Network Security

- [ ] **TLS enforcement:** Verify mainnet connections reject non-SSL URLs.
- [ ] **Certificate validation:** Confirm `electrum_client` crate validates certificates by default.
- [ ] **Response bounds:** Check that Electrum responses with unreasonable values (negative amounts, enormous heights) are rejected.
- [ ] **No key transmission:** Grep entire codebase for any path where private key bytes could reach a network call.

### 5.7 Input Validation

- [ ] **Mnemonic validation:** Confirm invalid checksums are rejected.
- [ ] **Timelock range:** Confirm 0 and >65535 are rejected.
- [ ] **Xpub parsing:** Confirm malformed xpubs produce errors, not panics.
- [ ] **PSBT parsing:** Confirm invalid PSBTs produce errors, not panics.
- [ ] **File path traversal:** If any file paths come from user input, confirm no `../` traversal.

### 5.8 Dependencies

- [ ] **`cargo audit`:** Run and fix all advisories.
- [ ] **`cargo deny`:** Check for duplicate versions, banned licenses, known vulnerabilities.
- [ ] **Dependency count:** Document total dependency tree size (smaller = less attack surface).
- [ ] **Pin versions:** Confirm all workspace dependencies use exact versions or `Cargo.lock`.

### 5.9 Build & Release

- [ ] **Reproducible builds:** Set up and verify deterministic builds.
- [ ] **Signed releases:** GPG-sign release binaries.
- [ ] **CI security checks:** Add `cargo audit`, `cargo clippy`, `cargo deny` to CI.

---

## 6. Identified Gaps & Pre-Audit Hardening

These are concrete issues to fix **before** engaging auditors. Fixing known issues pre-audit avoids paying $300+/hour for findings you already know about.

### Gap 1: Memory Zeroing (HIGH Priority)

**Problem:** The `zeroize` crate is declared in `Cargo.toml` workspace dependencies but never actually used in any crate. Seed bytes, derived keys, and mnemonic strings remain in memory after use.

**Fix:**
1. Add `zeroize` dependency to `nostring-core/Cargo.toml`
2. Wrap `[u8; 64]` seed returns in `Zeroizing<[u8; 64]>`
3. Derive `Zeroize` on `EncryptedSeed` (though less critical since it's ciphertext)
4. Zero the `key` array in `derive_key()` before return (or use `Zeroizing`)
5. Zero `plaintext` in `decrypt_seed()` after copying to output array

**Estimated effort:** 1–2 hours

### Gap 2: PSBT Input Data — ✅ RESOLVED

**Problem:** `build_psbt()` previously had TODO for `witness_utxo` and `witness_script`.

**Resolution:** Fully implemented in `checkin.rs`:
1. ✅ `psbt.inputs[0].witness_utxo` populated with correct `TxOut` (amount + script_pubkey)
2. ✅ `psbt.inputs[0].witness_script` populated via descriptor derivation with multi-path support
3. ⬜ BIP-174 derivation paths in PSBT inputs/outputs (future enhancement, not critical)

**Test:** `test_checkin_psbt_generation` verifies witness_utxo amount, P2WSH type, witness_script non-empty, and hash consistency.

### Gap 3: Panic in GF(256) Division (MEDIUM Priority)

**Problem:** `gf_div()` and `gf_inv()` use `assert!(b != 0)` which panics. If a malformed share has x=0, the entire application crashes.

**Fix:** Return `Result<u8, ShamirError>` instead of panicking, or validate share indices before calling GF operations.

**Estimated effort:** 1–2 hours

### Gap 4: Salt Entropy Question (MEDIUM Priority)

**Problem:** In `encrypt_seed()`, the salt is generated via `SaltString::generate()` which produces a base64-encoded string. The code then takes `.as_str().as_bytes()` and slices to 16 bytes. Since SaltString uses base64 characters (A-Z, a-z, 0-9, +, /), each byte only has ~6 bits of entropy instead of 8. This means the 16-byte salt has ~96 bits of entropy instead of 128.

**Fix:** Use `OsRng.fill_bytes()` directly to generate a 16-byte random salt, bypassing the `SaltString` API.

**Estimated effort:** 30 minutes

### Gap 5: Empty Password Accepted (LOW Priority)

**Problem:** `encrypt_seed("", seed)` works. While the test documents this as intentional, for a Bitcoin inheritance tool, an empty password should at minimum produce a warning.

**Fix:** Add an optional password strength check with configurable enforcement level.

**Estimated effort:** 1–2 hours

### Gap 6: Fuzzing Infrastructure (MEDIUM Priority)

**Problem:** No fuzz targets exist. Auditors will likely ask for them, and fuzzing finds bugs that unit tests miss.

**Fix:** Create fuzz targets for:
- `EncryptedSeed::from_bytes()` — malformed ciphertext handling
- `parse_mnemonic()` (BIP-39) — arbitrary word strings
- `parse_mnemonic()` (SLIP-39) — arbitrary word vectors
- `codex32::decode()` — arbitrary strings
- Electrum response parsing — malformed JSON

**Estimated effort:** 4–6 hours (using `cargo-fuzz`)

### Gap 7: Core Dump & mlock (LOW-MEDIUM Priority)

**Problem:** Neither core dumps are disabled nor are sensitive memory pages locked.

**Fix:**
1. At app startup: `setrlimit(RLIMIT_CORE, &rlimit { rlim_cur: 0, rlim_max: 0 })`
2. When seed is decrypted: `mlock(seed_ptr, 64)` and `munlock` after use
3. Use `secrecy` crate as an alternative for ergonomic secret handling

**Estimated effort:** 2–3 hours

### Gap 8: Reproducible Builds (LOW Priority)

**Problem:** No reproducible build process documented or verified.

**Fix:** Pin Rust toolchain version, use `SOURCE_DATE_EPOCH`, verify deterministic output across builds.

**Estimated effort:** 4–8 hours

---

## 7. Audit Firm Selection

### Recommended Firms (Bitcoin/Rust Expertise)

| Firm | Specialization | Estimated Cost | Notes |
|------|---------------|----------------|-------|
| **Trail of Bits** | General security, crypto, Rust | $80K–$150K | Top-tier, very thorough, long waitlist |
| **NCC Group** | Crypto, applied security | $60K–$120K | Audited Bitcoin Core, Zcash |
| **Cure53** | Web/desktop apps, crypto | $30K–$80K | Excellent for Tauri/Electron-style apps |
| **Least Authority** | Bitcoin, privacy tech | $40K–$80K | Audited Zcash, Tor, Lightning |
| **Coinspect** | Bitcoin-focused | $25K–$60K | Specialize in Bitcoin wallets/protocols |
| **Quarkslab** | Low-level, crypto, Rust | $40K–$90K | Strong in Rust and applied crypto |
| **Independent Researchers** | Varies | $10K–$30K | Can be excellent for focused scope |

### Selection Criteria

1. **Bitcoin protocol expertise** — Must understand miniscript, PSBT, BIP-32/39/84, timelocks
2. **Rust experience** — Must have audited Rust codebases (not just Solidity)
3. **Cryptographic review capability** — Must assess Argon2id/AES-GCM implementation correctness
4. **Published reports** — Prefer firms that publish audit reports (transparency signal)
5. **Availability** — Top firms have 3-6 month waitlists

### Anti-Patterns to Avoid

- **Smart-contract-only firms** — They specialize in Solidity/EVM, not Bitcoin Script or desktop apps
- **Automated-only audits** — Tools find surface bugs but miss logic errors in miniscript policies
- **Firms without Bitcoin experience** — Miniscript, PSBT, and timelock semantics require domain knowledge

---

## 8. Cost & Timeline Estimates

### Scope-Based Estimate

| Tier | Lines of Code | Auditor-Weeks | Estimated Cost |
|------|--------------|---------------|----------------|
| Tier 1 (Critical crypto + Bitcoin) | ~3,500 | 3–4 weeks | $30K–$60K |
| Tier 1 + Tier 2 (Full backend) | ~5,200 | 4–6 weeks | $45K–$90K |
| Full audit (all tiers) | ~8,800 | 6–8 weeks | $60K–$120K |

### Recommended Approach: Staged Audit

**Stage 1: Critical Path Only** ($30K–$60K, 3–4 weeks)
- Seed encryption/decryption
- Key derivation paths
- Miniscript policy construction
- Shamir secret sharing (GF256 + SLIP-39 + Codex32)
- PSBT building

**Stage 2: Network & State** ($15K–$30K, 2–3 weeks)
- Electrum client (untrusted input handling)
- Watch service (timelock tracking, spend detection)
- Heir management

**Stage 3: Full Application** ($10K–$20K, 1–2 weeks)
- Notification system
- Server daemon
- Tauri integration

**Total staged estimate: $55K–$110K over 6–9 weeks**

### Budget Reality Check

If budget is limited:
- **$10K–$20K:** Hire an independent Bitcoin security researcher for a focused review of Tier 1 only
- **$30K–$50K:** Mid-tier firm, Tier 1 + critical Tier 2 components
- **$50K–$100K+:** Top-tier firm, comprehensive audit with remediation cycles

### Timeline

```
Month 1:     Pre-audit hardening (Gaps 1-8)
Month 2:     Audit firm selection & scoping call
Month 3-4:   Audit engagement (may be later due to waitlists)
Month 4-5:   Receive findings, remediation sprint
Month 5:     Re-audit / verification round
Month 6:     Publish audit report, tag audited release
```

---

## 9. Deliverables for Auditors

Prepare this package before the engagement begins:

### Documentation Package

- [ ] **Architecture overview** — Crate dependency graph, data flow diagrams
- [ ] **THREAT_MODEL.md** — Already exists ✅
- [ ] **SECURITY.md** — Already exists ✅
- [ ] **SECURITY_AUDIT.md** — Already exists ✅
- [ ] **Cryptographic specifications** — Which algorithms, parameters, standards
- [ ] **Test vector sources** — Links to BIP-39, BIP-84, BIP-93, NIP-06, SLIP-39 specs
- [ ] **Known limitations** — Things the auditors don't need to find (save their time)

### Code Package

- [ ] **Tagged git commit** — Specific commit hash for the audit scope
- [ ] **Build instructions** — Verified on clean machine
- [ ] **Test suite** — All tests passing, including ignored integration tests
- [ ] **Dependency manifest** — `cargo tree` output, `cargo audit` report
- [ ] **Fuzz targets** — Ready-to-run with `cargo fuzz`

### Context Package

- [ ] **Comparable projects** — Links to Liana wallet (closest peer project), their architecture decisions
- [ ] **Standards referenced** — BIP-32, BIP-39, BIP-84, BIP-93, BIP-174, NIP-06, SLIP-39
- [ ] **Threat actor profiles** — From THREAT_MODEL.md
- [ ] **Specific concerns** — The salt entropy question, cascade policy correctness, GF(256) table verification

---

## 10. Post-Audit Process

### On Receiving the Report

1. **Triage findings** by severity (Critical → Informational)
2. **Fix all Critical and High findings** before any release
3. **Document accepted risks** for Medium/Low findings with rationale
4. **Request re-audit** of fixed code (typically included or discounted)

### Publishing

1. **Publish the full audit report** — Redact nothing unless it would enable active exploitation
2. **Tag the audited commit** — `git tag -s v1.0.0-audited`
3. **Document what changed** post-audit in CHANGELOG
4. **Set up vulnerability disclosure** — security@nostring.dev, responsible disclosure policy

### Ongoing

1. **Re-audit on major changes** — New cryptographic code, new spending paths, new dependency
2. **Bug bounty program** — Even a small one ($500–$5000 per vuln) attracts ongoing scrutiny
3. **Dependency monitoring** — `cargo audit` in CI, `dependabot`/`renovate` for updates

---

## 11. Roadmap

### Pre-Audit Sprint (2–4 weeks)

| Task | Priority | Effort | Assignee |
|------|----------|--------|----------|
| ~~Implement `zeroize` for seed handling~~ | ~~HIGH~~ | ~~2h~~ | ✅ Done |
| ~~Fix PSBT `witness_utxo`/`witness_script` TODO~~ | ~~HIGH~~ | ~~4h~~ | ✅ Done |
| ~~Replace `assert!` with `Result` in GF(256)~~ | ~~MEDIUM~~ | ~~2h~~ | ✅ Done |
| ~~Fix salt generation (use OsRng directly)~~ | ~~MEDIUM~~ | ~~30min~~ | ✅ Done |
| ~~Create fuzz targets (5 targets)~~ | ~~MEDIUM~~ | ~~6h~~ | ✅ 3 targets |
| Add core dump protection | MEDIUM | 1h | — |
| Add mlock for seed pages | MEDIUM | 2h | — |
| Run `cargo audit` + `cargo deny`, fix issues | MEDIUM | 2h | — |
| Add password entropy warning | LOW | 2h | — |
| Document reproducible build process | LOW | 8h | — |
| Prepare auditor documentation package | HIGH | 4h | — |
| Run full self-audit checklist (Section 5) | HIGH | 8h | — |

**Total pre-audit effort: ~40 hours**

### Audit Engagement (4–8 weeks)

1. Contact 3–5 firms for quotes
2. Provide scope document + code access
3. Scoping call to agree on focus areas
4. Engagement begins
5. Mid-audit check-in (Q&A with auditors)
6. Receive draft report
7. Remediation sprint
8. Final report + re-audit

### Post-Audit Release

1. Fix all findings
2. Publish audit report
3. Tag `v1.0.0` (first production release)
4. Set up bug bounty program

---

## Appendix A: Codebase Statistics

```
Total Rust lines:     ~8,800 (application code)
Crate count:          7 (core, inherit, shamir, electrum, notify, watch, email)
Test count:           115+
unsafe blocks:        0
unwrap on user input: 0
Dependencies:         ~40 crates (via cargo tree)
```

## Appendix B: Reference Audits

These published audit reports from similar projects provide context for what auditors find:

- **Liana Wallet** (Wizardsardine) — Closest comparable project, also uses miniscript for inheritance
- **Bitcoin Core** — Multiple audits by NCC Group and others
- **rust-bitcoin / rust-miniscript** — Upstream library audits
- **Zcash (Least Authority)** — Cryptographic implementation review
- **Lightning Network (various implementations)** — PSBT and transaction handling

---

*An audit is not a seal of perfection. It's a professional assessment of risk at a point in time. The real security comes from the ongoing process: testing, reviewing, updating, and staying vigilant.*
