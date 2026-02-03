# Security Audit Preparation

This document prepares NoString for security review.

---

## Scope

### In Scope

| Component | Location | Priority |
|-----------|----------|----------|
| Seed encryption | nostring-core/src/crypto.rs | Critical |
| Key derivation | nostring-core/src/seed.rs | Critical |
| Miniscript policies | nostring-inherit/src/policy.rs | Critical |
| PSBT handling | nostring-inherit/src/checkin.rs | Critical |
| Shamir implementation | nostring-shamir/ | High |
| Electrum client | nostring-electrum/ | Medium |
| Notification service | nostring-notify/ | Low |
| Watch service | nostring-watch/ | Low |

### Out of Scope

- Tauri framework itself
- Third-party dependencies (separate audits)
- Frontend JavaScript (non-critical path)

---

## Cryptographic Primitives

### Seed Encryption

| Property | Implementation |
|----------|----------------|
| Algorithm | AES-256-GCM |
| KDF | Argon2id |
| Nonce | 12 bytes, random |
| Salt | 32 bytes, random |
| Argon2 params | m=64MB, t=3, p=1 |

**Location:** `crates/nostring-core/src/crypto.rs`

**Review points:**
- [ ] Nonce reuse prevention
- [ ] Constant-time comparison
- [ ] Memory zeroing after use
- [ ] Salt uniqueness

### Key Derivation

| Standard | Path | Purpose |
|----------|------|---------|
| BIP-39 | — | Mnemonic generation |
| BIP-32 | — | HD key derivation |
| BIP-84 | m/84'/0'/0' | Bitcoin SegWit |
| NIP-06 | m/44'/1237'/0'/0/0 | Nostr identity |

**Location:** `crates/nostring-core/src/seed.rs`

**Review points:**
- [ ] Correct derivation paths
- [ ] Hardened derivation where required
- [ ] No key material logging

### Shamir Secret Sharing

| Variant | Standard | Checksum |
|---------|----------|----------|
| SLIP-39 | SLIP-0039 | RS1024 Reed-Solomon |
| Codex32 | BIP-93 | BCH |

**Locations:**
- `crates/nostring-shamir/src/slip39.rs`
- `crates/nostring-shamir/src/codex32.rs`

**Review points:**
- [ ] GF(256) arithmetic correctness
- [ ] Lagrange interpolation
- [ ] Checksum validation
- [ ] Threshold enforcement

---

## Transaction Security

### Miniscript Policies

```
wsh(or_d(pk(owner), and_v(v:pk(heir), older(blocks))))
```

**Location:** `crates/nostring-inherit/src/policy.rs`

**Review points:**
- [ ] Policy compiles to valid descriptors
- [ ] Timelock values are reasonable
- [ ] Multi-path policies are unambiguous
- [ ] No unexpected spending paths

### PSBT Handling

**Location:** `crates/nostring-inherit/src/checkin.rs`

**Review points:**
- [ ] Output validation (correct addresses)
- [ ] Fee validation (reasonable range)
- [ ] Change handling
- [ ] No information leakage in PSBT

---

## Network Security

### Electrum Client

**Location:** `crates/nostring-electrum/src/lib.rs`

**Review points:**
- [ ] TLS certificate validation
- [ ] No plaintext on mainnet by default
- [ ] Response validation
- [ ] No private key transmission

### Nostr DM

**Location:** `crates/nostring-notify/src/nostr_dm.rs`

**Review points:**
- [ ] NIP-04 encryption
- [ ] No key logging
- [ ] Relay connection security

---

## Data Handling

### Sensitive Data

| Data | Storage | Protection |
|------|---------|------------|
| Seed | Encrypted file | AES-256-GCM |
| Password | Never stored | User memory |
| xpubs | Config file | Not secret |
| UTXOs | State file | Public data |

### Memory Handling

**Review points:**
- [ ] Seed zeroed after use
- [ ] No swap file leakage
- [ ] No core dump with secrets

---

## Known Limitations

### Documented

1. **Watch service spend detection** — Returns `Unknown`, doesn't analyze spending tx
2. **Electrum privacy** — Queries leak address interest to server
3. **Notification metadata** — Email/DM reveal timing information

### Accepted Risks

1. **Public Electrum servers** — Mitigated by self-hosting option
2. **Single-device seed** — Mitigated by Shamir backup

---

## Test Coverage

### Critical Paths

| Test | Location | Coverage |
|------|----------|----------|
| Encryption roundtrip | nostring-core | ✅ |
| Key derivation | nostring-core | ✅ |
| Policy compilation | nostring-inherit | ✅ |
| PSBT generation | nostring-inherit | ✅ |
| Shamir split/combine | nostring-shamir | ✅ |
| RS1024 checksum | nostring-shamir | ✅ |
| Codex32 BCH | nostring-shamir | ✅ |

### Test Vectors

- BIP-39 test vectors
- BIP-93 (Codex32) test vectors
- Custom miniscript compilation tests

---

## Pre-Audit Review (2026-02-02)

### Code Review Findings

| Check | Status | Notes |
|-------|--------|-------|
| `unsafe` blocks | ✅ Pass | Zero unsafe blocks in codebase |
| `unwrap()` on user input | ✅ Pass | All user-facing paths use Result |
| `panic!` in production | ✅ Pass | Only in tests |
| Secret logging | ✅ Pass | No seed/key logging found |
| Dependencies pinned | ✅ Pass | Cargo.lock committed |

### Production `unwrap()` Audit

Found 167 `unwrap()` calls, but all are in:
- Test functions
- Constant parsing (e.g., email address literals)
- Post-validation contexts (already checked)

**No `unwrap()` on untrusted input.**

### Memory Safety TODO

| Item | Status | Priority |
|------|--------|----------|
| Add `zeroize` to seed | ❌ TODO | High |
| Disable core dumps | ❌ TODO | Medium |
| mlock seed pages | ❌ TODO | Medium |

---

## Audit Preparation Checklist

### Code

- [x] All `unsafe` blocks documented (none exist)
- [x] No `unwrap()` on user input
- [x] Error messages don't leak secrets
- [x] Dependencies pinned to specific versions

### Documentation

- [x] SECURITY.md current
- [x] Threat model documented (docs/THREAT_MODEL.md)
- [x] Known limitations listed

### Testing

- [x] All tests pass (115)
- [ ] Fuzzing targets available
- [x] Edge cases covered

### Process

- [ ] Reproducible builds
- [ ] Signed releases
- [x] Vulnerability disclosure process

---

## Recommended Audit Focus

1. **Critical:** Seed encryption/decryption cycle
2. **Critical:** Miniscript policy → descriptor → address
3. **High:** Shamir share generation and reconstruction
4. **High:** PSBT validation before broadcast
5. **Medium:** Electrum response parsing
6. **Low:** Notification content generation

---

## Contact

Security issues: security@nostring.dev

---

*Audit for confidence. Document for transparency.*
