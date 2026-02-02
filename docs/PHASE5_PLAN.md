# Phase 5: UX Polish — Implementation Plan

## Overview

Transform NoString from library code into a production-ready desktop application with:
- Check-in reminder notifications
- Heir onboarding flow
- Tauri desktop app
- Auto check-in detection
- SeedSigner hardware wallet integration

---

## 5.1 Notifications (Check-in Reminders)

### Requirements
- Alert users before timelock expiry
- Multiple reminder thresholds: 30 days, 7 days, 1 day, urgent
- Delivery via email and/or Telegram

### Implementation
1. **Timelock Monitor Service**
   - Track inheritance UTXOs
   - Calculate blocks until expiry
   - Trigger alerts at configured thresholds

2. **Email Notifications**
   - SMTP integration via `lettre` crate
   - Template: "Your NoString check-in expires in X days"

3. **Telegram Notifications**
   - Use existing OpenClaw Telegram integration
   - Or standalone bot via `teloxide` crate

### Files
- `crates/nostring-notify/` — new crate

---

## 5.2 Heir Onboarding

### Requirements
- Guide heirs through xpub export
- Explain inheritance in plain language
- Distribute Shamir shares if applicable

### Implementation
1. **Onboarding Documentation**
   - `docs/HEIR_GUIDE.md` — step-by-step for heirs
   - `docs/CLAIM_GUIDE.md` — how to claim after owner passes

2. **Xpub Collection Flow**
   - QR scan of heir's xpub (from their wallet)
   - Validate as valid BIP-84 xpub
   - Add to policy

3. **Shamir Share Distribution**
   - Generate Codex32 shares for heir
   - Print-friendly format

### Files
- `docs/HEIR_GUIDE.md`
- `docs/CLAIM_GUIDE.md`
- UI components in Tauri app

---

## 5.3 Desktop App (Tauri)

### Requirements
- Cross-platform: macOS, Linux, Windows
- Rust backend, lightweight frontend
- Core flows: seed setup, policy management, check-in

### Tech Stack
- **Framework:** Tauri 2.x
- **Frontend:** Leptos (Rust WASM) or simple HTML/JS
- **Backend:** Existing nostring-* crates

### Implementation
1. **Tauri Shell Setup**
   ```
   tauri-app/
   ├── src-tauri/          # Rust backend
   │   ├── src/
   │   │   ├── main.rs
   │   │   ├── commands.rs  # Tauri commands
   │   │   └── state.rs     # App state
   │   └── Cargo.toml
   ├── src/                 # Frontend
   │   └── index.html
   └── tauri.conf.json
   ```

2. **Core Screens**
   - Welcome / First Run
   - Seed Import or Create
   - Policy Status Dashboard
   - Check-in Action
   - Settings

3. **Tauri Commands (Rust → Frontend)**
   - `create_seed()` → Generate BIP-39 mnemonic
   - `import_seed(mnemonic)` → Validate & store
   - `get_policy_status()` → Current timelock state
   - `initiate_checkin()` → Create unsigned PSBT
   - `complete_checkin(signed_psbt)` → Broadcast

### Files
- `tauri-app/src-tauri/` — Rust backend
- `tauri-app/src/` — Frontend

---

## 5.4 Auto Check-in

### Requirements
- Detect any spend from inheritance UTXO
- Automatically rebuild UTXO with fresh timelock
- Or: notify user to manually check in

### Implementation
1. **UTXO Watcher**
   - Connect to Bitcoin node/API (Esplora, Electrum)
   - Monitor inheritance address(es)
   - Detect confirmations

2. **Transaction Detection**
   - If UTXO spent → owner is alive → may need new inheritance UTXO
   - Parse transaction to understand if it's:
     - Owner spending (check-in)
     - Heir claiming (timeout passed)

3. **Fresh UTXO Creation**
   - Build new inheritance output
   - Requires signing (user action or auto if hot)

### Files
- `crates/nostring-watch/` — new crate for blockchain monitoring
- Integration with nostring-inherit

---

## 5.5 SeedSigner Integration

### Requirements
- Air-gapped signing via QR codes
- Support for check-in transactions
- PSBT round-trip: generate → display → scan → broadcast

### QR Protocol
SeedSigner uses **BC-UR v2** (Blockchain Commons Uniform Resources):
- Format: `UR:CRYPTO-PSBT`
- Animated QR for large PSBTs (fountain codes)

### Implementation
1. **PSBT Generation**
   - Already have miniscript policies in nostring-inherit
   - Create spending transaction for check-in
   - Output as PSBT

2. **UR Encoding**
   - Crate: `ur` or implement BC-UR spec
   - Encode PSBT bytes to UR format
   - Split into animated frames if needed

3. **QR Display**
   - Crate: `qrcode` + image rendering
   - Animated display in Tauri window
   - Frame rate ~10 fps

4. **QR Scanning**
   - Webcam capture via `nokhwa` or `eye` crate
   - Decode QR frames
   - Reassemble animated UR sequence
   - Decode UR back to PSBT bytes

5. **Broadcast**
   - Extract signed PSBT
   - Finalize transaction
   - Broadcast via Esplora/Electrum

### Files
- `crates/nostring-qr/` — QR encode/decode + UR format
- `crates/nostring-seedsigner/` — SeedSigner-specific flow

---

## Dependencies (New Crates)

```toml
# QR codes
qrcode = "0.14"
image = "0.25"

# Webcam
nokhwa = "0.10"  # or rscam

# UR encoding (Blockchain Commons)
ur = "0.4"  # or implement from spec

# Notifications
lettre = "0.11"  # Email
teloxide = "0.12"  # Telegram (optional)

# Tauri
tauri = "2.0"

# Bitcoin API
esplora-client = "0.6"  # or electrum-client
```

---

## Execution Order

| Task | Dependencies | Estimate |
|------|--------------|----------|
| 5.3a Tauri shell setup | None | 0.5 day |
| 5.3b Basic UI (seed import, status) | 5.3a | 1 day |
| 5.5a PSBT generation | nostring-inherit | 0.5 day |
| 5.5b UR encoding | None | 0.5 day |
| 5.5c QR display in Tauri | 5.3a, 5.5b | 0.5 day |
| 5.5d QR scanning (webcam) | 5.3a | 1 day |
| 5.5e Full SeedSigner flow | 5.5a-d | 0.5 day |
| 5.4 Auto check-in (UTXO watch) | 5.5 | 1 day |
| 5.1 Notifications | 5.4 | 1 day |
| 5.2 Heir onboarding docs | None | 0.5 day |

**Total: ~7 days**

---

## Security Considerations

- [ ] Never store unencrypted seeds
- [ ] Webcam feed is local only (no network)
- [ ] PSBT validation before broadcast
- [ ] Timelock verification before signing
- [ ] Notification content doesn't leak wallet details

---

## Decisions (Confirmed 2026-02-02)

1. **Frontend:** HTML/JS (simpler, faster iteration)
2. **Bitcoin API:** Electrum (established, better privacy with own server)
3. **Hardware wallet:** SeedSigner (via BC-UR QR codes)
4. **Notifications:** Email + Telegram
5. **Mobile:** Skip for MVP

---

*Created: 2026-02-02*
*bb-feature Phase 1: Research & Plan ✅*
