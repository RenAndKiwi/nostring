# NoString

**Sovereign communications for life — and beyond.**

Encrypted email with Nostr identity and Bitcoin-secured inheritance.

---

## What is NoString?

NoString is an encrypted email client that uses your Nostr keypair for identity and encryption. Unlike traditional PGP, key discovery is automatic — if someone has a Nostr profile, you can send them encrypted email.

What makes NoString unique: **inheritance is a first-class feature.** Using Bitcoin timelocks (miniscript), you can ensure your heirs gain access to your communications if you become incapacitated.

## Core Features

- **Encrypted Email** — NIP-44 encryption, works with any SMTP/IMAP provider
- **Nostr Identity** — One keypair for identity, encryption, and contact discovery
- **Bitcoin Inheritance** — Timelock-based deadman switch, trustless and on-chain
- **BIP-39 + Codex32** — Standard seed backup, with optional physical Shamir splits
- **Self-Hosted** — Your infrastructure, your rules

## Philosophy

1. **One seed rules all** — BIP-39 seed derives both Nostr keys (NIP-06) and Bitcoin keys (BIP-84)
2. **No phone numbers** — Your pubkey is your identity
3. **Email is archival** — Chat is ephemeral, email persists
4. **Death is a feature** — Inheritance should be planned, not an afterthought
5. **Trustless timelocks** — Bitcoin secures access, not corporate promises

## Status

🚧 **Pre-alpha** — Planning and architecture phase

See [ROADMAP.md](docs/ROADMAP.md) for development plan.

## Built On

- [nostr-mail](https://github.com/asherp/nostr-mail) — Nostr-encrypted email client (Tauri/Rust)
- [Liana](https://github.com/wizardsardine/liana) — Bitcoin wallet with miniscript timelocks (Rust)

## License

BSD 3-Clause (following upstream projects)

---

*Your keys, your messages, your legacy.*
