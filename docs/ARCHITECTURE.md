# NoString Architecture

## Overview

NoString is a sovereign Bitcoin inheritance tool with optional Nostr identity inheritance. It combines:

1. **Watch-only Bitcoin wallet** — monitors inheritance UTXOs, generates check-in PSBTs
2. **Miniscript policy engine** — creates timelocked inheritance descriptors
3. **Nostr service key** — sends check-in reminder DMs
4. **Shamir secret sharing** — splits nsec for identity inheritance
5. **SQLite persistence** — all state survives app restarts

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
    ├──► SQLite Database (app data dir)
    │    ├── config: wallet state, settings, service key
    │    ├── heirs: heir registry
    │    └── checkin_log: check-in history with txids
    │
    ├──► Service Key (Nostr keypair, generated locally)
    │    └─► Sends NIP-04 encrypted DM reminders
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
| Owner xpub | NoString SQLite (watch-only) | Public key — no spend risk |
| Heir xpubs | NoString SQLite | Public keys — no risk |
| Service key (Nostr) | NoString SQLite | Notification bot only |
| Owner nsec (optional) | Shamir-split, in memory only during split | Zeroed after split completes |
| Descriptor backup | Downloaded file | Recovery lifeline |
| Check-in history | NoString SQLite | Audit trail |

---

## Persistence Layer

All durable state is stored in SQLite (`nostring.db` in Tauri's app data directory):

```
~/.local/share/com.nostring.app/nostring.db  (Linux)
~/Library/Application Support/com.nostring.app/nostring.db  (macOS)
%APPDATA%\com.nostring.app\nostring.db  (Windows)
```

### Tables

| Table | Purpose | Schema |
|-------|---------|--------|
| `config` | Key-value store | `key TEXT PK, value TEXT` |
| `heirs` | Heir registry | `fingerprint TEXT PK, label, xpub, derivation_path` |
| `checkin_log` | Check-in history | `id INTEGER PK, timestamp INTEGER, txid TEXT` |

### Config Keys

| Key | Example | Description |
|-----|---------|-------------|
| `owner_xpub` | `xpub6ABC...` | Watch-only wallet xpub |
| `watch_only` | `true` | Whether in watch-only mode |
| `encrypted_seed` | `(hex)` | AES-GCM encrypted seed (advanced mode only) |
| `service_key` | `(hex)` | Nostr service key secret |
| `service_npub` | `npub1...` | Service key public identity |
| `electrum_url` | `ssl://...` | Electrum server |
| `network` | `bitcoin` | Bitcoin network |
| `notify_owner_npub` | `npub1...` | Owner's npub for DM notifications |
| `notify_email_address` | `user@...` | Email notification recipient |
| `inheritance_descriptor` | `wsh(...)` | Current inheritance descriptor |
| `inheritance_timelock` | `26280` | Timelock in blocks |

### Write-Through Pattern

Every mutation to `AppState` writes through to SQLite immediately:

```rust
// In-memory + SQLite in one call
state.set_owner_xpub(&xpub);     // updates Mutex + SQLite
state.persist_heir(&heir);         // updates registry + SQLite
state.log_checkin(&txid);          // inserts into checkin_log
```

On startup, `AppState::from_db_path()` loads everything from SQLite into memory. Watch-only wallets auto-unlock (no password needed since there's no private key).

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
│   │  │  Notify     │  │  Persistence│            │             │
│   │  │  Service    │  │  (SQLite)   │            │             │
│   │  │             │  │             │            │             │
│   │  │ • Nostr DM  │  │ • Config    │            │             │
│   │  │ • Email     │  │ • Heirs     │            │             │
│   │  │ • Svc key   │  │ • Checkins  │            │             │
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
   e. Check-in logged to SQLite with timestamp and txid
   f. Prompt to download updated descriptor backup
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
Service Key (generated in NoString, persisted in SQLite)
    │
    ├──► Sends NIP-04 encrypted DMs to owner's npub
    │    • 30 days remaining — gentle reminder
    │    • 7 days remaining — warning
    │    • 1 day remaining — urgent
    │    • 0 days — critical (timelock expired)
    │
    └──► Also sends via SMTP email (configurable)

Owner follows service key npub in their Nostr client.
Service key is NOT the owner's identity.

Notifications auto-check on every status refresh.
Owner configures their npub + optional email in Settings.
"Send Test DM" button verifies the pipeline works.
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
| SQLite database | Local only — encrypted seed stored as AES-GCM ciphertext |
| Nostr relays | Untrusted — can't read encrypted DMs |
| Bitcoin network | Trusted — enforces timelocks |
| Electrum server | Untrusted — provides block data, can't steal funds |
| Heirs | Trusted with shares, not with threshold |
| Descriptor backup | Critical — must be stored securely |

### Attack Vectors

| Attack | Mitigation |
|--------|-----------|
| Stolen device | No Bitcoin keys on device. Service key is low-value. |
| Heir collusion (nsec) | N shares < N+1 threshold. Blocked until inheritance. |
| Lost descriptor backup | Owner can regenerate from NoString. Multiple copies recommended. |
| Compromised Electrum server | Can see UTXOs but can't spend. Watch-only. |
| Service key compromised | Attacker can send fake DMs. No fund risk. Owner can regenerate. |
| SQLite database read | Seed is AES-GCM encrypted. xpub/heirs are public keys. |

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
│   ├── src-tauri/src/
│   │   ├── main.rs        # Tauri setup, command registration
│   │   ├── commands.rs    # All Tauri commands (bridge to Rust)
│   │   ├── state.rs       # AppState with SQLite-backed persistence
│   │   └── db.rs          # SQLite schema, queries, migrations
│   └── frontend/
│       ├── index.html     # App shell
│       └── js/app.js      # Frontend logic (vanilla JS)
├── tests/e2e/             # Integration test suite
└── docs/                  # Documentation
```

---

*Last updated: 2026-02-03 — Added SQLite persistence, notification wiring, e2e tests*
