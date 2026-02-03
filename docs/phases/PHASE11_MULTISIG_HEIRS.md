# Phase 11: Multi-Sig Heirs — Require M-of-N Heirs to Agree Before Claiming

**Goal:** Allow the owner to configure threshold heir groups within the cascade policy, so that M-of-N heirs must cooperate (co-sign) before any of them can claim the inheritance UTXO.

**Status:** Planning

---

## Motivation

The current cascade policy system supports multiple heirs with individual timelocks:

```
or(
  pk(OWNER),
  or(
    and(pk(SPOUSE), older(26280)),      // spouse at 6 months
    or(
      and(pk(KID1), older(38880)),      // kid1 at 9 months
      and(pk(EXECUTOR), older(52560))   // executor at 12 months
    )
  )
)
```

Each heir can claim **independently** once their personal timelock expires. This is by design — a spouse shouldn't need anyone else's permission. But for groups of peers (e.g., three children inheriting equally), a single-key-per-child design has problems:

1. **No check on rogue heirs.** One child could sweep the entire UTXO without consulting siblings.
2. **No family consensus.** The owner may want to require siblings to agree before disbursement.
3. **Lost key = lost funds.** If a single heir loses their key, that recovery path is bricked. A threshold scheme (2-of-3) tolerates one lost key.

The miniscript policy engine (`crates/nostring-inherit/src/policy.rs`) already has `PathInfo::Multi(threshold, keys)` and the `thresh()` operator. The cascade builder already accepts `PathInfo` at each tier. **The plumbing exists.** What's missing is:

- A domain model for "heir groups" (named group, member list, threshold)
- UX for configuring groups in the setup wizard
- PSBT coordination workflow so heirs can co-sign
- Security analysis of failure modes
- Documentation and testing

---

## 1. Research & Analysis

### 1.1 What Already Works

| Component | Status | Notes |
|-----------|--------|-------|
| `PathInfo::Multi(thresh, keys)` | ✅ Implemented | Compiles to `thresh(M, pk(K1), ..., pk(KN))` |
| `InheritancePolicy::cascade()` | ✅ Implemented | Accepts `PathInfo` (single or multi) per tier |
| `InheritancePolicy::simple_with_multisig_heir()` | ✅ Implemented | Convenience for single-tier multi-sig heir |
| `InheritancePolicy::multisig_owner()` | ✅ Implemented | Multi-sig on the *owner* side |
| WSH descriptor compilation | ✅ Implemented | `to_wsh_descriptor()` works for multi-sig recovery paths |
| `HeirRegistry` | ✅ Implemented | Stores heirs by label + fingerprint + xpub |
| `HeirKey::to_descriptor_key()` | ✅ Implemented | Produces `[fp/path]xpub/<0;1>/*` |
| Test: `test_cascade_with_multisig_heirs` | ✅ Passes | 2-of-3 kids in a cascade with spouse + executor |
| Test: `test_multisig_heir_threshold` | ✅ Passes | Basic threshold validation |
| PSBT generation (check-in) | ✅ Implemented | `CheckinTxBuilder` produces unsigned PSBTs |

**Key insight:** The low-level miniscript machinery is ready. The gap is in the *application layer* — there's no concept of a named heir group, no multi-party PSBT signing flow, and the Tauri commands/UI don't expose group configuration.

### 1.2 Miniscript `thresh()` Behavior

The `thresh(M, pk(K1), pk(K2), ..., pk(KN))` policy compiles to a script that requires M valid signatures from the N provided keys. Inside our inheritance policy, this becomes:

```
and(
  thresh(2, pk(KID1), pk(KID2), pk(KID3)),
  older(38880)
)
```

Which means: after 38,880 blocks (~9 months), any 2 of the 3 kids can cooperate to spend.

**Compilation constraints:**
- P2WSH scripts are limited to **3,600 bytes** (consensus) and **10,000 weight units** (standardness).
- Each `pk()` adds ~34 bytes. A `thresh(3, pk, pk, pk, pk, pk)` is comfortably under limits.
- Practical limit: ~15-20 keys per threshold group in P2WSH. Beyond that, Taproot (P2TR) with script tree leaves would be needed.
- The `miniscript` Rust crate handles compilation and will error if the script exceeds limits.

### 1.3 How Liana Handles This

Liana (our upstream inspiration) supports:
- Multi-sig on primary spending path (e.g., 2-of-2 corporate keys)
- Multi-sig on recovery paths (e.g., 2-of-3 recovery keys after timelock)
- "Expanding multisig" pattern: 2-of-2 primary → 2-of-3 after delay (adds a recovery key)
- Each path can be single-key or multi-sig independently

