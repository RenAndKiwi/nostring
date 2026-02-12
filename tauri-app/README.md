# NoString Desktop App

Taproot inheritance vault manager with MuSig2 collaborative custody.

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) (v18+)
- [Tauri CLI](https://tauri.app/start/): `cargo install tauri-cli`

## Quick Start

```bash
cd tauri-app
npm install
cargo tauri dev
```

The app opens at 1024x768. First run shows the wallet onboarding screen.

## Testnet Setup

1. **Create or import a wallet** (Onboarding screen)
2. **Go to Settings** (🛠️ tab) → select **Testnet** → Save
3. **Test Connection** → should show testnet block height
4. **Register a co-signer** (Setup tab):
   - You need a co-signer's public key (33-byte compressed hex) and chain code (32-byte hex)
   - For testing with a friend: they run their own instance and share their pubkey + chain code
5. **Add heirs** (Heirs tab):
   - Heir's xpub (extended public key) and optional npub (Nostr)
6. **Create vault** (Vault tab):
   - Set timelock (3-24 months, use 3 for testing)
   - Creates a Taproot P2TR address
7. **Fund the vault**: Send testnet BTC to the vault address
   - Faucet: https://signetfaucet.com/ (signet) or https://coinfaucet.eu/en/btc-testnet/ (testnet)
8. **Check Dashboard** (Status tab) → should show balance after confirmation
9. **Check-in** (Check-in tab):
   - Start signing session → sends nonce request to co-signer
   - Co-signer responds with their nonces
   - Submit nonces → get signing challenge
   - Co-signer responds with partial signatures
   - Finalize → broadcasts check-in transaction
10. **Deliver backup** (Deliver tab):
    - Export vault backup JSON (includes recovery scripts + control blocks)
    - Or deliver via NIP-17 encrypted DM to heir's npub

## Screens

| Tab | Purpose |
|-----|---------|
| ⚙️ Setup | Register co-signer (pubkey + chain code) |
| 👥 Heirs | Add/remove heirs (xpub + npub) |
| 🔐 Vault | Create Taproot vault with timelock |
| 📊 Status | Dashboard: balance, heartbeat, timelock progress |
| ✅ Check-in | MuSig2 signing ceremony to reset timelock |
| 📨 Deliver | Export/deliver vault backup to heirs |
| 🛠️ Settings | Network, Electrum server, connection test |

## Architecture

- **Frontend**: Svelte 5 (runes)
- **Backend**: Rust via Tauri commands
- **Crypto**: nostring-ccd (MuSig2), nostring-inherit (Taproot vaults, miniscript)
- **Network**: Electrum protocol for blockchain queries
- **Notifications**: NIP-17 encrypted DMs via Nostr relays

## Build for Distribution

```bash
cd tauri-app
cargo tauri build
```

Output in `src-tauri/target/release/bundle/`.
