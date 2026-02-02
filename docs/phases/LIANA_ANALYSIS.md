# Liana Analysis

**Objective:** Understand Liana's miniscript/timelock implementation for NoString inheritance.

---

## Repository Structure

```
liana-upstream/
├── liana/src/               ← Core library
│   ├── descriptors/
│   │   ├── mod.rs           ← LianaDescriptor, LianaPolicy
│   │   ├── keys.rs          ← Key types
│   │   └── analysis.rs      ← Policy analysis
│   ├── spend.rs             ← Transaction spending logic
│   ├── signer.rs            ← Hot signer implementation
│   └── lib.rs               ← Library exports
└── liana-gui/               ← Desktop GUI (Iced framework)
```

---

## Core Concepts

### LianaPolicy

The spending policy definition:
- **Primary path**: Owner can always spend (no timelock)
- **Recovery paths**: One or more timelocked paths for heirs

```rust
pub struct LianaPolicy {
    pub primary_path: PathInfo,
    pub recovery_paths: BTreeMap<u16, PathInfo>,  // timelock -> keys
}
```

### PathInfo

Describes keys for a spending path:
```rust
pub enum PathInfo {
    Single(DescriptorPublicKey),              // Single key
    Multi(usize, Vec<DescriptorPublicKey>),   // M-of-N multisig
}
```

### LianaDescriptor

The compiled descriptor:
- Contains multipath descriptor (receive + change)
- Manages PSBT creation and signing
- Handles address derivation

---

## Policy Examples

### Simple 1-owner, 1-heir
```
or(
  pk(OWNER),
  and(pkh(HEIR), older(52560))  // ~1 year
)
```

### 3-of-3 decaying to 2-of-3
```
or(
  multi(3, OWNER1, OWNER2, OWNER3),
  and(
    thresh(2, pkh(HEIR1), pkh(HEIR2), pkh(HEIR3)),
    older(26352)  // ~6 months
  )
)
```

### Cascade (multiple timelocks)
```
or(
  pk(OWNER),
  or(
    and(multi(2, SPOUSE, CHILD1, CHILD2), older(26280)),   // 6 months
    and(pk(EXECUTOR), older(39420))                        // 9 months
  )
)
```

---

## Key Methods to Port

### Policy Construction
```rust
LianaPolicy::new(
    primary_path: PathInfo,
    recovery_paths: BTreeMap<u16, PathInfo>,
)
```

### Descriptor Generation
```rust
let policy = LianaPolicy::new(...);
let descriptor = LianaDescriptor::new(policy);
let address = descriptor.receive_descriptor().derive(index, secp).address(network);
```

### Check-in Logic (from spend.rs)
1. Find the current timelock UTXO
2. Create transaction spending it (owner's primary path)
3. Create new output with same descriptor
4. Sign and broadcast

---

## What We Need for NoString

### From Liana (adapt/port):
1. **LianaPolicy** → `InheritancePolicy` in nostring-inherit
2. **Policy → Miniscript compilation** → Already in miniscript crate
3. **Descriptor management** → Adapt for our use case
4. **Timelock calculation** → Block-based, ~144 blocks/day

### We DON'T need:
1. **Full wallet functionality** — We're not a wallet
2. **PSBT workflow** — Simplified for check-in only
3. **GUI** — We have our own

---

## Timelock Math

| Duration | Blocks (~10 min each) |
|----------|----------------------|
| 1 day    | 144                  |
| 1 week   | 1,008                |
| 1 month  | ~4,320               |
| 6 months | ~26,280              |
| 1 year   | ~52,560              |

Maximum CSV value: 65,535 blocks (~455 days)

For longer timelocks, use CLTV with absolute block height.

---

## Security Observations

**Good:**
- Uses miniscript for policy compilation (verified, audited)
- Supports Taproot for better privacy
- Multiple recovery paths with cascade timelocks
- BIP32 key derivation throughout

**Our Adaptations:**
- We derive from BIP-39 seed (Liana uses external xpubs)
- We need simpler UTXO management (single inheritance UTXO)
- We add Shamir for seed backup (orthogonal to Liana's approach)

---

## Integration Plan

1. **Use miniscript crate directly** — Don't port Liana's wrapper
2. **Simplify policy** — Start with single owner + single heir
3. **Add complexity later** — Multi-heir, cascade timelocks in Phase 4
4. **Key derivation** — From our unified BIP-39 seed

---

*Analysis completed: 2026-02-02 01:00 CST*
