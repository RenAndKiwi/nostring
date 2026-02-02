# nostr-mail Analysis

**Objective:** Identify core components to port to NoString.

---

## Repository Structure

```
nostr-mail/
├── tauri-app/
│   ├── backend/src/    ← Core Rust modules
│   │   ├── crypto.rs   ← Key/encryption operations
│   │   ├── email.rs    ← SMTP/IMAP handling
│   │   ├── nostr.rs    ← Nostr protocol operations
│   │   ├── database.rs ← SQLite storage
│   │   ├── state.rs    ← Application state
│   │   ├── storage.rs  ← Persistent storage
│   │   ├── types.rs    ← Shared types
│   │   └── lib.rs      ← Tauri commands (5000+ lines!)
│   └── frontend/       ← Vanilla JS UI
└── mock-relay/         ← Test relay implementation
```

---

## Core Modules to Port

### 1. crypto.rs (ESSENTIAL)
Already reviewed. Contains:
- `generate_keypair()` — Uses nostr-sdk Keys::generate()
- `encrypt_message()` — NIP-44 (default) or NIP-04 (legacy)
- `decrypt_message()` — NIP-44 decryption
- `sign_data()` — Schnorr signatures for email signing
- `verify_signature()` — Schnorr verification
- `encrypt_setting_value()` / `decrypt_setting_value()` — AES-256-GCM for local storage

**Port Status:** We can use this almost directly, but our nostring-core already handles key derivation from BIP-39.

### 2. email.rs (ESSENTIAL)
Contains:
- SMTP sending via lettre crate
- IMAP fetching via imap crate
- Custom headers: `X-Nostr-Pubkey`, `X-Nostr-Sig`
- Email signature verification
- Attachment handling (hybrid encryption)
- Platform-specific TLS (native-tls for desktop, rustls for Android)

**Key Pattern:** Emails are signed with sender's Nostr key, so recipients can verify authenticity.

### 3. nostr.rs (ESSENTIAL)
Contains:
- Relay connection management
- Profile fetching (kind 0 events)
- Direct message sending/receiving (kind 4 events)
- Contact list fetching (kind 3 events)
- Following list sync

**Key Insight:** Uses nostr-sdk client for relay management and event handling.

### 4. database.rs (USEFUL)
SQLite schema for:
- Contacts (pubkey, metadata, last_updated)
- Emails (message_id, from, to, subject, body, encrypted)
- Direct messages (event_id, content, timestamp)
- Relays (url, is_active)
- Settings (per-user encrypted settings)

**Port Decision:** Adapt schema, don't copy wholesale.

### 5. types.rs (REFERENCE)
Type definitions:
- `KeyPair`, `EmailConfig`, `EmailMessage`
- `Profile`, `NostrEvent`
- `MessageContent::Plaintext` | `MessageContent::Encrypted`

---

## What We DON'T Need

1. **lib.rs** — 5000+ lines of Tauri commands, very specific to their UI
2. **mock-relay/** — Testing infrastructure
3. **Frontend** — We'll build our own

---

## Encryption Flow (Critical to Understand)

### Sending Email:
1. Compose plaintext email
2. Encrypt body with NIP-44 (sender_privkey + recipient_pubkey → shared secret)
3. Sign encrypted body with sender's Schnorr key
4. Add `X-Nostr-Pubkey` header (sender's pubkey)
5. Add `X-Nostr-Sig` header (signature of body)
6. Send via SMTP

### Receiving Email:
1. Fetch via IMAP
2. Extract `X-Nostr-Pubkey` header (sender's pubkey)
3. Verify `X-Nostr-Sig` against body
4. Decrypt body with NIP-44 (recipient_privkey + sender_pubkey → same shared secret)
5. Display plaintext

### Key Discovery:
- Before sending, lookup recipient's Nostr profile (kind 0) on relays
- Profile contains pubkey + optional email field
- If email field present, recipient "accepts" encrypted email

---

## Dependencies Used

| Crate | Purpose | Version in nostr-mail |
|-------|---------|----------------------|
| nostr-sdk | Nostr protocol | 0.39+ |
| lettre | SMTP client | 0.11+ |
| imap | IMAP client | 3.0.0-alpha |
| aes-gcm | Local encryption | 0.10 |
| sha2 | Hashing | - |
| base64 | Encoding | - |

All already in our workspace!

---

## Port Strategy

### Phase 1: Copy crypto patterns
- We already have key derivation
- Add encrypt/decrypt using their pattern
- Add sign/verify for email signatures

### Phase 2: Port email module
- SMTP sending with custom headers
- IMAP fetching with decryption
- Signature verification on receive

### Phase 3: Port Nostr operations
- Profile lookup
- Contact discovery
- (We won't need DMs — we have email)

---

## Security Observations

**Good:**
- NIP-44 is the recommended modern encryption
- Signatures prove email authenticity
- Local settings encrypted with user's key

**Concerns:**
- Private key stored in browser localStorage (we'll use encrypted seed instead)
- Some legacy NIP-04 support (we can skip this)

---

*Analysis completed: 2026-02-02 00:30 CST*
