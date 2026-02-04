# NoString Security Model

## Core Principles

1. **Zero trust for intermediaries** — Relays, email servers see only ciphertext
2. **Bitcoin as arbiter** — Timelocks enforced by consensus, not promises
3. **Defense in depth** — Multiple layers protect the seed
4. **Fail secure** — If something breaks, access is denied, not granted

---

## Threat Model

### Assets to Protect

| Asset | Impact if Compromised |
|-------|----------------------|
| BIP-39 seed | Total loss — all keys derived from it |
| Nostr private key | Identity theft, read all emails |
| Bitcoin private key | Theft of timelock funds |
| Email content | Privacy breach |
| Heir relationships | Social engineering vector |

### Adversary Capabilities

| Adversary | Capabilities |
|-----------|--------------|
| Passive network observer | See encrypted blobs, metadata |
| Compromised relay | Store/forward, can't decrypt |
| Compromised email server | Store/forward, can't decrypt |
| Stolen device (locked) | Physical access, no password |
| Stolen device (unlocked) | Full local access |
| Colluding heirs (< M) | Shamir shares, can't reconstruct |
| Colluding heirs (≥ M) | Can reconstruct seed |
| State actor | All of above + rubber hose |

---

## Mitigations

### Seed Protection

```
Seed (plaintext)
     │
     ▼
┌─────────────────┐
│ Argon2id KDF    │◄── User password
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ AES-256-GCM     │
│ Encrypted seed  │
└────────┬────────┘
         │
         ▼
    [Disk storage]
```

- **Argon2id** with high memory cost (resistant to GPU/ASIC attacks)
- Password never stored, only used to derive encryption key
- Seed decrypted to memory only when needed

### Key Derivation Isolation

- Nostr keys and Bitcoin keys derived from different BIP-32 paths
- Compromise of one doesn't directly expose the other
- Application enforces path separation

### Shamir Distribution

For paranoid users:

```
Seed ──► SLIP-39/Codex32 ──► 5 shares (3 required)
                               │
         ┌─────────┬─────────┬─┴───────┬─────────┐
         ▼         ▼         ▼         ▼         ▼
      Spouse    Child 1   Child 2   Lawyer   Safe box
      (local)   (city A)  (city B)  (office) (bank)
```

- Geographic distribution prevents single-point seizure
- Threshold prevents minority collusion
- No single heir can reconstruct alone

### Timelock Security

**Why Bitcoin timelocks are trustless:**

1. UTXO created with CSV (CheckSequenceVerify) condition
2. Condition is part of the transaction script, verified by all nodes
3. No server, company, or third party can override
4. Only the passage of blocks (time) unlocks

**Check-in transaction:**

```
Old UTXO (clock running)
     │
     ├── Owner spends (always allowed)
     │
     ▼
New UTXO (clock reset to 0)
```

- Each check-in costs ~200-500 sats in fees
- Creates on-chain proof of life
- Heir cannot spend until timeout expires

---

## Operational Security

### For Daily Use

- [ ] Strong, unique password for seed encryption
- [ ] Device encrypted at OS level
- [ ] Regular check-ins (don't let timer get close)
- [ ] Review heir list periodically

### For Seed Backup

- [ ] Generate seed on air-gapped device (ideal)
- [ ] Write down BIP-39 words, verify
- [ ] If using Shamir: generate shares, distribute immediately
- [ ] Never store seed digitally in plaintext
- [ ] Never email/message the seed

### For Heirs

- [ ] Heir knows they are an heir
- [ ] Heir has their Shamir share (if used)
- [ ] Heir has instructions on how to reconstruct
- [ ] Heir has xpub registered in policy (for Bitcoin timelock)

---

## Known Limitations

1. **Metadata leakage** — Email headers, Nostr event metadata visible
2. **Timing attacks** — Check-in frequency reveals activity patterns
3. **$5 wrench attack** — Physical coercion bypasses all crypto
4. **Heir coordination** — Heirs must actually cooperate to reconstruct
5. **Infrastructure continuity** — Self-hosted server needs to stay up

---

## Security Checklist (Pre-Release)

- [ ] Seed encryption uses Argon2id with appropriate parameters
- [ ] No seed/key material in logs
- [ ] NIP-44 encryption correctly implemented
- [ ] Miniscript policies validated
- [ ] Timelock arithmetic verified (no off-by-one)
- [ ] Share generation produces valid reconstructions
- [ ] Memory zeroization after sensitive operations
- [ ] Dependency audit (no malicious crates)

---

## Incident Response

### If device is stolen

1. Access from another device immediately
2. Check in to reset timelock (prevents heir trigger)
3. Consider rotating to new seed if device was unlocked

### If seed is compromised

1. Move all Bitcoin immediately
2. Notify all contacts of key rotation
3. Generate new seed, redistribute Shamir shares
4. Update Nostr profile with new pubkey

### If heir share is compromised

1. Reconstruct seed from remaining shares
2. Generate new share set
3. Distribute new shares (revoke old)

---

## Release Verification

All releases include SHA256SUMS.txt signed with our GPG key.

**Fingerprint:** `DBFD 98EB A90B BB2F FB60  9AD2 DB89 0E87 094F 72B6`

```bash
# Import the public key (included in repo as RELEASE_KEY.asc)
gpg --import RELEASE_KEY.asc

# Verify checksums signature
gpg --verify SHA256SUMS.txt.asc SHA256SUMS.txt

# Verify binary integrity
sha256sum --check SHA256SUMS.txt
```

---

*Last updated: 2026-02-04*
