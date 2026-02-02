# NoString Roadmap

## Vision

Encrypted email + Nostr identity + Bitcoin inheritance. One seed, sovereign comms, planned succession.

---

## Phase 0: Foundation ✅

**Status:** Complete

- [x] Define core mission and scope
- [x] Identify upstream projects (nostr-mail, Liana)
- [x] Document architecture
- [x] Set up development environment

---

## Phase 1: Unified Seed ✅

**Status:** Complete (23 tests)

- [x] BIP-39 seed generation/import
- [x] Derive Nostr keys via NIP-06
- [x] Derive Bitcoin keys via BIP-84
- [x] Secure seed storage (AES-GCM + Argon2)
- [x] Seed backup flow

---

## Phase 2: Inheritance MVP ✅

**Status:** Complete (25 tests)

- [x] Miniscript policy engine
- [x] Inheritance policy: owner OR (heir + timelock)
- [x] Check-in transaction builder
- [x] Heir key import (xpub)

---

## Phase 3: Shamir Backup ✅

**Status:** Complete (39 tests)

- [x] SLIP-39 with proper RS1024 checksum
- [x] Codex32 (BIP-93) with BCH checksum
- [x] M-of-N threshold configuration
- [x] Share generation and reconstruction

---

## Phase 4: Multi-Heir + Cascade ✅

**Status:** Complete (integrated in Phase 2 tests)

- [x] Multiple heir support
- [x] Cascade timelocks (spouse → kids → executor)
- [x] Threshold signatures (2-of-3 heirs)
- [x] Policy compiles to valid wsh() descriptors

---

## Phase 5: UX Polish ✅

**Status:** Complete (117 tests total)

### 5.1 Notifications ✅
- [x] Email via SMTP (lettre)
- [x] Nostr DM via NIP-04 (nostr-sdk)
- [x] Configurable thresholds (30, 7, 1, 0 days)
- **Crate:** `nostring-notify` (15 tests)

### 5.2 Heir Onboarding ✅
- [x] HEIR_GUIDE.md — setup for heirs
- [x] CLAIM_GUIDE.md — emergency claim procedure
- [x] Tauri commands: add/list/remove heir
- [x] Xpub validation

### 5.3 Desktop App (Tauri) ✅
- [x] Tauri shell with Rust backend
- [x] Seed import/create flow
- [x] Policy status dashboard
- [x] Check-in transaction builder

### 5.4 Auto Check-in ✅
- [x] UTXO monitoring service
- [x] Event detection (appeared, spent, warning)
- [x] Persistent state tracking
- [x] Rate limiting
- **Crate:** `nostring-watch` (13 tests + 2 integration)

### 5.5 Hardware Wallet Integration ✅
- [x] PSBT generation for check-in
- [x] QR code display (qrcode.js)
- [x] QR scanning (jsQR + webcam)
- [x] Electrum air-gap flow (base64 PSBTs)

**Decision:** Electrum watch-only over SeedSigner BC-UR (simpler, wider adoption)

---

## Phase 6: Self-Hosting & Docs 🔄

**Status:** In Progress

- [ ] Docker compose for deployment
- [ ] Infrastructure inheritance docs
- [ ] Operational runbook
- [ ] Security audit preparation
- [ ] README and contribution guide

---

## Phase 7: Polish (Future)

- [ ] Tauri UI for heir management
- [ ] Background polling integration
- [ ] Spend type detection (owner vs heir)
- [ ] End-to-end testnet testing
- [ ] Mobile consideration

---

## Crate Structure

```
nostring/
├── crates/
│   ├── nostring-core      # Seed, crypto, BIP-39/84 (23 tests)
│   ├── nostring-inherit   # Policies, miniscript (25 tests)
│   ├── nostring-shamir    # SLIP-39, Codex32 (39 tests)
│   ├── nostring-electrum  # Bitcoin network (4 tests)
│   ├── nostring-notify    # Email + Nostr DM (15 tests)
│   ├── nostring-watch     # UTXO monitoring (15 tests)
│   └── nostring-email     # IMAP (placeholder)
├── tauri-app/             # Desktop application
└── docs/                  # Documentation
```

---

## Test Coverage

| Crate | Tests | Status |
|-------|-------|--------|
| nostring-core | 23 | ✅ |
| nostring-inherit | 25 | ✅ |
| nostring-shamir | 39 | ✅ |
| nostring-electrum | 4 | ✅ (2 ignored) |
| nostring-notify | 15 | ✅ |
| nostring-watch | 15 | ✅ (2 ignored) |
| **Total** | **121** | ✅ |

*Ignored tests require network access. Run with `--ignored`.*

---

*Last updated: 2026-02-02*
