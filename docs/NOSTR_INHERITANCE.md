# Nostr Identity Inheritance — Feature Spec

## Problem

Bitcoin inheritance has timelocks (trustless, enforced by consensus). Nostr doesn't. There's no Bitcoin script that says "this nsec activates after 6 months." We need a mechanism to pass down a Nostr identity that:

1. **Can't be accessed early** — heirs shouldn't be able to impersonate you while you're alive
2. **Becomes available after death/incapacity** — tied to the same trigger as Bitcoin inheritance
3. **Is resilient** — survives lost shares, missing heirs
4. **Requires no trusted third party**

## Solution: Shamir-Gated nsec Inheritance

Use Shamir secret sharing with a **split threshold** — heirs hold some shares upfront, but not enough to reconstruct. The remaining shares are locked behind the Bitcoin timelock. Only after Bitcoin inheritance triggers can heirs combine all shares and recover the nsec.

### The Math

| Heirs | Threshold | Total Shares | Pre-distributed (1/heir) | Locked in Inheritance | Safety |
|-------|-----------|-------------|--------------------------|----------------------|--------|
| 1 | 2-of-3 | 3 | 1 | 2 | Heir alone: ✗ (1<2). After inheritance: 1+2=3 ✓ |
| 2 | 3-of-5 | 5 | 2 (1 each) | 3 | All heirs collude: ✗ (2<3). After inheritance: 2+3=5 ✓ |
| 3 | 4-of-7 | 7 | 3 (1 each) | 4 | All heirs collude: ✗ (3<4). After inheritance: 3+4=7 ✓ |

**Formula:** For N heirs:
- Threshold = N + 1
- Total shares = 2N + 1
- Pre-distributed = N (one per heir)
- Locked = N + 1

**Key property:** Even if ALL heirs collude, they have N shares but need N+1. They're always one short — until the Bitcoin inheritance unlocks.

### Redundancy

After inheritance triggers, heirs have access to N+1 locked shares. The threshold is N+1. This means:
- Any single heir can recover alone (their 1 share + N+1 locked = N+2 > threshold ✓)
- Even if an heir loses their share, the locked shares alone meet threshold (N+1 = N+1 ✓)
- Maximum fault tolerance: all heirs could lose their pre-distributed shares and still recover

## How Locked Shares Reach Heirs

The locked Shamir shares are encrypted and stored alongside the Bitcoin inheritance. Two storage mechanisms:

### Option A: On-Chain (Preferred for Small Data)

Encrypt the locked shares and embed them as `OP_RETURN` outputs in the inheritance UTXO transaction. When heirs claim the Bitcoin:
1. They decode the transaction
2. Extract the `OP_RETURN` data
3. Decrypt using a key derived from their heir xpub + a known derivation path
4. Combine with their pre-distributed share → nsec recovered

**Limitation:** `OP_RETURN` is ~80 bytes. Codex32 shares are ~48 chars each. 4 shares ≈ 200 bytes. May need multiple `OP_RETURN` outputs or compression.

### Option B: Encrypted Nostr Events (More Flexible)

Publish the locked shares as NIP-44 encrypted events to Nostr relays:
1. Each locked share encrypted to each heir's npub (or derived from their xpub)
2. Published to multiple relays for redundancy
3. Heirs can always download and decrypt, but without threshold it's useless
4. After Bitcoin inheritance, they have enough shares

**Advantage:** No size limits, relay redundancy, heirs can pre-fetch.
**Trade-off:** Relies on at least one relay surviving.

### Option C: Descriptor Backup File (Simplest)

Include the locked shares in the descriptor backup file:
1. Descriptor backup already contains: descriptor, heir info, recovery instructions
2. Add: encrypted locked Shamir shares
3. File is stored securely by the owner (safe deposit box, etc.)
4. Upon death, heirs receive the file alongside Bitcoin inheritance

**Advantage:** Zero infrastructure dependency. Works offline.
**Trade-off:** Physical file must be stored and found by heirs.

### Recommendation

**Ship Option C first** (descriptor backup) — it works today, no infrastructure. Add Option B (Nostr events) as enhancement. Option A (on-chain) for future if data size permits.

## Notification Service Key

Separate from nsec inheritance, NoString needs to send check-in reminders:

1. **On first setup**, NoString generates a random Nostr keypair (the "service key")
2. Service key is stored locally, encrypted with the app password
3. Owner follows the service key's npub
4. NoString sends encrypted DMs from the service key:
   - "30 days until your timelock expires — check in soon"
   - "7 days remaining — this is getting urgent"
   - "1 day left — check in NOW or your heirs can claim"
5. Service key is NOT the owner's identity — it's a notification bot

## User Flows

### Owner Setup (New)

```
1. Import watch-only wallet (xpub)
2. Add heir(s) (their xpubs)
3. Configure timelock
4. [Optional] Enter nsec for identity inheritance
   → NoString Shamir-splits the nsec
   → Generates shares per the formula
   → Gives owner instructions to distribute 1 share per heir
   → Stores locked shares in descriptor backup
5. NoString generates service key for notifications
6. Owner follows service key npub
7. Download descriptor backup (includes locked shares if nsec provided)
```

