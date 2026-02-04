# NoString Threat Model

This document describes the security threats NoString is designed to mitigate and the assumptions we make.

---

## Assets Under Protection

| Asset | Sensitivity | Storage |
|-------|-------------|---------|
| BIP-39 seed (64 bytes) | Critical | Encrypted on disk |
| Nostr private key | Critical | Derived, never stored |
| Bitcoin private keys | Critical | Derived, never stored |
| Heir xpubs | Public | Config file |
| Policy descriptors | Public | Config file |
| UTXO data | Public | State file |

---

## Threat Actors

### 1. Remote Attacker (Network)

**Capabilities:**
- Intercept network traffic
- Control malicious Electrum servers
- Send malicious data

**Mitigations:**
- TLS required for Electrum connections
- Response validation for all network data
- No private keys transmitted over network
- Server responses treated as untrusted

### 2. Local Attacker (Physical Access)

**Capabilities:**
- Read files on disk
- Access memory dumps
- Install keyloggers

**Mitigations:**
- Seed encrypted with AES-256-GCM + Argon2id
- Password never stored
- Memory zeroing via `zeroize` crate (Zeroizing<> wrapper on derived keys, Drop impl on EncryptedSeed)
- Core dumps disabled via `setrlimit(RLIMIT_CORE, 0)` at startup

### 3. Malicious Heir

**Capabilities:**
- Has their private key
- Knows the policy structure
- May attempt early claim

**Mitigations:**
- Timelocks enforced by Bitcoin consensus
- Cannot claim until blocks elapse
- Owner can always outspend heir (immediate path)

### 4. Supply Chain Attacker

**Capabilities:**
- Compromise dependencies
- Modify build artifacts

**Mitigations:**
- Minimal dependencies
- Cargo.lock pinned
- Reproducible builds (pinned Rust toolchain, `--locked` builds, committed Cargo.lock)
- Signed releases (GPG-signed SHA256SUMS when key configured)

---

## Attack Scenarios

### A1: Seed Theft via Disk Access

**Scenario:** Attacker gains read access to ~/.nostring/seed.enc

**Impact:** Without password, encrypted blob is useless.

**Defense:**
1. AES-256-GCM authenticated encryption
2. Argon2id with 64MB memory cost (GPU-resistant)
3. Random salt prevents rainbow tables
4. Random nonce prevents replay

**Residual Risk:** Weak password enables brute force.

**Recommendation:** Enforce minimum password entropy.

---

### A2: Man-in-the-Middle on Electrum

**Scenario:** Attacker intercepts Electrum traffic

**Impact:** Could learn addresses of interest, return false UTXOs

