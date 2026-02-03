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

## Phase 8: Nostr Identity Inheritance ✅

**Status:** Complete (2026-02-03) — see [NOSTR_INHERITANCE.md](NOSTR_INHERITANCE.md)

### 8.1 nsec Shamir Split ✅
- [x] Optional nsec input during setup wizard (step 3 of 4)
- [x] Also accessible from Settings → Identity Inheritance
- [x] Calculate threshold/shares: N heirs → (N+1)-of-(2N+1) split
- [x] Generate Codex32 shares of nsec
- [x] Display per-heir shares with copy buttons + distribution instructions
- [x] Zero nsec from memory after split (zeroize crate)
- [x] Locked shares persisted to SQLite, included in descriptor backup
- [x] Owner npub stored so UI shows identity inheritance status

### 8.2 Heir Recovery Flow ✅
- [x] "Recover a Loved One's Identity" mode on setup screen
- [x] Dynamic share input (add more fields as needed)
- [x] Enter shares → verify threshold → reveal nsec + npub
- [x] nsec blurred by default with reveal button
- [x] Copy nsec to clipboard for import into Nostr client
- [x] Validates recovered bytes are a valid Nostr secret key
- [x] Descriptor backup includes locked shares + recovery instructions

### 8.3 Nostr Relay Storage (Future Enhancement)
- [ ] Publish encrypted locked shares to multiple relays
- [ ] Heir pre-fetch mechanism
- [ ] Redundancy across relays

---

## Phase 9: Spend Type Detection ✅

**Status:** Complete (2026-02-04)

### 9.1 Witness Analysis Engine ✅
- [x] `spend_analysis` module in `nostring-watch`
- [x] Witness stack analysis: owner path (1 stack item) vs heir path (2+ with empty dummy)
- [x] Cascade policy support (nested `or_d` branches detected)
- [x] Timing-based fallback (spend before timelock expiry = must be owner)
- [x] Combined analysis: witness primary, timing fallback, confidence scoring
- [x] `analyze_transaction_for_outpoint` — find specific input in a transaction
- [x] 14 unit tests for witness/timing analysis

### 9.2 Electrum Integration ✅
- [x] `get_script_history` added to `ElectrumClient` (all txs for a script)
- [x] `ScriptHistoryItem` type for history results
- [x] `WatchService::detect_spend_type_for_utxo` — full pipeline: find spending tx → analyze witness
- [x] `WatchService::find_spending_tx` — scan script history to locate the spending transaction

### 9.3 Database & Tauri Commands ✅
- [x] `spend_events` table: txid, spend_type, confidence, method, policy_id, outpoint
- [x] `spend_type` column added to `checkin_log` (with migration for existing DBs)
- [x] `detect_spend_type` Tauri command: fetch tx via Electrum → analyze → log
- [x] `get_spend_events` Tauri command: list all detected spend events
- [x] `check_heir_claims` Tauri command: boolean alert for heir claim detection
- [x] 3 new DB tests (spend events CRUD, heir claim detection, typed checkin log)

---

## Phase 10: Release & Hardening

- [ ] Build release binaries (macOS/Win/Linux)
- [ ] Security audit preparation
- [ ] Docker compose for self-hosting
- [ ] Mobile consideration (Tauri mobile or separate app)
- [ ] nsec revocation / re-split flow
- [ ] Dashboard UI: spend type icons (✅ Owner check-in vs ⚠️ Heir claim)
- [ ] Heir claim alert banner in dashboard

---

## Crate Structure

```
nostring/
├── crates/
│   ├── nostring-core      # Seed, crypto, BIP-39/84 (22 tests)
│   ├── nostring-inherit   # Policies, miniscript (25 tests)
│   ├── nostring-shamir    # SLIP-39, Codex32 (39 tests)
│   ├── nostring-electrum  # Bitcoin network (3 tests, 2 ignored)
│   ├── nostring-notify    # Email + Nostr DM (27 tests)
│   ├── nostring-watch     # UTXO monitoring + spend analysis (29 tests, 2 ignored)
│   └── nostring-email     # IMAP (placeholder)
├── tauri-app/             # Desktop application (13 tests)
├── tests/e2e/             # Integration test suite (18 tests, 2 ignored)
└── docs/                  # Documentation
```

---

## Test Coverage

| Crate | Tests | Status |
|-------|-------|--------|
| nostring-app | 13 | ✅ |
| nostring-core | 22 | ✅ |
| nostring-inherit | 25 | ✅ |
| nostring-shamir | 39 | ✅ |
| nostring-electrum | 3 | ✅ (2 ignored) |
| nostring-notify | 27 | ✅ (2 ignored) |
| nostring-watch | 29 | ✅ (2 ignored) |
| nostring-e2e | 18 | ✅ (2 ignored) |
| **Total** | **172** | ✅ |

*Ignored tests require network access. Run with `--ignored`.*

---

*Last updated: 2026-02-04*