### Heir Receives Share (Pre-distribution)

```
1. Owner gives heir their Shamir share (Codex32 string)
2. Heir stores it securely (paper, steel, safe)
3. Heir is told: "This is one piece of a key. You'll get the rest
   when the inheritance triggers. Keep it safe."
```

### After Inheritance Triggers

```
1. Bitcoin timelock expires
2. Heir claims Bitcoin using their own wallet
3. Heir obtains descriptor backup (from safe deposit box, lawyer, etc.)
4. Descriptor backup contains locked Shamir shares
5. Heir combines:
   - Their pre-distributed share (from step 2 of setup)
   - Locked shares from descriptor backup
   → Threshold met → nsec recovered
6. Heir imports nsec into their Nostr client
7. Heir now controls the deceased's Nostr identity
```

## Security Analysis

### Attack: All heirs collude while owner is alive
- N heirs have N shares, threshold is N+1
- **Blocked.** They need the locked shares which are in the descriptor backup (held by owner)

### Attack: Single heir finds descriptor backup
- Has 1 share + N+1 locked shares = N+2 total
- Threshold is N+1, so N+2 > N+1
- **Can recover nsec.** But this requires physical access to the backup file.
- **Mitigation:** Encrypt the locked shares in the backup with a password. Or: store backup in a location only accessible after death (lawyer, timed safe).

### Attack: Relay publishes encrypted shares early (Option B)
- Shares are encrypted to heir keys. Heirs can decrypt.
- But without threshold, decrypted shares are useless alone.
- After combining with locked shares (which they don't have yet), they could recover.
- **Same risk as finding the descriptor backup.** Mitigated by the locked shares not being accessible.

### Attack: Owner wants to revoke
- Owner generates new Shamir split with different shares
- Distributes new shares to heirs, tells them to destroy old ones
- Updates descriptor backup with new locked shares
- **Revocation works.** Old shares become useless if threshold changes.

## Implementation Plan

### Phase 1: Service Key (v0.2)
- [ ] Generate Nostr keypair on first setup
- [ ] Store encrypted in app state
- [ ] Send NIP-44 encrypted DMs for notifications
- [ ] UI: show service npub, "follow this for reminders"

### Phase 2: nsec Shamir Split (v0.3)
- [ ] Optional nsec input during setup
- [ ] Calculate threshold/shares based on heir count
- [ ] Generate Codex32 shares of nsec
- [ ] Display pre-distribution shares with instructions
- [ ] Include locked shares in descriptor backup file

### Phase 3: Heir Recovery Flow (v0.3)
- [ ] Heir guide for combining shares
- [ ] Validation tool: enter shares → verify threshold → reveal nsec
- [ ] Add to CLAIM_GUIDE.md

### Phase 4: Nostr Relay Storage (v0.4, optional)
- [ ] Publish encrypted locked shares to relays
- [ ] Heir pre-fetch mechanism
- [ ] Multi-relay redundancy

## Security Review

### Threat Model

| Threat | Severity | Mitigation |
|--------|----------|-----------|
| Heir collusion (all heirs combine shares early) | HIGH | N shares < N+1 threshold. Mathematically impossible. |
| Single heir finds descriptor backup | MEDIUM | Locked shares alone meet threshold. Consider password-protecting. |
| Attacker steals NoString device | LOW | nsec only in memory during Shamir split, then destroyed. Service key is low-value. |
| Heir loses their pre-distributed share | LOW | Locked shares alone = N+1 = threshold. Redundancy built in. |
| Owner wants to revoke after distributing shares | MEDIUM | Re-split nsec, distribute new shares, update backup. Old shares become useless. |
| Descriptor backup file compromised | MEDIUM | Contains locked shares. If attacker gets 0 pre-distributed shares, they still need 1 more. Password-protect for defense in depth. |

### Security Decisions

1. **nsec only in memory during split.** Never persisted to disk. After Shamir split completes, the raw nsec is zeroed from memory.
2. **Locked shares in backup are password-protected.** The descriptor backup file encrypts the locked shares section with the owner's app password. Heirs will need to know this password (included in heir guide, or shared separately).
3. **Service key is low-value.** If compromised, attacker can send fake DMs. No fund risk. Owner can regenerate.
4. **Codex32 format for shares.** Human-readable, checksummed, can be written on paper. No digital dependency.

## Open Questions

1. **What if the owner changes their nsec?** Need a re-split and re-distribution flow. v1: manual. v2: automated re-distribution.
2. **Should we support NIP-26 delegation?** Owner could delegate posting rights to heirs without giving full nsec. But NIP-26 isn't widely supported yet.
3. **Multi-sig for Nostr?** Future NIP for threshold Nostr signing could replace Shamir approach entirely.

---

*Created: 2026-02-02*
