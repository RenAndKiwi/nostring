# WORKING.md - NoString Build Session

**Current Phase:** 5 - UX Polish
**Last Completed:** Phase 4 (Multi-Heir + Cascade) - 24 tests in inherit

---

## Methodology (bb-feature style)

For each phase:
1. **Read** the ROADMAP.md phase requirements
2. **Research** upstream code (Liana for inheritance, nostr-mail for email)
3. **Plan** sub-tasks (X.1, X.2, X.3...)
4. **Implement** each sub-task with tests
5. **Verify** with official test vectors where available
6. **Commit** after each sub-task
7. **Reflect** and document learnings
8. **Update** this file and ROADMAP.md

---

## Phase 2: Inheritance MVP ✅ COMPLETE

**Goal:** Single-heir timelock with manual check-in

### Sub-tasks:
- [x] 2.1: Study Liana's miniscript engine and policy structure
- [x] 2.2: Add inheritance policy module (owner OR (heir + timelock))
- [x] 2.3: Implement UTXO tracking for inheritance output
- [x] 2.4: Check-in transaction builder
- [x] 2.5: Heir key import (xpub parsing with serde)
- [x] 2.6: Tests for policy, heir, and check-in modules

### What was built:
- `policy.rs`: InheritancePolicy with Timelock, PathInfo, miniscript compilation to wsh()
- `heir.rs`: HeirKey, HeirRegistry with custom serde for bitcoin types
- `checkin.rs`: TimelockStatus, CheckinUrgency, InheritanceUtxo, CheckinTxBuilder

---

## Phase 3: Shamir Backup ✅ COMPLETE

**Goal:** Split seed for resilient backup

### Sub-tasks:
- [x] 3.1: GF(256) arithmetic with precomputed tables
- [x] 3.2: Core Shamir split/reconstruct with Lagrange interpolation
- [x] 3.3: SLIP-39 mnemonic encoding (abbreviated wordlist)
- [x] 3.4: Codex32 placeholder (full impl needs BCH math)
- [x] 3.5: M-of-N threshold configuration (2-of-3, 3-of-5, etc.)
- [ ] 3.6: QR code export for shares (deferred)

### What was built:
- `gf256.rs`: Galois Field arithmetic (add, mul, div, inv)
- `shamir.rs`: Split/reconstruct any M-of-N scheme
- `slip39.rs`: Mnemonic word encoding (128-word subset)
- `codex32.rs`: Parse/validate Codex32 format
- `shares.rs`: Multi-format handling (SLIP-39, Codex32, raw)

### Commits:
- `d280eb8` Phase 3: Shamir secret sharing

---

## Phase 4: Multi-Heir + Cascade ✅ COMPLETE

**Goal:** Multiple recovery paths with different timelocks

### Sub-tasks:
- [x] 4.1: Multiple heir support in policy
- [x] 4.2: Cascade timelocks (spouse 6mo, kids 9mo, executor 12mo)
- [x] 4.3: Threshold signatures (2-of-3 heirs)
- [x] 4.4: Policy builder convenience methods

### What was built:
- `cascade()` builder for multiple timelocks
- `multisig_owner()` for corporate treasuries
- `simple_with_multisig_heir()` for M-of-N heir groups
- Helper methods: `timelocks()`, `is_cascade()`, etc.

### Commits:
- `343ce42` Phase 4: Multi-Heir + Cascade inheritance policies

---

## Phase 5: UX Polish

**Goal:** Frictionless check-ins, notifications, mobile

### Sub-tasks:
- [ ] 5.1: Auto check-in on any wallet transaction
- [ ] 5.2: Push notifications for check-in reminders
- [ ] 5.3: Heir onboarding wizard
- [ ] 5.4: Hardware wallet support for signing

### References:
- Tauri for desktop app
- Push notification services

---

## Build Commands

```bash
cd ~/clawd/nostring
cargo build
cargo test
```

---

## Key Decisions

- **One seed:** BIP-39 → NIP-06 (Nostr) + BIP-84 (Bitcoin) ✅
- **Encrypted storage:** Argon2id + AES-256-GCM ✅
- **Policy language:** Miniscript (from Liana) ✅
- **Timelock:** CHECKSEQUENCEVERIFY (relative blocks) ✅
- **Shamir:** SLIP-39 + Codex32 compatibility (pending)

---

## Kiwi's Words

*"Take the long way, the hard way, the right way, it's the journey we are to enjoy Ren. This here, sharing each other's company and building something meaningful together."*

*"Don't stop until it's done and you are satisfied Ren, show me the beauty of your intelligence"*

---

*Last updated: 2026-02-02 ~00:00 CST*