Liana does NOT have:
- Named "heir groups" as a first-class concept
- Built-in multi-party PSBT coordination (they rely on external tools)
- Heir notification/delivery (that's our NoString differentiation)

### 1.4 PSBT Multi-Party Signing Workflow

When heirs need to co-sign, the standard PSBT workflow is:

```
1. Heir A (initiator) creates unsigned PSBT from the descriptor
2. Heir A signs with their key → partially signed PSBT
3. Heir A sends PSBT to Heir B (via file, QR, or relay)
4. Heir B adds their signature → fully signed PSBT (if M=2 reached)
5. Heir B (or anyone) broadcasts the finalized transaction
```

For NoString, this means:
- Any heir in the group can initiate a claim
- The PSBT must be passed between M heirs for signing
- Each heir signs with their own hardware wallet (SeedSigner, ColdCard, etc.)
- The `combinepsbt` step merges partial signatures

---

## 2. Design

### 2.1 Domain Model: `HeirGroup`

A new struct representing a named threshold group:

```rust
/// A named group of heirs that must cooperate to claim
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeirGroup {
    /// Unique identifier
    pub id: Uuid,
    /// Human-readable name (e.g., "Children", "Business Partners")
    pub name: String,
    /// Minimum signatures required (M)
    pub threshold: u8,
    /// Member heir fingerprints (references into HeirRegistry)
    pub members: Vec<Fingerprint>,
}
```

**Location:** `crates/nostring-inherit/src/heir.rs` alongside `HeirKey` and `HeirRegistry`.

### 2.2 Extended `HeirRegistry`

Add group management to the existing registry:

```rust
impl HeirRegistry {
    // Existing: add, remove, get, list, to_descriptor_keys...

    /// Create a new heir group from existing heirs
    pub fn create_group(
        &mut self,
        name: String,
        threshold: u8,
        member_fingerprints: Vec<Fingerprint>,
    ) -> Result<HeirGroup, HeirError>;

    /// Remove a group (members remain in registry)
    pub fn remove_group(&mut self, group_id: &Uuid) -> Option<HeirGroup>;

    /// List all groups
    pub fn list_groups(&self) -> &[HeirGroup];

    /// Convert a group to PathInfo::Multi for policy construction
    pub fn group_to_path_info(&self, group: &HeirGroup) -> Result<PathInfo, HeirError>;
}
```

### 2.3 Cascade Configuration with Groups

The `cascade()` builder already accepts `PathInfo` per tier. No change needed at the policy layer. The application layer needs a higher-level configuration:

```rust
/// A tier in the inheritance cascade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CascadeTier {
    /// Single heir at this timelock
    SingleHeir {
        heir_fingerprint: Fingerprint,
        timelock: Timelock,
    },
    /// Threshold group at this timelock
    HeirGroup {
        group_id: Uuid,
        timelock: Timelock,
    },
}

/// Full cascade configuration (serializable, stored in DB)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadeConfig {
    pub tiers: Vec<CascadeTier>,
}

impl CascadeConfig {
    /// Build the InheritancePolicy from this config + registry
    pub fn to_policy(
        &self,
        owner_key: DescriptorPublicKey,
        registry: &HeirRegistry,
    ) -> Result<InheritancePolicy, PolicyError>;
}
```

**Example configuration:**

```json
{
  "tiers": [
    {
      "SingleHeir": {
        "heir_fingerprint": "a1b2c3d4",
        "timelock": { "blocks": 26280 }
      }
    },
    {
      "HeirGroup": {
        "group_id": "550e8400-e29b-41d4-a716-446655440000",
        "timelock": { "blocks": 38880 }
      }
    },
    {
      "SingleHeir": {
        "heir_fingerprint": "e5f6a7b8",
        "timelock": { "blocks": 52560 }
      }
    }
  ]
}
```

This represents: Spouse alone at 6mo → 2-of-3 children at 9mo → Executor alone at 12mo.

### 2.4 SQLite Schema Changes

```sql
-- New table: heir groups
CREATE TABLE heir_groups (
    id TEXT PRIMARY KEY,           -- UUID
    name TEXT NOT NULL,
    threshold INTEGER NOT NULL,
    created_at INTEGER NOT NULL    -- Unix timestamp
);

-- New table: group membership (many-to-many)
CREATE TABLE heir_group_members (
    group_id TEXT NOT NULL REFERENCES heir_groups(id),
    heir_fingerprint TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (group_id, heir_fingerprint)
);

-- New table: cascade configuration
CREATE TABLE cascade_config (
    id INTEGER PRIMARY KEY,        -- Always 1 (singleton)
    config_json TEXT NOT NULL,     -- Serialized CascadeConfig
    updated_at INTEGER NOT NULL
);
```

### 2.5 Tauri Commands

New commands exposed to the frontend:

```rust
// Group management
#[tauri::command] fn create_heir_group(name: String, threshold: u8, members: Vec<String>) -> Result<HeirGroup>;
#[tauri::command] fn update_heir_group(id: String, name: Option<String>, threshold: Option<u8>, members: Option<Vec<String>>) -> Result<HeirGroup>;
#[tauri::command] fn delete_heir_group(id: String) -> Result<()>;
#[tauri::command] fn list_heir_groups() -> Result<Vec<HeirGroup>>;

// Cascade configuration
#[tauri::command] fn set_cascade_config(config: CascadeConfig) -> Result<()>;
#[tauri::command] fn get_cascade_config() -> Result<Option<CascadeConfig>>;
#[tauri::command] fn preview_cascade_policy(config: CascadeConfig) -> Result<String>; // Returns descriptor string

// PSBT coordination (heir-side)
#[tauri::command] fn create_claim_psbt(descriptor: String, utxo: InheritanceUtxo, dest_address: String) -> Result<String>;
#[tauri::command] fn add_signature_to_psbt(psbt_base64: String) -> Result<String>; // Returns updated PSBT
#[tauri::command] fn check_psbt_completeness(psbt_base64: String) -> Result<PsbtStatus>;
#[tauri::command] fn finalize_and_broadcast_psbt(psbt_base64: String) -> Result<String>; // Returns txid
```

### 2.6 PSBT Coordination Protocol

For heirs to cooperate, they need a way to pass PSBTs between each other. Three options, in order of priority:

#### Option A: Manual File/QR Exchange (v1 — Ship First)
- Heir A exports PSBT as base64 string or QR code
- Sends to Heir B via any communication channel (email, Signal, in person)
- Heir B imports, adds signature, exports again
- Simple, no infrastructure needed, fully sovereign

#### Option B: Nostr Relay Coordination (v2 — After Initial Release)
- Heir A publishes partially-signed PSBT to a Nostr relay (NIP-44 encrypted to group members)
- Other heirs poll for pending PSBTs addressed to their npub
- Each heir signs and re-publishes
- Once M signatures collected, any heir can finalize and broadcast
- Uses existing `nostr_relay.rs` infrastructure
- Event kind: custom (e.g., `kind: 30079` — "inheritance PSBT coordination")

#### Option C: Direct P2P via Nostr DMs (v2 alternative)
- Use NIP-17 gift-wrapped DMs to pass PSBTs between heirs
- More private than relay publication
- Requires all heirs to be online (or check DMs periodically)

**Recommendation:** Ship Option A first. It's simpler, more sovereign, and works with air-gapped setups. Add Option B as an enhancement after the core flow is proven.

---

## 3. Security Review

### 3.1 Threat Model

| Threat | Severity | Mitigation |
|--------|----------|------------|
| One heir in group loses their key | **High** | Threshold design (2-of-3 tolerates 1 loss). Add backup key rotation mechanism. |
| Rogue heir tries to sweep solo | **Low** | Threshold requires M signatures; 1 key alone is insufficient |
| Heirs collude before timelock expires | **None** | Timelock is enforced by Bitcoin consensus; no number of signatures bypasses `older()` |
| PSBT intercepted during coordination | **Medium** | PSBT without all M sigs is useless. Option B adds NIP-44 encryption. |
| Heir impersonates another during PSBT exchange | **Low** | Each heir signs with their own hardware wallet; signatures are cryptographically bound |
| Group + timelock creates UTXO that nobody can claim | **High** | If threshold can't be met (too many lost keys), funds are permanently locked until a later cascade tier's timelock expires |
| Owner dies, heirs don't know about each other | **Medium** | Descriptor delivery (existing) includes group info. Heir notification includes member list. |

### 3.2 Key Loss Analysis

This is the most critical security consideration:

**Scenario: 2-of-3 group, one heir loses their key**
- ✅ Remaining 2 heirs can still claim. System works as designed.

**Scenario: 2-of-3 group, two heirs lose their keys**
- ❌ Group cannot claim. Only 1 valid signer remains.
- **Mitigation 1:** Later cascade tier (e.g., executor at 12 months) can still claim.
- **Mitigation 2:** Owner should design the cascade with a fallback single-key tier.
- **Mitigation 3:** Consider recommending 2-of-3 over 3-of-5 for family setups (fewer keys to manage).

**Scenario: All heirs in a group lose keys**
- ❌ Group tier is bricked.
- **Mitigation:** Cascade fallback tiers. This is why cascade + multi-sig is powerful — later tiers catch failures in earlier tiers.

**Recommended cascade pattern for families:**
```
Tier 1: Spouse alone (6 months) — simplest, most likely claimant
Tier 2: 2-of-3 children (9 months) — group consensus, tolerates 1 key loss
Tier 3: Family lawyer / executor alone (12 months) — ultimate fallback
```

### 3.3 Interaction with Shamir nsec Inheritance

The nsec Shamir split (`nostring-shamir`) is **orthogonal** to multi-sig heirs:

| Concern | Bitcoin Inheritance (Multi-Sig) | Nostr Identity (Shamir) |
|---------|-------------------------------|------------------------|
| What's protected | Bitcoin UTXOs | Nostr nsec (private key) |
| Protection mechanism | Miniscript `thresh()` + `older()` | Shamir Secret Sharing (SLIP-39/Codex32) |
| Key type | BIP-84 xpubs per heir | Shamir shares of master nsec |
| Threshold meaning | M heirs must sign a Bitcoin tx | M shares must be combined to reconstruct nsec |
| On-chain enforcement | Yes (consensus rules) | No (off-chain secret reconstruction) |

**They can share the same threshold parameters but serve different purposes:**
- Multi-sig heirs: "2-of-3 kids must agree to move Bitcoin"
- Shamir nsec: "2-of-3 shares must be combined to recover the Nostr identity"

**Recommended UX:** When creating a 2-of-3 heir group for Bitcoin, offer to also split the nsec with a matching 2-of-3 Shamir scheme and distribute one share to each group member. This creates a unified mental model: "any 2 of my 3 kids can recover both my Bitcoin and my Nostr identity."

**Important distinction:** Shamir share distribution happens once (at setup). Multi-sig coordination happens at claim time (months/years later). The two mechanisms are independent at the protocol level but should feel unified in the UX.

### 3.4 Script Size Considerations

| Group Size | Script Size (approx) | Witness Size (approx) | Viable in P2WSH? |
|------------|---------------------|-----------------------|-------------------|
| 2-of-3 | ~200 bytes | ~180 bytes | ✅ Yes |
| 3-of-5 | ~300 bytes | ~250 bytes | ✅ Yes |
| 5-of-7 | ~450 bytes | ~380 bytes | ✅ Yes |
| 7-of-10 | ~600 bytes | ~500 bytes | ✅ Yes, but expensive |
| 10-of-15 | ~900 bytes | ~750 bytes | ⚠️ Marginal |
| 15-of-20 | ~1200 bytes | ~1000 bytes | ❌ Too large for practical use |

**Recommendation:** Cap group size at 7 members in the UI with a warning above 5. For larger groups, future Taproot (P2TR) support would use script tree leaves to avoid witness bloat.

---

## 4. UX Design

### 4.1 Setup Wizard Changes

The current heir setup flow:
```
Add Heir → Enter label + xpub → Set timelock → Done
```

Extended flow:
```
Add Recovery Tier →
  ├─ "Single Heir" → Enter label + xpub → Set timelock → Done
  └─ "Heir Group (Multi-Sig)" →
       Enter group name (e.g., "Children") →
       Set threshold (M) →
       Add members:
         ├─ Select existing heir from registry
         └─ Add new heir (label + xpub)
       Set timelock →
       Review (e.g., "2 of 3 Children must sign after 9 months") →
       Done
```

### 4.2 Cascade Visualization

Display the cascade as a visual timeline:

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Owner can always spend
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
      ↓ 6 months                ↓ 9 months           ↓ 12 months
  ┌──────────┐           ┌──────────────┐        ┌──────────┐
  │  Spouse  │           │  2-of-3 Kids │        │ Executor │
  │  (solo)  │           │  Alice, Bob, │        │  (solo)  │
  │          │           │  Charlie     │        │          │
  └──────────┘           └──────────────┘        └──────────┘
```

### 4.3 Heir Claim Flow (Multi-Sig)

When an heir in a group wants to claim:

```
1. "Start Claim" → App detects this tier is multi-sig (2-of-3)
2. "You need 1 more signature. Create PSBT?"
3. App creates unsigned PSBT for the claim transaction
4. Heir A signs with hardware wallet → partially signed PSBT
5. Export options:
   ├─ Copy base64 to clipboard
   ├─ Save as .psbt file
   ├─ Display as animated QR code
   └─ (Future) Send via Nostr DM to group members
6. "Share this PSBT with another group member for co-signing"
7. Heir B imports PSBT → signs → PSBT now has 2/2 required sigs
8. "PSBT complete! Ready to broadcast?"
9. Broadcast → Funds sent to destination address
```

### 4.4 Group Member Notifications

When the owner's timelock is approaching critical for a multi-sig tier:

**Existing behavior (single heir):** Deliver descriptor backup to heir's npub/email.

**New behavior (heir group):** Deliver descriptor backup to **all group members**, with additional context:
- "You are part of a 2-of-3 group. You will need to coordinate with at least 1 other member."
- Include list of other group members' labels (but NOT their keys — they already have the descriptor).
- Include group members' npubs (if configured) so they can contact each other.
- Optionally include a "coordination guide" explaining the PSBT signing flow.

### 4.5 Configuration Validation

The UI must validate:
- Threshold ≥ 1 and ≤ group size
- Group size ≥ 2 (otherwise use single heir)
- No heir appears in multiple tiers (existing duplicate-key check)
- No heir appears in multiple groups
- At least one cascade tier exists
- Timelocks are strictly ascending (existing check)
- Script compiles successfully (call `to_wsh_descriptor()` as validation)

### 4.6 Recommended Presets

Offer one-click templates alongside "Build Your Own":

| Template | Structure | Use Case |
|----------|-----------|----------|
| **Simple Inheritance** | Spouse at 6mo | Married, no kids |
| **Family Cascade** | Spouse at 6mo → 2-of-3 Kids at 9mo → Executor at 12mo | Family with children |
| **Business Partners** | 2-of-3 partners at 6mo → Lawyer at 12mo | Small business |
| **Solo Backup** | Backup key at 12mo | Single person, recovery key in safe deposit box |

---

## 5. Implementation Roadmap

### Phase 11.1: Domain Model & Persistence
- [ ] `HeirGroup` struct in `heir.rs`
- [ ] `HeirRegistry` group management methods
- [ ] `CascadeConfig` / `CascadeTier` structs
- [ ] SQLite migration: `heir_groups`, `heir_group_members`, `cascade_config` tables
- [ ] CRUD operations for groups in DB
- [ ] Unit tests: group creation, validation, persistence roundtrip

### Phase 11.2: Policy Integration
- [ ] `CascadeConfig::to_policy()` — convert config + registry to `InheritancePolicy`
- [ ] Validation: threshold bounds, duplicate keys, script size estimation
- [ ] `preview_cascade_policy` — compile and return descriptor string without committing
- [ ] Integration tests: config → policy → wsh descriptor for various cascade shapes
- [ ] Test edge cases: 1-of-2, N-of-N, mixed single + group tiers

### Phase 11.3: Tauri Commands
- [ ] Group CRUD commands (`create_heir_group`, `update_heir_group`, `delete_heir_group`, `list_heir_groups`)
- [ ] Cascade config commands (`set_cascade_config`, `get_cascade_config`, `preview_cascade_policy`)
- [ ] Wire up policy compilation from cascade config
- [ ] Command tests

### Phase 11.4: PSBT Coordination (Heir-Side)
- [ ] `create_claim_psbt` — build unsigned PSBT for a group claim
- [ ] `add_signature_to_psbt` — import signed PSBT, validate, merge
- [ ] `check_psbt_completeness` — report how many sigs present vs needed
- [ ] `finalize_and_broadcast_psbt` — finalize when threshold met, broadcast
- [ ] PSBT export: base64 string, file save, QR code
- [ ] PSBT import: base64 paste, file open, QR scan
- [ ] Unit tests: partial signing, combining, threshold detection

### Phase 11.5: Heir Notification Enhancement
- [ ] Update `generate_heir_delivery_message` to include group context
- [ ] Include group member list and npubs in delivery
- [ ] Add "coordination guide" template for multi-sig heirs
- [ ] Update escalation logic in `check_and_notify` to handle groups
- [ ] Tests for group-aware delivery messages

### Phase 11.6: Frontend (Tauri UI)
- [ ] Cascade setup wizard with "Single Heir" / "Heir Group" choice per tier
- [ ] Group configuration form (name, threshold, member selection)
- [ ] Cascade timeline visualization
- [ ] Template presets (Family Cascade, Business Partners, etc.)
- [ ] Claim flow: PSBT creation → export → import → broadcast
- [ ] PSBT status indicator ("1 of 2 signatures collected")

### Phase 11.7 (Future): Nostr Relay PSBT Coordination
- [ ] Custom event kind for PSBT coordination messages
- [ ] NIP-44 encrypted PSBT publication to group members
- [ ] Polling for pending PSBTs addressed to current heir
- [ ] Auto-combine received partial signatures
- [ ] Relay-based claim initiation notification

---

## 6. Testing Strategy

### Unit Tests
- `HeirGroup` creation with valid/invalid thresholds
- `CascadeConfig` → `InheritancePolicy` conversion
- Policy compilation to WSH descriptor for all cascade shapes:
  - Single heir only
  - Group only
  - Mixed single + group cascade
  - Multiple groups at different tiers
- PSBT partial signing and combination
- Group persistence in SQLite

### Integration Tests
- End-to-end: create group → configure cascade → compile descriptor → generate address → create claim PSBT → partial sign → combine → finalize
- Descriptor roundtrip: compile → export → re-import → verify identical
- Notification delivery with group context

### Property Tests (if time allows)
- Random cascade configurations should always compile (if valid) or return clear errors (if invalid)
- `threshold ≤ group_size` invariant
- No duplicate keys across all tiers

---

## 7. Migration & Compatibility

### Backward Compatibility
- Existing single-heir cascades continue to work unchanged
- `CascadeConfig` with only `SingleHeir` tiers is equivalent to current behavior
- No changes to the `InheritancePolicy` or `PathInfo` core types
- Existing descriptors remain valid

### Data Migration
- New SQLite tables are additive (no existing table changes)
- Existing heirs in the registry remain as-is
- Groups are optional — users who don't need them never see them

### Descriptor Format
- Multi-sig heir descriptors are standard miniscript WSH — compatible with any miniscript-aware wallet
- Heirs using Liana, Sparrow, or Bitcoin Core with miniscript support can import the descriptor
- No proprietary extensions

---

## 8. Open Questions

1. **Maximum group size?** Recommend capping at 7 in UI. Should we hard-cap in the library too, or just warn?

2. **Same heir in multiple tiers?** Currently rejected (duplicate key check). Should we allow a child to appear both in a 2-of-3 group at 9mo AND as a solo heir at 15mo (different timelock)? This requires different derivation paths per tier.

3. **Group key rotation?** If an heir in a group loses their key, can the owner update the group (while still alive) without affecting the cascade? Yes — the owner does a check-in (spends + recreates UTXO) with the new descriptor. But the owner must be alive to do this.

4. **Taproot upgrade path?** P2TR with script tree leaves would allow larger groups and cheaper spends. Should we design the `CascadeConfig` to be forward-compatible with a future Taproot backend?

5. **Nostr relay coordination event kind?** Need to register or pick an appropriate NIP for PSBT coordination events. Could use NIP-89 (application handlers) or propose a new kind.

---

## 9. Dependencies

- No new crate dependencies required for Phase 11.1–11.5
- QR code generation for PSBT export may need `qrcode` crate (already used for Codex32 shares?)
- Nostr relay PSBT coordination (Phase 11.7) reuses existing `nostr_relay.rs` infrastructure

---

## 10. Estimated Effort

| Sub-phase | Effort | Notes |
|-----------|--------|-------|
| 11.1 Domain Model & Persistence | 1 day | Mostly new structs + SQLite |
| 11.2 Policy Integration | 1 day | Thin layer over existing `PathInfo::Multi` |
| 11.3 Tauri Commands | 0.5 day | Standard CRUD pattern |
| 11.4 PSBT Coordination | 2 days | Core complexity — multi-party signing flow |
| 11.5 Notification Enhancement | 0.5 day | Template updates |
| 11.6 Frontend | 2-3 days | Wizard UX, timeline visualization |
| 11.7 Nostr Relay Coordination | 2 days | Can be deferred to v0.4+ |
| **Total** | **~7-9 days** | Phases 11.1-11.6; 11.7 is optional |

---

*Phase 11 plan created: 2026-06-04*
*Author: Claude (subagent: nostring-multisig-plan)*
