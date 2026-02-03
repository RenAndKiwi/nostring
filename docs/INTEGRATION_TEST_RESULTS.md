# Integration Test Results — Live Network & Nostr Sprint

**Date:** 2026-02-03  
**Rust toolchain:** stable  
**All tests:** ✅ **5/5 PASSED**

---

## Test 1: Electrum Testnet Connection ✅ PASS

### Test 1a: Live Network Query

**Server:** `ssl://blockstream.info:993` (testnet3)  
**Status:** Connected successfully, all queries returned data.

| Check | Result |
|-------|--------|
| SSL Connection | ✅ Connected on first try |
| Tip header | ✅ Block timestamp age: ~142s |
| Balance query | ✅ **347,970 sats** confirmed |
| UTXO discovery | ✅ 1 UTXO found |
| Transaction history | ✅ 1 transaction |

**UTXO Details:**
```
txid: 53036e1468b1c1c60ea8f40a3492515c0d91dbd056dcbbc827840d321810bff8
vout: 1
value: 347,970 sats
confirmation height: 4,838,889
```

**Known Issue:** The `ElectrumClient::get_height()` method uses a binary search starting at block 930,000, which is a mainnet assumption. For testnet3 (height ~4.8M+), this causes the method to hang. The `get_tip_header()` method works correctly. **Recommendation:** Make `get_height()` network-aware or use `block_headers_subscribe()` for the tip.

### Test 1b: Mnemonic → Address Derivation ✅ PASS

Verified that the BIP-84 testnet derivation path (`m/84'/1'/0'/0/0`) from the mnemonic produces the expected address.

```
Mnemonic: wrap bubble bunker win flat south life shed twelve payment super taste
Derived:  tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v
Expected: tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v
✅ Match
```

---

## Test 2: Nostr DM Notifications (Real) ✅ PASS

**Relays:** `wss://relay.damus.io`, `wss://nos.lol`, `wss://relay.nostr.band`  
**Encryption:** NIP-04

| Step | Result |
|------|--------|
| Generate keypairs | ✅ Two fresh Ed25519 keypairs |
| NIP-04 encrypt | ✅ Encrypted DM content |
| Publish to relays | ✅ Event accepted by relays |
| Fetch back as recipient | ✅ 1 event found |
| Decrypt and verify | ✅ Content matches original |

**Keys Used (ephemeral, test-only):**
```
Service npub: npub1apg7taykjwuaryeaz765xmafmysrlx4gxv3nemwf5dzu27gvrxgs6z4jzy
Owner npub:   npub1pjyu0l5tdzyqeu3najs2v96vqs2exvucvjzt3w3s5vakry02k5lqtdmyxt
Event ID:     95b9295c403274cf9b668e051b02bd51cc9a2362441697404073c87a037d5208
```

**DM Content:** `NoString integration test DM — timestamp 1770158853`

**Note:** Required `rustls::crypto::ring::default_provider().install_default()` to be called before Nostr WebSocket connections. Without this, the nostr-sdk client panics with a CryptoProvider error. This was fixed in the test harness but should be addressed in the main application startup as well.

---

## Test 3: Nostr Relay Storage (Real) ✅ PASS

**Relays:** `wss://relay.damus.io`, `wss://nos.lol`, `wss://relay.nostr.band`  
**Encryption:** NIP-44 (V2, with NIP-04 fallback)

| Step | Result |
|------|--------|
| Generate test share (32 random bytes) | ✅ |
| Encrypt as SharePayload JSON | ✅ NIP-44 V2 |
| Publish to relays | ✅ 1 share published, event accepted |
| Fetch back as heir | ✅ 1 event found, 1 share recovered |
| Verify contents match | ✅ Share data + split_id match |

**Keys Used (ephemeral, test-only):**
```
Service npub: npub1mxy7vn48qtmjau69aa4k62ffepjtv6wsdjzxyq88gmc6mchuepjskxtf03
Heir npub:    npub1q6u5l082etcq97eejhrjdy3v3g7g0jg6rvvypev9ya88hsjl44cs5np6q7
Split ID:     69827b05cfbe
Event ID:     f8654a1242f9fbe8910e24697ca7b0b9c1d352bd793dfaa5d34025aee3894af8
```

**Full round-trip verified:** Encrypted share published → fetched → decrypted → JSON deserialized → content matches original random bytes.

---

## Test 4: Full PSBT Check-in Flow (Offline) ✅ PASS

**No network required** — pure cryptographic construction.

| Step | Result |
|------|--------|
| Derive owner keys from testnet mnemonic | ✅ xpub + fingerprint correct |
| Generate test heir xpub | ✅ Fresh 24-word mnemonic |
| Build inheritance policy (6-month timelock) | ✅ 26,280 blocks |
| Compile to WSH descriptor | ✅ Valid miniscript |
| Derive P2WSH script_pubkey | ✅ Native SegWit |
| Create test UTXO (347,970 sats) | ✅ |
| Build unsigned PSBT | ✅ |
| Verify witness_utxo | ✅ Correct amount + P2WSH |
| Verify witness_script | ✅ 77 bytes, hash matches P2WSH |
| Verify witness_script ↔ script_pubkey hash | ✅ SHA256 match |
| Verify tx version 2 (BIP-68) | ✅ |
| Verify empty witness (unsigned) | ✅ Ready for HW wallet |
| Verify empty script_sig (native SegWit) | ✅ |
| Verify self-spend output | ✅ Same address |
| Verify fee (384 sats at 2 sat/vB) | ✅ Reasonable |
| PSBT base64 encoding | ✅ Starts with `cHNidP8` |
| PSBT binary encoding | ✅ Starts with `psbt\xff` |

**Descriptor generated:**
```
wsh(or_d(
  pk([7cee989c/84'/1'/0']tpubDDcuhfp.../<0;1>/*),
  and_v(
    v:pk([360640e2/84'/1'/0']tpubDCxJBrc.../<0;1>/*),
    older(26280)
  )
))
```

**PSBT size:** 231 bytes — small enough for QR code transmission to SeedSigner/ColdCard.

---

## Summary

| Test | Status | Network? | Notes |
|------|--------|----------|-------|
| 1a: Electrum testnet | ✅ PASS | Yes (SSL) | 347,970 sats confirmed, UTXOs visible |
| 1b: Address derivation | ✅ PASS | No | BIP-84 testnet derivation correct |
| 2: Nostr DM | ✅ PASS | Yes (WSS) | NIP-04 encrypt/send/fetch/decrypt verified |
| 3: Nostr relay storage | ✅ PASS | Yes (WSS) | NIP-44 share publish/fetch round-trip verified |
| 4: PSBT check-in | ✅ PASS | No | Full PSBT construction, all BIP-174 fields correct |

### Issues Found

1. **`get_height()` hangs on testnet** — Binary search starts at mainnet block 930,000. Testnet3 is at ~4.8M. Use `block_headers_subscribe()` instead, or make the starting range network-dependent.

2. **Rustls CryptoProvider not installed** — `nostr-sdk` WebSocket connections require `rustls::crypto::ring::default_provider().install_default()` to be called before any relay connection. This should be added to the application initialization.

### Test File Location

```
tests/e2e/live_integration.rs
```

Run all integration tests:
```bash
cargo test -p nostring-e2e --test live_integration -- --ignored --nocapture
```
