# NoString Roadmap

## Vision

Encrypted email + Nostr identity + Bitcoin inheritance. One seed, sovereign comms, planned succession.

---

## Phase 0: Foundation (Current)

**Status:** Planning

- [x] Define core mission and scope
- [x] Identify upstream projects (nostr-mail, Liana)
- [x] Document architecture
- [ ] Fork nostr-mail as base
- [ ] Set up development environment
- [ ] Verify nostr-mail builds and runs locally

**Deliverable:** Working fork of nostr-mail, builds cleanly

---

## Phase 1: Unified Seed

**Goal:** One BIP-39 seed → Nostr keys + Bitcoin keys

- [ ] Implement BIP-39 seed generation/import
- [ ] Derive Nostr keys via NIP-06 (m/44'/1237'/0'/0/0)
- [ ] Derive Bitcoin keys via BIP-84 (m/84'/0'/0')
- [ ] Secure seed storage (encrypted at rest)
- [ ] Seed backup flow (display mnemonic, verify)

**Deliverable:** App uses single seed for both Nostr and Bitcoin

---

## Phase 2: Inheritance MVP

**Goal:** Single-heir timelock with manual check-in

- [ ] Integrate Liana wallet core (miniscript engine)
- [ ] Create inheritance policy: owner OR (heir + timelock)
- [ ] Implement check-in transaction (spend + recreate UTXO)
- [ ] Manual "I'm alive" button
- [ ] Heir key import (xpub from heir's wallet)

**Deliverable:** Working deadman switch — heir can claim after timeout

---

## Phase 3: Shamir Backup

**Goal:** Split seed for resilient backup

### Digital Path (SLIP-39)
- [ ] SLIP-39 Shamir implementation
- [ ] Configure M-of-N threshold
- [ ] Export shares as text/QR

### Physical Path (Codex32)
- [ ] Codex32 share generation (compatible with BIP-39 reconstruction)
- [ ] Documentation for offline volvelle splitting
- [ ] Verification checksums

**Deliverable:** Seed can be split and reconstructed via either method

---

## Phase 4: Multi-Heir + Cascade

**Goal:** Multiple recovery paths with different timelocks

- [ ] Multiple heir support in policy
- [ ] Cascade timelocks (spouse at 6mo, kids at 9mo, executor at 12mo)
- [ ] Threshold signatures (2-of-3 heirs)
- [ ] Policy builder UI

**Deliverable:** Full Liana-style inheritance policies

---

## Phase 5: UX Polish

**Goal:** Frictionless check-ins, notifications, mobile

- [ ] Auto check-in on any wallet transaction
- [ ] Push notifications for check-in reminders
- [ ] Heir onboarding wizard
- [ ] Mobile app (Tauri Android)
- [ ] Hardware wallet support for signing

**Deliverable:** Production-ready UX

---

## Phase 6: Self-Hosting & Docs

**Goal:** Anyone can run their own stack

- [ ] Docker compose for email server
- [ ] Infrastructure inheritance docs (how heirs take over server)
- [ ] Operational runbook
- [ ] Security audit preparation

**Deliverable:** Complete self-hosting guide

---

## Timeline Estimate

| Phase | Duration | Cumulative |
|-------|----------|------------|
| 0: Foundation | 2 weeks | 2 weeks |
| 1: Unified Seed | 3 weeks | 5 weeks |
| 2: Inheritance MVP | 4 weeks | 9 weeks |
| 3: Shamir Backup | 3 weeks | 12 weeks |
| 4: Multi-Heir | 3 weeks | 15 weeks |
| 5: UX Polish | 4 weeks | 19 weeks |
| 6: Self-Hosting | 2 weeks | 21 weeks |

**Total: ~5 months to production-ready**

---

## Open Questions

1. **Encrypted pointer storage** — OP_RETURN vs off-chain? Size limits?
2. **Hardware wallet Nostr signing** — NIP-06 support in BitBox/Coldcard?
3. **Mobile priority** — Desktop-first or mobile-first?
4. **Relay selection** — Default relays vs user-configured only?

---

*Last updated: 2026-02-01*
