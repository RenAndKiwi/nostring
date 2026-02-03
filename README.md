<div align="center">

# NoString

**Bitcoin inheritance without trusted third parties.**

[![CI](https://github.com/RenAndKiwi/nostring/actions/workflows/ci.yml/badge.svg)](https://github.com/RenAndKiwi/nostring/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-BSD--3--Clause-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-115%20passing-brightgreen.svg)](#)

*Your heirs inherit your Bitcoin when you stop checking in. No custodians. No monthly fees. Just math.*

[Features](#features) • [Quick Start](#quick-start) • [How It Works](#how-it-works) • [Documentation](#documentation)

</div>

---

## The Problem

You've taken custody of your Bitcoin. But what happens to it when you die?

| Traditional Option | The Problem |
|-------------------|-------------|
| **Custodians** | They can rug you, get hacked, or go bankrupt |
| **Paper instructions** | Heirs lose access, get phished, or can't execute |
| **Lawyers & wills** | Probate courts, delays, fees—they don't understand Bitcoin |

**NoString solves this with timelocks.** Your heirs can only claim after you stop checking in. No company, no custodian, no permission needed.

---

## Features

- **🔐 Single Seed** — One BIP-39 mnemonic for Bitcoin and Nostr identity
- **⏱️ Timelock Inheritance** — Miniscript policies with configurable check-in periods
- **👥 Multi-Heir Cascade** — Spouse at 6 months → Children at 12 months → Executor at 18 months
- **🔑 Shamir Backup** — Split your seed with SLIP-39 or Codex32 (2-of-3, 3-of-5, etc.)
- **📱 Air-Gap Signing** — QR-based PSBT flow for Electrum or hardware wallets
- **🔔 Notifications** — Email and Nostr DM reminders before timelock expiry
- **💻 Desktop App** — Cross-platform Tauri application (macOS, Windows, Linux)

---

## Screenshots

<div align="center">
<img src="docs/assets/screenshot-dashboard.png" alt="Dashboard" width="600">
<p><em>Dashboard showing policy status, check-in timeline, and heir cascade</em></p>
</div>

---

## Quick Start

### Run the App (Fastest)

```bash
# Install Rust (if you don't have it)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone and run
git clone https://github.com/RenAndKiwi/nostring
cd nostring
cargo tauri dev
```

First run takes a few minutes to compile. The app window opens automatically.

### Prerequisites

- **Rust (latest stable)** — `rustup update stable`
- **Tauri deps** — see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS

### Build from Source

```bash
git clone https://github.com/RenAndKiwi/nostring
cd nostring

# Run tests
cargo test

# Build release binary
cargo build --release

# Build desktop app (creates installer)
cargo tauri build
```

### Download Binary

Coming soon — see [Releases](https://github.com/RenAndKiwi/nostring/releases).

---

## How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│   Owner can spend immediately                                   │
│                         OR                                      │
│   Heir can spend after 26,280 blocks (~6 months of inactivity) │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 1. Setup
- Generate or import a BIP-39 seed
- Add heirs by importing their xpub
- Configure timelock periods
- Fund the inheritance address

### 2. Check-In
- Periodically sign a transaction to prove you're alive
- This resets the timelock countdown
- Miss enough check-ins and the clock starts ticking

### 3. Inheritance
- When the timelock expires, heirs can claim with their key
- No intermediaries, no permission, no court orders
- Just Bitcoin script doing its job

---

## Architecture

```
nostring/
├── crates/
│   ├── nostring-core      # Seed, encryption, key derivation
│   ├── nostring-inherit   # Miniscript policies, check-in builder
│   ├── nostring-shamir    # SLIP-39 and Codex32 secret sharing
│   ├── nostring-electrum  # Bitcoin network via Electrum
│   ├── nostring-notify    # Email and Nostr notifications
│   └── nostring-watch     # UTXO monitoring service
├── tauri-app/             # Desktop application
└── docs/                  # Documentation
```

### Dependencies

| Crate | Purpose |
|-------|---------|
| [bitcoin](https://crates.io/crates/bitcoin) | Bitcoin primitives |
| [miniscript](https://crates.io/crates/miniscript) | Policy → Script compilation |
| [electrum-client](https://crates.io/crates/electrum-client) | Electrum protocol |
| [nostr-sdk](https://crates.io/crates/nostr-sdk) | Nostr notifications |
| [tauri](https://tauri.app) | Desktop app framework |

---

## Documentation

| Document | Description |
|----------|-------------|
| [HEIR_GUIDE.md](docs/HEIR_GUIDE.md) | How heirs set up their wallet |
| [CLAIM_GUIDE.md](docs/CLAIM_GUIDE.md) | How heirs claim when the time comes |
| [SELF_HOSTING.md](docs/SELF_HOSTING.md) | Run your own infrastructure |
| [OPERATIONS.md](docs/OPERATIONS.md) | Operational runbook |
| [SECURITY_AUDIT.md](docs/SECURITY_AUDIT.md) | Pre-audit security review |

---

## Security Model

| Aspect | Approach |
|--------|----------|
| **At rest** | AES-256-GCM + Argon2id key derivation |
| **In transit** | No private keys ever transmitted |
| **Signing** | Air-gapped via QR codes |
| **Network** | TLS required for Electrum |
| **Trust** | Zero—verify the math yourself |

See [SECURITY_AUDIT.md](docs/SECURITY_AUDIT.md) for the full threat model.

---

## Contributing

We welcome contributions. See [CONTRIBUTING.md](CONTRIBUTING.md).

```bash
# Run all tests
cargo test

# Run with network tests
cargo test -- --ignored

# Check formatting
cargo fmt --check

# Lint
cargo clippy --workspace
```

---

## License

BSD-3-Clause. See [LICENSE](LICENSE).

---

## Acknowledgments

- [Liana](https://wizardsardine.com/liana/) — Miniscript inheritance pioneer
- [SLIP-39](https://github.com/satoshilabs/slips/blob/master/slip-0039.md) — Shamir secret sharing spec
- [Codex32](https://github.com/BlockstreamResearch/codex32) — Human-computable checksums

---

<div align="center">

**Built by [Bitcoin Butlers](https://bitcoinbutlers.com)**

*Helping you hold your own keys—literally and metaphorically.*

</div>
