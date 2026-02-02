# Phase 1: Unified Seed

**Goal:** BIP-39 seed derives both Nostr and Bitcoin keys correctly, with proper test vectors and encrypted storage.

**Started:** 2026-02-02 01:10 CST

---

## Feature 1.1: BIP-39 Mnemonic Generation

### Research & Plan

**Objective:** Generate and validate BIP-39 mnemonics.

**What we have:**
- Basic `generate_mnemonic()` already works
- Uses bip39 crate with rand feature

**What we need to add:**
- Word count options (12, 15, 18, 21, 24)
- Language support (English primary)
- Checksum validation on import
- Test against known BIP-39 test vectors

**Approach:**
1. Refactor generate_mnemonic to accept word count
2. Add import with validation
3. Add test vectors from BIP-39 spec

### Security Review

- Entropy source: bip39 crate uses system RNG via rand
- Mnemonic never logged, never stored in plaintext
- Passphrase handling must be secure

### Implementation

*See work log below*

### Testing

- [ ] Generate 12-word mnemonic
- [ ] Generate 24-word mnemonic  
- [ ] Parse valid mnemonic
- [ ] Reject invalid mnemonic (bad checksum)
- [ ] Test vectors from BIP-39 spec

---

## Feature 1.2: Seed Derivation

### Research & Plan

**Objective:** Derive 64-byte seed from mnemonic with optional passphrase.

**BIP-39 spec:**
- PBKDF2-HMAC-SHA512
- Salt: "mnemonic" + passphrase
- Iterations: 2048
- Output: 512 bits (64 bytes)

**What we have:**
- `derive_seed()` works
- bip39 crate handles PBKDF2 internally

**What we need:**
- Test against BIP-39 test vectors
- Verify passphrase handling

### Implementation

*See work log below*

### Testing

- [ ] Test vector: mnemonic → seed (no passphrase)
- [ ] Test vector: mnemonic → seed (with passphrase)

---

## Feature 1.3: NIP-06 Nostr Key Derivation

### Research & Plan

**Objective:** Derive Nostr keys from seed via NIP-06.

**NIP-06 spec:**
- Path: m/44'/1237'/0'/0/0
- Uses BIP-32/BIP-44 derivation
- 1237 is the registered coin type for Nostr

**What we have:**
- `derive_nostr_keys()` works
- Uses bitcoin crate for BIP-32

**What we need:**
- Test against known NIP-06 vectors
- Verify bech32 encoding (npub, nsec)

### Implementation

*See work log below*

### Testing

- [ ] Known mnemonic → expected npub
- [ ] Verify nsec encoding
- [ ] Keys are deterministic

---

## Feature 1.4: BIP-84 Bitcoin Key Derivation

### Research & Plan

**Objective:** Derive Bitcoin keys for timelocks via BIP-84.

**BIP-84 spec:**
- Path: m/84'/0'/0' (mainnet) or m/84'/1'/0' (testnet)
- Native SegWit (P2WPKH)
- Uses BIP-32 derivation

**What we have:**
- `derive_bitcoin_master()` works
- Returns xpriv at m/84'/0'/0'

**What we need:**
- Derive individual addresses
- Test against known BIP-84 vectors
- Network awareness (mainnet vs testnet)

### Implementation

*See work log below*

### Testing

- [ ] Known seed → expected xpub
- [ ] Derive receive address (m/84'/0'/0'/0/0)
- [ ] Derive change address (m/84'/0'/0'/1/0)

---

## Feature 1.5: Encrypted Seed Storage

### Research & Plan

**Objective:** Store seed encrypted at rest, decrypt with user password.

**Approach (from nostr-mail):**
1. User provides password
2. Derive encryption key via Argon2id
3. Encrypt seed with AES-256-GCM
4. Store: nonce || ciphertext

**Parameters:**
- Argon2id: m=64MB, t=3, p=4 (OWASP recommendations)
- AES-256-GCM: 256-bit key, 96-bit nonce, 128-bit tag

### Security Review

- Password never stored
- Argon2id is memory-hard (resists GPU attacks)
- Nonce is random per encryption
- Tag provides authentication

### Implementation

*See work log below*

### Testing

- [ ] Encrypt seed with password
- [ ] Decrypt with correct password
- [ ] Reject wrong password
- [ ] Ciphertext changes with each encryption (random nonce)

---

## Work Log

### 2026-02-02 01:10 CST — Starting Phase 1

Beginning with Feature 1.1: BIP-39 Mnemonic Generation.

### 2026-02-02 01:30 CST — Feature 1.1-1.2 Complete

- Refactored `generate_mnemonic()` to accept WordCount enum
- Added BIP-39 test vectors from Trezor reference implementation
- All vectors pass with "TREZOR" passphrase

### 2026-02-02 01:45 CST — Feature 1.3 Complete

- Added official NIP-06 test vector
- Verifies: hex privkey, nsec, hex pubkey, npub
- Tests for passphrase affecting key derivation

### 2026-02-02 02:00 CST — Feature 1.4 Complete

- Added `derive_bitcoin_address()` for P2WPKH addresses
- Verified against known BIP-84 test vector
- First address matches: bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu

### 2026-02-02 02:15 CST — Feature 1.5 Complete

- Implemented Argon2id key derivation (64MB, 3 iterations, 4 threads)
- Implemented AES-256-GCM encryption
- Created `EncryptedSeed` type with serialization
- All security tests pass (wrong password, tampering, etc.)

---

## Phase 1 Reflection

**What we accomplished:**
- Complete key derivation from BIP-39 seed
- Both Nostr (NIP-06) and Bitcoin (BIP-84) paths working
- Secure encrypted storage with industry-standard cryptography
- 23 unit tests + 1 doc test all passing

**Test coverage:**
- BIP-39: 4 test vectors + validation tests
- NIP-06: 1 official test vector + derivation tests
- BIP-84: Address derivation tests, receive/change separation
- Crypto: 6 comprehensive security tests

**What worked well:**
- Using official test vectors ensured correctness
- The bitcoin crate handles BIP-32 derivation cleanly
- nostr-sdk provides good bech32 encoding

**Challenges encountered:**
- bitcoin 0.32 changed the P2WPKH API (easy fix)
- Had to verify exact test vector values carefully
- Argon2 requires significant memory for tests (~6 seconds)

**Code quality:**
- Every function has documentation
- No shortcuts taken — all tests use real cryptography
- Security-critical code has comprehensive tests

**Ready for Phase 2:** Yes — we have a solid unified seed implementation.

---

*Phase 1 completed: 2026-02-02 02:15 CST*
