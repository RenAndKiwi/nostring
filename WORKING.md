# WORKING.md - NoString Build Session

**Current Phase:** 3 - Shamir Backup
**Last Completed:** Phase 2 (Inheritance MVP) - 17 tests passing

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

## Phase 3: Shamir Backup

**Goal:** Split seed for resilient backup

### Sub-tasks:
- [ ] 3.1: Research SLIP-39 implementation
- [ ] 3.2: Implement SLIP-39 share generation
- [ ] 3.3: Implement SLIP-39 share recovery
- [ ] 3.4: Codex32 compatibility layer
- [ ] 3.5: M-of-N threshold configuration
- [ ] 3.6: QR code export for shares

### References:
- SLIP-39: https://github.com/satoshilabs/slips/blob/master/slip-0039.md
- Codex32: https://github.com/roconnor-blockstream/SSS32

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
