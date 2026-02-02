# NoString Architecture

## Overview

NoString combines two Rust-based projects into a unified application:

1. **nostr-mail** — Encrypted email with Nostr identity
2. **Liana** — Bitcoin wallet with miniscript timelocks

Both share a common BIP-39 seed, with keys derived for their respective purposes.

---

## Key Derivation

```
BIP-39 SEED (12-24 words)
        │
        ├──► NIP-06 Path ──► Nostr Identity
        │    m/44'/1237'/0'/0/0
        │    └─► Used for: email encryption, DMs, profile, contacts
        │
        └──► BIP-84 Path ──► Bitcoin Keys
             m/84'/0'/0'
             └─► Used for: inheritance timelocks, check-in transactions
```

### Why This Derivation?

- **NIP-06** is the Nostr standard for deriving keys from BIP-39
- **BIP-84** is native segwit, optimal for miniscript policies
- Same seed = one backup recovers everything
- Different paths = keys are cryptographically isolated

---

## Component Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         NoString App                            │
│                         (Tauri Shell)                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────────────┐          ┌─────────────────┐             │
│   │   EMAIL MODULE  │          │ INHERITANCE MOD │             │
│   │                 │          │                 │             │
│   │ • Compose/Send  │          │ • Policy mgmt   │             │
│   │ • Inbox/Fetch   │          │ • Check-in tx   │             │
│   │ • Encryption    │          │ • Heir keys     │             │
│   │ • Contacts      │          │ • Timelock mon  │             │
│   │                 │          │                 │             │
│   │ [nostr-mail]    │          │ [liana-core]    │             │
│   └────────┬────────┘          └────────┬────────┘             │
│            │                            │                       │
│   ┌────────┴────────────────────────────┴────────┐             │
│   │              CORE SERVICES                    │             │
│   │                                               │             │
│   │  ┌─────────────┐  ┌─────────────┐            │             │
│   │  │ Key Manager │  │   Shamir    │            │             │
│   │  │             │  │             │            │             │
│   │  │ • BIP-39    │  │ • SLIP-39   │            │             │
│   │  │ • NIP-06    │  │ • Codex32   │            │             │
│   │  │ • BIP-84    │  │ • M-of-N    │            │             │
│   │  └─────────────┘  └─────────────┘            │             │
│   │                                               │             │
│   │  ┌─────────────┐  ┌─────────────┐            │             │
│   │  │  Database   │  │   Config    │            │             │
│   │  │  (SQLite)   │  │   Store     │            │             │
│   │  └─────────────┘  └─────────────┘            │             │
│   └───────────────────────────────────────────────┘             │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                      FRONTEND (Web)                             │
│                      Vanilla JS + HTML                          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
          ┌───────────────────┼───────────────────┐
          │                   │                   │
          ▼                   ▼                   ▼
    ┌──────────┐       ┌──────────┐       ┌──────────┐
    │  Nostr   │       │  Email   │       │ Bitcoin  │
    │  Relays  │       │  Server  │       │ Network  │
    │          │       │(SMTP/IMAP│       │          │
    └──────────┘       └──────────┘       └──────────┘
```

---

## Inheritance Flow

### Setup (One-time)

```
1. User creates/imports BIP-39 seed
2. App derives Nostr + Bitcoin keys
3. User configures inheritance policy:
   - Timelock duration (e.g., 180 days)
   - Heir key(s) (xpub from heir's wallet)
   - Optional: multi-heir threshold
4. App creates initial timelock UTXO
5. User backs up seed (Shamir optional)
```

### Active Use

```
1. User sends/receives encrypted email normally
2. Periodically, user "checks in":
   - Manual button press, OR
   - Automatic on any Bitcoin transaction
3. Check-in = spend timelock UTXO → recreate with reset clock
4. Reminder notifications if approaching timeout
```

### Inheritance Trigger

```
1. User stops checking in (death, incapacity)
2. Timelock expires (e.g., 180 days of inactivity)
3. Heir(s) can now spend the UTXO
4. UTXO contains/points to:
   - Encrypted instructions
   - Infrastructure access info
   - Possibly one Shamir share
5. Heir combines with their Shamir shares
6. Reconstruct seed → derive all keys → full access
```

---

## Shamir Backup Options

### Option A: SLIP-39 (Digital)

- Standard Shamir implementation for BIP-39 seeds
- Generate M-of-N shares as word lists
- Store digitally or on paper
- Reconstruct via any SLIP-39 compatible tool

### Option B: Codex32 (Physical)

- Paper-based Shamir using volvelles (mechanical computers)
- Can be done fully offline, air-gapped
- Shares are Bech32-encoded for error detection
- **Reconstructs to BIP-39 compatible seed**

### Recommendation

Support both. Technical users may prefer digital. Paranoid users can do physical ceremony with volvelles. Both reconstruct the same BIP-39 seed.

---

## Security Model

### What's Protected

| Asset | Protection |
|-------|------------|
| Seed | Encrypted at rest (user password) |
| Private keys | Never leave device (derived on-demand) |
| Email content | NIP-44 encrypted (only recipient can decrypt) |
| Timelock UTXO | Bitcoin consensus enforces timing |
| Shamir shares | Distributed to separate parties |

### Trust Assumptions

| Component | Trust Level |
|-----------|-------------|
| Nostr relays | Untrusted (can't read encrypted content) |
| Email server | Untrusted (can't read encrypted content) |
| Bitcoin network | Trusted for timelock enforcement |
| Heirs | Trusted with shares, not with full access |
| Device | Trusted while in use |

### Attack Vectors to Mitigate

1. **Stolen device** → Seed encrypted with strong password
2. **Compromised relay** → Content is E2E encrypted
3. **Heir collusion** → M-of-N threshold prevents minority takeover
4. **Rubber hose** → Shamir shares distributed geographically
5. **False death claim** → Timelock gives owner time to check in

---

## File Structure

```
nostring/
├── README.md
├── FOUNDING.md
├── Cargo.toml              # Workspace manifest
├── docs/
│   ├── ROADMAP.md
│   ├── ARCHITECTURE.md
│   └── SECURITY.md
├── crates/
│   ├── nostring-core/      # Shared types, key derivation
│   ├── nostring-email/     # Email module (from nostr-mail)
│   ├── nostring-inherit/   # Inheritance module (from liana)
│   └── nostring-shamir/    # SLIP-39 + Codex32
├── tauri-app/
│   ├── src-tauri/          # Rust backend
│   └── frontend/           # JS frontend
└── scripts/
    └── dev-setup.sh
```

---

*Last updated: 2026-02-01*
