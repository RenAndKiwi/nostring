# NoString Roadmap

## Vision

Sovereign Bitcoin inheritance with optional Nostr identity inheritance. Watch-only wallet, timelocked policies, no trusted third parties.

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

**Status:** Complete (121 tests total)

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
- **Crate:** `nostring-watch` (15 tests)

### 5.5 Hardware Wallet Integration ✅
- [x] PSBT generation for check-in
- [x] QR code display (QRious)
- [x] QR scanning (jsQR + webcam)
- [x] Electrum air-gap flow (base64 PSBTs)

**Decision:** Electrum watch-only over SeedSigner BC-UR (simpler, wider adoption)

---

## Phase 6: Architecture Pivot ✅

**Status:** Complete (2026-02-02)

- [x] Watch-only wallet as primary mode (xpub import, no seed on device)
- [x] Setup wizard (add heirs → review → complete)
- [x] Descriptor backup (download button + post-check-in prompt)
- [x] "How It Works" in-app education
- [x] Toast notifications (replace alerts)
- [x] QR code fix (browser-native library)

---

## Phase 7: Persistence + Notifications ✅

**Status:** Complete (2026-02-03)

### 7.1 SQLite Persistence ✅
- [x] SQLite database in Tauri app data dir
- [x] `config` table: key-value store for wallet state, service key, settings
- [x] `heirs` table: structured heir registry
- [x] `checkin_log` table: timestamped check-in history
- [x] Write-through helpers (every mutation persists immediately)
- [x] Load on startup — state survives app restarts
- [x] Watch-only wallets auto-unlock on restart

### 7.2 Service Key Notifications ✅
- [x] Generate Nostr keypair (service key) on first setup
- [x] Service key persisted to SQLite
- [x] NIP-04 encrypted DMs to owner's npub
- [x] Notification thresholds: 30/7/1/0 days
- [x] Tauri commands: configure_notifications, send_test_notification, check_and_notify
- [x] Auto-check on status refresh (fires if thresholds hit)
- [x] Frontend: notification settings UI (owner npub, email config, test DM)
- [x] Email notifications via SMTP (configurable)

### 7.3 End-to-End Testing ✅
- [x] 10 offline integration tests covering full inheritance flow
- [x] 2 network tests (mainnet + testnet Electrum)
- [x] nsec inheritance formula verification
- [x] All 131 workspace tests passing

---

## Phase 8: Nostr Identity Inheritance 🔜

**Status:** Next — see [NOSTR_INHERITANCE.md](NOSTR_INHERITANCE.md)

### 8.1 nsec Shamir Split
- [ ] Optional nsec input during setup
- [ ] Calculate threshold/shares: N heirs → (N+1)-of-(2N+1) split
- [ ] Generate Codex32 shares of nsec
- [ ] Display pre-distribution shares with heir instructions
- [ ] Zero nsec from memory after split
- [ ] Include locked shares (encrypted) in descriptor backup

### 8.2 Heir Recovery Flow
- [ ] Share combination tool in app
- [ ] Enter shares → verify threshold → reveal nsec
- [ ] Update CLAIM_GUIDE.md with Nostr recovery steps
- [ ] Update HEIR_GUIDE.md with share storage instructions

### 8.3 Nostr Relay Storage (Optional Enhancement)
- [ ] Publish encrypted locked shares to multiple relays
- [ ] Heir pre-fetch mechanism
- [ ] Redundancy across relays

---

## Phase 9: Release & Hardening

- [ ] Build release binaries (macOS/Win/Linux)
- [ ] Security audit preparation
- [ ] Docker compose for self-hosting
- [ ] Mobile consideration (Tauri mobile or separate app)
- [ ] Spend type detection (owner vs heir)
- [ ] nsec revocation / re-split flow

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
├── tests/e2e/             # Integration test suite (10 tests)
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
| nostring-e2e | 12 | ✅ (2 ignored) |
| **Total** | **133** | ✅ |

*Ignored tests require network access. Run with `--ignored`.*

---

*Last updated: 2026-02-03*
