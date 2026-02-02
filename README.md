# NoString

**Sovereign Bitcoin inheritance. No trusted third parties.**

NoString is a Bitcoin inheritance system that uses timelocked transactions to pass Bitcoin to your heirs without lawyers, courts, or custodians. Check in periodically to prove you're still in control. If you stop checking in, your heirs can claim.

---

## Features

- **Single Seed** — One BIP-39 mnemonic for Bitcoin and Nostr identity
- **Timelock Inheritance** — Miniscript policies with configurable check-in periods
- **Multi-Heir Support** — Cascade timelocks (spouse → children → executor)
- **Shamir Backup** — Split your seed with SLIP-39 or Codex32
- **Air-Gap Signing** — QR-based PSBT flow for hardware wallets
- **Notifications** — Email and Nostr DM reminders before timelock expiry
- **Desktop App** — Cross-platform Tauri application

---

## Quick Start

### Prerequisites

- Rust 1.75+
- Node.js 20+ (for Tauri frontend)

### Build

```bash
# Clone (replace with actual repo URL when published)
git clone https://github.com/RenAndKiwi/nostring
cd nostring

# Build all crates
cargo build --release

# Run tests
cargo test

# Build Tauri app (requires additional setup)
cd tauri-app
npm install
npm run tauri build
```

### Development

```bash
# Run tests with network access
cargo test -- --ignored

# Run specific crate tests
cargo test --package nostring-core
cargo test --package nostring-inherit
cargo test --package nostring-watch
```

---

## Architecture

```
nostring/
├── crates/
│   ├── nostring-core      # Seed generation, encryption, key derivation
│   ├── nostring-inherit   # Miniscript policies, check-in transactions
│   ├── nostring-shamir    # SLIP-39 and Codex32 secret sharing
│   ├── nostring-electrum  # Bitcoin network via Electrum protocol
│   ├── nostring-notify    # Email and Nostr DM notifications
│   ├── nostring-watch     # UTXO monitoring service
│   └── nostring-email     # IMAP email (placeholder)
├── tauri-app/             # Desktop application
└── docs/                  # Documentation
```

### Key Dependencies

- [bitcoin](https://crates.io/crates/bitcoin) — Bitcoin primitives
- [miniscript](https://crates.io/crates/miniscript) — Policy compilation
- [electrum-client](https://crates.io/crates/electrum-client) — Electrum protocol
- [nostr-sdk](https://crates.io/crates/nostr-sdk) — Nostr protocol
- [tauri](https://tauri.app) — Desktop application framework

---

## How It Works

### 1. Setup

1. Generate or import a BIP-39 seed
2. Add heir(s) by importing their xpub
3. Configure timelock (e.g., 6 months)
4. Fund the inheritance address

### 2. Check-In

Periodically "check in" by signing a transaction that resets the timelock:

```
Owner can spend immediately
    OR
Heir can spend after 26,280 blocks (~6 months)
```

### 3. Inheritance

If you stop checking in:
1. Timelock expires
2. Heir uses their key to claim
3. No intermediaries required

---

## Documentation

| Document | Purpose |
|----------|---------|
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Technical design |
| [SECURITY.md](docs/SECURITY.md) | Security model |
| [HEIR_GUIDE.md](docs/HEIR_GUIDE.md) | Setup guide for heirs |
| [CLAIM_GUIDE.md](docs/CLAIM_GUIDE.md) | How heirs claim |
| [SELF_HOSTING.md](docs/SELF_HOSTING.md) | Deployment guide |
| [OPERATIONS.md](docs/OPERATIONS.md) | Operational runbook |
| [ROADMAP.md](docs/ROADMAP.md) | Project status |

---

## Security

- Seeds are encrypted at rest (AES-256-GCM + Argon2)
- No private keys transmitted over network
- Air-gapped signing via QR codes
- TLS required for Electrum connections
- No trusted third parties

See [SECURITY.md](docs/SECURITY.md) for the full security model.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

### Running Tests

```bash
# Unit tests (no network)
cargo test

# Integration tests (requires network)
cargo test -- --ignored

# All tests
cargo test && cargo test -- --ignored
```

---

## License

BSD-3-Clause. See [LICENSE](LICENSE).

---

## Acknowledgments

- [Liana](https://wizardsardine.com/liana/) — Miniscript inheritance inspiration
- [SLIP-39](https://github.com/satoshilabs/slips/blob/master/slip-0039.md) — Shamir secret sharing
- [Codex32](https://github.com/BlockstreamResearch/codex32) — BIP-93 implementation
- [Bitcoin Butlers](https://bitcoinbutlers.com) — Sovereign Bitcoin education

---

*NoString: Your keys, your Bitcoin, your inheritance.*
