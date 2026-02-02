# NoString Master Plan

*Every feature planned. Every component considered. The right way.*

---

## Methodology

Adapted from bb-feature workflow:

### For Each Feature:
1. **Research & Plan** — Understand the problem, research solutions, document options
2. **Security Review** — Attack surfaces, trust boundaries, mitigations
3. **Design** — Types, interfaces, data flow
4. **Implementation** — Minimal, focused, elegant code
5. **Testing** — Unit tests, integration tests, edge cases
6. **Reflection** — What worked, what didn't, lessons learned
7. **Documentation** — Update docs, add comments where non-obvious

### Principles:
- No code without a plan
- No feature without tests
- No shortcuts that create debt
- Reflect before moving on

---

## Phase Overview

| Phase | Name | Goal | Features |
|-------|------|------|----------|
| 0 | Foundation | Working dev environment | 4 |
| 1 | Unified Seed | One seed → Nostr + Bitcoin | 5 |
| 2 | Inheritance MVP | Single-heir timelock | 6 |
| 3 | Shamir Backup | SLIP-39 + Codex32 | 4 |
| 4 | Multi-Heir | Cascade timelocks | 4 |
| 5 | UX Polish | Production-ready | 5 |
| 6 | Self-Hosting | Complete guide | 3 |

**Total: 31 features**

---

## Phase 0: Foundation

**Goal:** Working Rust workspace with all crates building.

### 0.1 — Dev Environment Setup
- [ ] Rust toolchain (stable)
- [ ] Required system dependencies
- [ ] IDE configuration
- [ ] Build verification script

### 0.2 — Workspace Configuration
- [ ] Cargo workspace builds without errors
- [ ] All crate dependencies resolve
- [ ] Common dependencies shared properly

### 0.3 — nostr-mail Analysis
- [ ] Clone and build original nostr-mail
- [ ] Identify core modules to port
- [ ] Document API surface we need
- [ ] Note what we can discard

### 0.4 — Liana Core Analysis
- [ ] Study Liana's miniscript usage
- [ ] Identify descriptor patterns
- [ ] Document policy construction
- [ ] Note integration points

---

## Phase 1: Unified Seed

**Goal:** BIP-39 seed derives both Nostr and Bitcoin keys correctly.

### 1.1 — BIP-39 Mnemonic Generation
- [ ] Generate 24-word mnemonics
- [ ] Validate mnemonic checksums
- [ ] Parse existing mnemonics
- [ ] Handle optional passphrase

### 1.2 — Seed Derivation
- [ ] Derive 64-byte seed from mnemonic
- [ ] Test vectors from BIP-39 spec
- [ ] Passphrase handling

### 1.3 — NIP-06 Nostr Key Derivation
- [ ] Derive from path m/44'/1237'/0'/0/0
- [ ] Produce valid Nostr keypair
- [ ] Test against known NIP-06 vectors

### 1.4 — BIP-84 Bitcoin Key Derivation
- [ ] Derive from path m/84'/0'/0'
- [ ] Produce valid xpriv/xpub
- [ ] Test against known BIP-84 vectors

### 1.5 — Encrypted Seed Storage
- [ ] Argon2id key derivation from password
- [ ] AES-256-GCM encryption
- [ ] Secure storage format
- [ ] Decryption and verification

---

## Phase 2: Inheritance MVP

**Goal:** Single heir can claim after timelock expires.

### 2.1 — Miniscript Policy Construction
- [ ] Define policy: `or(pk(owner), and(pk(heir), older(N)))`
- [ ] Compile to miniscript
- [ ] Generate descriptor

### 2.2 — Timelock UTXO Creation
- [ ] Create address from descriptor
- [ ] Fund with small amount
- [ ] Verify script correctness

### 2.3 — Check-in Transaction
- [ ] Spend UTXO via owner path
- [ ] Recreate with same policy
- [ ] Broadcast transaction

### 2.4 — Heir Key Import
- [ ] Accept heir's xpub
- [ ] Derive heir's pubkey for policy
- [ ] Store heir configuration

### 2.5 — Timelock Monitoring
- [ ] Track current timelock status
- [ ] Calculate blocks remaining
- [ ] Warning thresholds

### 2.6 — Recovery Instructions
- [ ] Generate heir recovery doc
- [ ] Include descriptor
- [ ] Step-by-step claim process

---

## Phase 3: Shamir Backup

**Goal:** Seed can be split and reconstructed via SLIP-39 or Codex32.

### 3.1 — SLIP-39 Implementation
- [ ] Split seed into shares
- [ ] M-of-N threshold
- [ ] Reconstruct from shares
- [ ] Validate against test vectors

### 3.2 — Codex32 Share Generation
- [ ] Bech32 encoding
- [ ] Checksum computation
- [ ] Compatible with BIP-39 reconstruction

### 3.3 — Share Management
- [ ] Label shares (human-readable)
- [ ] Export as QR codes
- [ ] Verify share validity

### 3.4 — Reconstruction Flow
- [ ] Collect M shares
- [ ] Reconstruct seed
- [ ] Verify against known fingerprint

---

## Phase 4: Multi-Heir + Cascade

**Goal:** Multiple heirs with different timelocks.

### 4.1 — Multi-Heir Policy
- [ ] Multiple heir keys in policy
- [ ] Threshold signatures (2-of-3, etc.)
- [ ] Test complex policies

### 4.2 — Cascade Timelocks
- [ ] First recovery path (e.g., 6 months)
- [ ] Second recovery path (e.g., 9 months)
- [ ] Executor fallback (e.g., 12 months)

### 4.3 — Policy Builder UI
- [ ] Add/remove heirs
- [ ] Configure thresholds
- [ ] Set timelock durations

### 4.4 — Policy Verification
- [ ] Compile and test all paths
- [ ] Verify recovery scenarios
- [ ] Document policy in human terms

---

## Phase 5: UX Polish

**Goal:** Production-ready user experience.

### 5.1 — Auto Check-in
- [ ] Check-in on any wallet activity
- [ ] Background refresh option
- [ ] Minimize user friction

### 5.2 — Notifications
- [ ] Check-in reminders
- [ ] Timelock warnings
- [ ] Heir notifications (optional)

### 5.3 — Heir Onboarding Wizard
- [ ] Guided share distribution
- [ ] Heir receives instructions
- [ ] Test recovery with heir

### 5.4 — Mobile App
- [ ] Tauri Android build
- [ ] Touch-friendly UI
- [ ] Camera for QR scanning

### 5.5 — Hardware Wallet Support
- [ ] BitBox02 signing
- [ ] Coldcard support
- [ ] Ledger (if feasible)

---

## Phase 6: Self-Hosting

**Goal:** Anyone can run their own NoString stack.

### 6.1 — Docker Compose
- [ ] Email server (Postfix/Dovecot or similar)
- [ ] NoString app container
- [ ] Backup volumes

### 6.2 — Infrastructure Inheritance
- [ ] Server access in encrypted pointer
- [ ] DNS/domain transfer docs
- [ ] Handoff checklist

### 6.3 — Operational Runbook
- [ ] Maintenance procedures
- [ ] Troubleshooting guide
- [ ] Security hardening

---

## Tracking

Progress tracked in individual phase files:
- `docs/phases/PHASE_0.md`
- `docs/phases/PHASE_1.md`
- etc.

Each feature gets its own section with:
- Research notes
- Security considerations
- Implementation details
- Test results
- Reflections

---

*Let's build something beautiful.*