**Defense:**
1. TLS required (ssl:// prefix)
2. Certificate validation
3. UTXOs verified against policy-derived addresses
4. Balance discrepancies detected

**Residual Risk:** Malicious server could withhold UTXOs (liveness attack).

**Recommendation:** Support multiple servers, compare results.

---

### A3: Timelock Bypass

**Scenario:** Heir attempts to claim before timelock expires

**Impact:** None if policy is correct

**Defense:**
1. Timelocks enforced by Bitcoin miners
2. Policy compiles to miniscript, not custom script
3. Descriptors are standard (wsh())
4. Multiple independent wallets can verify

**Residual Risk:** Bug in miniscript library (mitigated by using well-audited crate).

---

### A4: Notification Suppression

**Scenario:** Attacker prevents check-in reminders

**Impact:** Owner misses check-in, heir can claim

**Defense:**
1. Multiple notification channels (email + Nostr)
2. Local reminders in app
3. Calendar export
4. Self-hosted notification option

**Residual Risk:** All channels fail simultaneously.

**Recommendation:** Support SMS/push notifications.

---

### A5: PSBT Manipulation

**Scenario:** Attacker modifies PSBT before signing

**Impact:** Funds sent to wrong address

**Defense:**
1. Air-gapped signing (QR transfer)
2. Hardware wallet verifies outputs
3. App shows output addresses before broadcast

**Residual Risk:** User doesn't verify on hardware wallet.

**Recommendation:** Enforce verification prompts.

---

### A6: Memory Disclosure

**Scenario:** Attacker reads seed from memory

**Impact:** Complete fund loss

**Defense:**
1. Seed only in memory during operations
2. ✅ `zeroize` crate used — `Zeroizing<>` wrapper on derived keys, `Drop` impl zeroes EncryptedSeed fields
3. ✅ Core dumps disabled via `setrlimit(RLIMIT_CORE, 0)` at startup
4. ✅ Memory pages locked via `mlock()` — `LockedBuffer` RAII wrapper zeroizes + munlocks on drop

**Residual Risk:** Cold boot attacks, hypervisor escapes.

**Recommendation:** Implement memory protections, consider Rust secure memory crates.

---

## Trust Boundaries

```
┌─────────────────────────────────────────────────────────┐
│                     USER'S DEVICE                       │
│  ┌─────────────────────────────────────────────────┐   │
│  │              NOSTRING APP                        │   │
│  │  ┌──────────────┐  ┌──────────────────────────┐ │   │
│  │  │ Encrypted    │  │ Runtime (seed in memory) │ │   │
│  │  │ Seed File    │  └──────────────────────────┘ │   │
│  │  └──────────────┘                                │   │
│  └─────────────────────────────────────────────────┘   │
└────────────────────────┬────────────────────────────────┘
                         │ TLS
                         ▼
              ┌──────────────────────┐
              │   ELECTRUM SERVER    │
              │   (Untrusted)        │
              └──────────────────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │   BITCOIN NETWORK    │
              │   (Consensus Trust)  │
              └──────────────────────┘
```

---

## Security Assumptions

1. **Bitcoin consensus is honest** — Miners follow protocol rules
2. **Cryptographic primitives are secure** — AES-256, Argon2id, secp256k1
3. **Rust memory safety** — No buffer overflows in safe code
4. **User device not compromised** — No keyloggers, malware
5. **User password has sufficient entropy** — Weak passwords are their problem
6. **Hardware wallet is secure** — For air-gapped signing

---

## Recommendations for Audit

### Priority 1: Critical Path

1. `encrypt_seed()` / `decrypt_seed()` — Correct implementation
2. `InheritancePolicy::to_miniscript()` — No extra spending paths
3. `CheckinTxBuilder::build_psbt()` — Outputs are correct

### Priority 2: Data Handling

1. Verify seed never logged
2. Verify xpubs parsed correctly
3. Verify network responses validated

### Priority 3: Memory Safety

1. Add `zeroize` to seed handling
2. Disable core dumps
3. Lock memory pages (mlock)

---

## Open Issues

| Issue | Severity | Status |
|-------|----------|--------|
| Memory zeroing | Medium | ✅ Done (`zeroize` crate) |
| Core dump disable | Low | ✅ Done (`setrlimit`) |
| Minimum password entropy | Low | ✅ Done (entropy estimation + warnings) |
| Multi-server consensus | Low | Design documented (see §Multi-Server Consensus below) |

---

## Multi-Server Consensus (Future)

### Problem

A single `nostring-server` daemon monitors the owner's check-in. If that
server is compromised, an attacker could:
- Suppress notifications (owner thinks timelock is far away)
- Send false "timelock expiring" alerts to heirs
- Tamper with the check-in schedule

### Proposed Design

**N-of-M server consensus** — multiple independent servers must agree
that a check-in was missed before heir notifications fire.

**Architecture:**
1. Owner runs 2-3 `nostring-server` instances on different infrastructure
   (e.g., home server + VPS + friend's server)
2. Each server independently monitors the blockchain for check-in UTXOs
3. Servers publish signed attestations to Nostr relays:
   - `"check-in seen at block X"` or `"no check-in, timelock at Y%"`
4. Heir notification only fires when **M-of-N servers agree** the timelock
   is approaching/expired
5. Heirs (or a coordinator) verify the attestation signatures before acting

**Key properties:**
- No single server can trigger false alerts
- No single server can suppress real alerts
- Servers don't need to communicate directly (relay-based coordination)
- Each server is stateless except for its signing key

**Implementation priority:** Low — the Bitcoin timelock itself is the
consensus mechanism. Multi-server consensus adds defense-in-depth for
the notification layer, not the inheritance mechanism itself. A compromised
server can annoy heirs with false alerts but cannot steal funds or
bypass the timelock.

---

*Security is a process, not a product.*
