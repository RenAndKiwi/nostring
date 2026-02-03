# NoString Architecture

## Overview

NoString is a sovereign Bitcoin inheritance tool with optional Nostr identity inheritance. It combines:

1. **Watch-only Bitcoin wallet** — monitors inheritance UTXOs, generates check-in PSBTs
2. **Miniscript policy engine** — creates timelocked inheritance descriptors
3. **Nostr service key** — sends check-in reminder DMs
4. **Shamir secret sharing** — splits nsec for identity inheritance

**Core principle:** Your Bitcoin keys never touch NoString. You sign externally with your hardware wallet. NoString is a coordinator, not a custodian.

---

## Key Architecture

```
OWNER'S HARDWARE WALLET (Cold)
    │
    └──► xpub (exported to NoString as watch-only)
         │
         ├──► Inheritance Descriptor ◄── Heir xpubs
         │    wsh(or_d(pk(owner), and_v(v:pk(heir), older(N))))
         │
         └──► Check-in PSBTs (signed on hardware wallet)


NOSTRING APP (Hot — but holds NO Bitcoin keys)
    │
    ├──► Service Key (Nostr keypair, generated locally)
    │    └─► Sends encrypted DM reminders
    │
    ├──► Descriptor Manager
    │    └─► Combines owner xpub + heir xpubs + timelock → inheritance address
    │
    └──► [Optional] Encrypted nsec
         └─► Shamir-split for identity inheritance
```

### What's Hot vs Cold

| Asset | Location | Risk |
|-------|----------|------|
| Owner Bitcoin seed | Hardware wallet (cold) | Never touches NoString |
| Owner xpub | NoString (watch-only) | Public key — no spend risk |
| Heir xpubs | NoString | Public keys — no risk |
| Service key (Nostr) | NoString (encrypted) | Notification bot only |
| Owner nsec (optional) | Shamir-split, encrypted | Only in memory during split, then destroyed |
| Descriptor backup | Downloaded file | Recovery lifeline |

---

