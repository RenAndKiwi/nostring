# Testnet Check-in Broadcast Results

## Summary

**Status: ✅ SUCCESS** — Real Bitcoin testnet transaction broadcast and accepted by the network.

**Date:** 2025-07-24  
**Txid:** `c0ec27c76552cad2dbb58fb5119a1f02787ccd304513e7aa341377e191f6e261`  
**Explorer:** https://mempool.space/testnet/tx/c0ec27c76552cad2dbb58fb5119a1f02787ccd304513e7aa341377e191f6e261

## Transaction Details

| Field | Value |
|-------|-------|
| **Txid** | `c0ec27c76552cad2dbb58fb5119a1f02787ccd304513e7aa341377e191f6e261` |
| **From (P2WPKH)** | `tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v` |
| **To (P2WSH inheritance)** | Script: `00202d8b93a85303bedc6d1d028deba7a86e205464f563ccd747540139ced7bf98ca` |
| **Amount** | 347,470 sats |
| **Fee** | 500 sats (~4 sat/vB) |
| **Input UTXO** | `53036e1468b1c1c60ea8f40a3492515c0d91dbd056dcbbc827840d321810bff8:1` |
| **Testnet height at broadcast** | 4,839,122 |

## Inheritance Policy

- **Type:** Simple owner + heir with timelock
- **Owner path:** Immediate spend (pk)
- **Heir path:** After 26,280 blocks (~6 months CSV timelock)
- **Descriptor:**
  ```
  wsh(or_d(
    pk([7cee989c/84'/1'/0']tpubDDcuhfpKiqYsdyXmtGMAsz8tTYouMXvXReapLPuPdvQysKjA1ntNUQQnDTMgwy73CNczpVTRKNU9puj7KHbMCCjy48bShF67EiGmwMTazCU/<0;1>/*),
    and_v(
      v:pk([e4371042/84'/1'/0']tpubDDdR8W1iivsCrXZsZqmBSwVz1jFYy2oE3NsqFxntVfaE9VojCpKEpN59yXwRpbL8TTPgucWK3QNKqSHHwsVQr1mEsLaQWPhjerpXsBD9rea/<0;1>/*),
      older(26280)
    )
  ))#ne2x27p0
  ```

## Keys Used

### Owner
- **Mnemonic:** `wrap bubble bunker win flat south life shed twelve payment super taste`
- **BIP-84 Path:** `m/84'/1'/0'`
- **Fingerprint:** `7cee989c`
- **Address:** `tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v`

### Heir (generated fresh for this test)
- **Mnemonic:** `fire foil clutch chalk rich talent addict skirt marine few armor multiply display capable old now candy text monkey purity fiscal twelve mistake beauty`
- **BIP-84 Path:** `m/84'/1'/0'`
- **Fingerprint:** `e4371042`

## What This Proves

1. **Key derivation works** — BIP-84 testnet derivation from mnemonic produces correct addresses
2. **Inheritance policy compiles** — Miniscript policy with CSV timelock compiles to valid P2WSH descriptor
3. **Transaction building works** — P2WPKH → P2WSH funding transaction is correctly constructed
4. **ECDSA signing works** — BIP-143 sighash computation and ECDSA signing produce valid signatures
5. **Broadcast works** — Signed transaction accepted by testnet Electrum server (Blockstream)
6. **Funds are now in a real inheritance UTXO** — The P2WSH output encodes the inheritance policy on-chain

## How to Reproduce

```bash
cargo test -p nostring-e2e --test testnet_checkin_broadcast -- --ignored --nocapture
```

> **Note:** This test can only be run once with the original UTXO. After broadcast, the UTXO is spent.
> The funds are now locked in the P2WSH inheritance address. A future test could spend from
> this P2WSH output to perform an actual check-in (P2WSH → P2WSH self-spend).

## Raw Transaction

```
02000000000101f8bf1018320d8427c8bbdc56d0db910d5c5192340af4a80ec6c1b168146e0353
0100000000fdffffff014e4d0500000000002200202d8b93a85303bedc6d1d028deba7a86e2054
64f563ccd747540139ced7bf98ca024730440220253ecf171b488d8a3f2c6731b7aa2a0b1b1d72
40d45e593c9007fcf4ee2fb6170220169e8c292d9410dc97ba0b04a8aacd9bb7538d0b81bc7aa0
d03d69e4ce31e3c7012102db7c7ae04c18445adee4cd68cf495fc0cfc5785b19e28b0d2e0bf936
1ea459ad00000000
```

## Next Steps

1. **Check-in from P2WSH** — Build a test that spends from the P2WSH inheritance output back to a new P2WSH output (actual check-in = timelock reset)
2. **Heir recovery test** — After the timelock expires (~6 months on testnet), test heir recovery spending
3. **SeedSigner integration** — Export the PSBT to SeedSigner for hardware wallet signing