## Component Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         NoString App                            │
│                         (Tauri Shell)                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌───────────────────┐        ┌───────────────────┐           │
│   │  WATCH-ONLY       │        │  INHERITANCE      │           │
│   │  WALLET           │        │  ENGINE           │           │
│   │                   │        │                   │           │
│   │ • Import xpub     │        │ • Policy builder  │           │
│   │ • UTXO monitoring │        │ • Descriptor gen  │           │
│   │ • PSBT creation   │        │ • Heir management │           │
│   │ • Balance display │        │ • Timelock config │           │
│   │                   │        │                   │           │
│   │ [nostring-electrum│        │ [nostring-inherit]│           │
│   └────────┬──────────┘        └────────┬──────────┘           │
│            │                            │                       │
│   ┌────────┴────────────────────────────┴────────┐             │
│   │              CORE SERVICES                    │             │
│   │                                               │             │
│   │  ┌─────────────┐  ┌─────────────┐            │             │
│   │  │ Key Manager │  │   Shamir    │            │             │
│   │  │             │  │             │            │             │
│   │  │ • xpub mgmt │  │ • SLIP-39   │            │             │
│   │  │ • Svc key   │  │ • Codex32   │            │             │
│   │  │ • Encrypt   │  │ • nsec split│            │             │
│   │  └─────────────┘  └─────────────┘            │             │
│   │                                               │             │
│   │  ┌─────────────┐  ┌─────────────┐            │             │
│   │  │  Notify     │  │  Descriptor │            │             │
│   │  │  Service    │  │  Backup     │            │             │
│   │  │             │  │             │            │             │
│   │  │ • Nostr DM  │  │ • Export    │            │             │
│   │  │ • Email     │  │ • Recovery  │            │             │
│   │  │ • Svc key   │  │ • Locked    │            │             │
│   │  │             │  │   shares    │            │             │
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
    │(DM notify│       │(SMTP     │       │(Electrum │
    │ + relay  │       │ notify)  │       │ server)  │
    │ storage) │       │          │       │          │
    └──────────┘       └──────────┘       └──────────┘
```

---

## Inheritance Flow

### Setup

```
1. Owner imports xpub from hardware wallet (watch-only)
2. Owner adds heir(s) with their xpubs
3. Configure timelock per heir (6mo, 12mo, 18mo)
4. NoString creates miniscript descriptor:
   - owner key can spend anytime
   - heir key can spend after N blocks
5. Owner funds the inheritance address
6. [Optional] Owner enters nsec for Nostr identity inheritance
   → NoString Shamir-splits nsec
   → Pre-distributes 1 share per heir
   → Locks remaining shares in descriptor backup
7. NoString generates service key, owner follows its npub
8. Owner downloads descriptor backup
```

### Active Use

```
1. NoString monitors inheritance UTXOs via Electrum
2. Service key sends Nostr DM reminders at 30/7/1 day thresholds
3. Owner clicks "Check In":
   a. NoString generates PSBT (spend → new inheritance address)
   b. Owner signs with hardware wallet (QR scan or paste)
   c. NoString broadcasts signed tx
   d. Timelock resets (new UTXO, fresh countdown)
   e. Prompt to download updated descriptor backup
```

### Bitcoin Inheritance

```
1. Owner stops checking in (death, incapacity)
2. Timelock expires (e.g., 26,280 blocks ≈ 6 months)
3. Heir claims Bitcoin using THEIR OWN wallet + key
4. No access to owner's seed needed
5. Enforced by Bitcoin consensus — trustless
```

### Nostr Identity Inheritance

```
1. Bitcoin inheritance triggers (timelock expired)
2. Heirs obtain descriptor backup (safe deposit box, lawyer, etc.)
3. Descriptor backup contains locked Shamir shares
4. Each heir combines:
   - Their pre-distributed share (received during setup)
   - Locked shares from descriptor backup
5. Threshold met → nsec recovered
6. Heir imports nsec into Nostr client
7. Heir controls deceased's Nostr identity
```

---

## Nostr Identity Inheritance — Shamir Threshold

For N heirs, the split is:
- **Threshold:** N + 1
- **Total shares:** 2N + 1
- **Pre-distributed:** N (1 per heir)
- **Locked in inheritance:** N + 1

This ensures:
- All heirs colluding have N shares but need N+1 → **blocked**
- After inheritance: any heir has 1 + (N+1) = N+2 → **exceeds threshold**
- If an heir loses their share: locked shares alone = N+1 → **still meets threshold**

See [NOSTR_INHERITANCE.md](NOSTR_INHERITANCE.md) for full spec.

---

## Notification Architecture

NoString generates a **service key** (random Nostr keypair) for notifications:

```
Service Key (generated in NoString)
    │
    ├──► Sends NIP-44 encrypted DMs to owner
    │    • 30 days remaining — gentle reminder
    │    • 7 days remaining — warning
    │    • 1 day remaining — urgent
    │    • 0 days — critical (timelock expired)
    │
    └──► Also sends via SMTP email (configurable)

Owner follows service key npub in their Nostr client.
Service key is NOT the owner's identity.
```

---

## Descriptor Backup

The descriptor backup file is the recovery lifeline. It contains:

```
# NoString Descriptor Backup
descriptor: wsh(or_d(pk([owner]xpub.../0/*), and_v(v:pk([heir]xpub.../0/*), older(26280))))
network: bitcoin
timelock_blocks: 26280
address: bc1q...
heirs:
  - label: Spouse
    xpub: xpub6DEF...
    timelock_months: 6
locked_shares: [encrypted Codex32 shares for nsec inheritance]
recovery_instructions: Import descriptor into Liana or Electrum
```

**Download prompts:**
- After initial setup
- After every check-in (descriptor changes)
- Always available in Settings tab

---

## Security Model

### Trust Assumptions

| Component | Trust Level |
|-----------|-------------|
| Hardware wallet | Trusted — holds Bitcoin keys |
| NoString app | Semi-trusted — holds no Bitcoin keys, may hold encrypted nsec temporarily |
| Nostr relays | Untrusted — can't read encrypted DMs |
| Bitcoin network | Trusted — enforces timelocks |
| Electrum server | Untrusted — provides block data, can't steal funds |
| Heirs | Trusted with shares, not with threshold |
| Descriptor backup | Critical — must be stored securely |

### Attack Vectors

| Attack | Mitigation |
|--------|-----------|
| Stolen device | No Bitcoin keys on device. Service key is encrypted. |
| Heir collusion (nsec) | N shares < N+1 threshold. Blocked until inheritance. |
| Lost descriptor backup | Owner can regenerate from NoString. Multiple copies recommended. |
| Compromised Electrum server | Can see UTXOs but can't spend. Watch-only. |
| Service key compromised | Attacker can send fake DMs. No fund risk. Owner can regenerate. |

---

## Crate Structure

```
nostring/
├── crates/
│   ├── nostring-core      # xpub management, encryption, service key
│   ├── nostring-inherit   # Miniscript policies, descriptor builder
│   ├── nostring-shamir    # SLIP-39, Codex32, nsec splitting
│   ├── nostring-electrum  # Electrum protocol, UTXO monitoring
│   ├── nostring-notify    # Service key DMs, SMTP email
│   ├── nostring-watch     # UTXO monitoring service
│   └── nostring-email     # Future: encrypted email
├── tauri-app/             # Desktop application
└── docs/                  # Documentation
```

---

*Last updated: 2026-02-02 — Revised for watch-only + service key + Shamir nsec architecture*
