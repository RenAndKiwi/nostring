# NoString: Your Bitcoin Inheritance, Proven on Testnet

*A full cascade demo — 3 heirs, timelocked Bitcoin, Shamir-split Nostr identity, zero trusted third parties.*

---

## The Problem Nobody Wants to Think About

You die. Your Bitcoin dies with you.

That's the current state of self-custody inheritance for most people. Your seed phrase is in a safe deposit box, maybe. Your family doesn't know what a UTXO is. Your lawyer thinks "private key" means a password. And the 2.4 million BTC estimated lost forever keeps growing.

Existing solutions ask you to trust someone — a custodian, a multisig service, a dead man's switch operator. But trusted third parties are security holes. That's not a Bitcoin opinion — it's a [mathematical fact](https://nakamotoinstitute.org/library/trusted-third-parties/).

**NoString solves this with pure Bitcoin script.** No custodians. No services to keep running. No trust required. Just math and timelocks.

---

## The Demo: A Family's Inheritance Plan

We built a real inheritance plan and executed it on Bitcoin testnet. Not a simulation — actual transactions broadcast to the network, validated by Bitcoin consensus rules.

### The Setup

**The owner** has Bitcoin and a Nostr identity (nsec). He sets up inheritance for three people:

| Heir | Role | Timelock | Priority |
|------|------|----------|----------|
| **Wife** | Spouse | 1 block (~10 min) | First to claim |
| **Daughter** | Child | 2 blocks (~20 min) | Second |
| **Lawyer** | Executor | 3 blocks (~30 min) | Last resort |

In production, these would be 6 months / 9 months / 12 months. We used 1/2/3 blocks for the demo so we could execute the full cascade in under an hour.

### What Gets Inherited

1. **Bitcoin** — locked in a P2WSH address that only unlocks for the right heir at the right time
2. **Nostr identity (nsec)** — split using Shamir's Secret Sharing, reconstructable by any heir

---

## How It Works: Step by Step

### 1. Key Derivation

From the owner's BIP-39 seed, we derive:
- The owner's spending key (immediate access)
- Unique keypairs for each heir (derived at different BIP-44 account indices)

Each heir also gets their own Nostr keypair for encrypted communication.

```
Owner:    tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v
Wife:     tb1qnw33udt6etnl0zwk6xnxr2afl8w53najjdjrvv  
Daughter: tb1qrk6q6qlzsnyackv3tckds630rzvw7ctdv563su
Lawyer:   tb1qcru22x4mmjxqgn3ll28nfrfqnmxyu5j0tsxz62
```

### 2. The Cascade Policy

NoString compiles a **miniscript policy** that encodes the inheritance rules directly into Bitcoin script:

```
or(
  pk(owner),                           // Owner can spend anytime
  or_i(
    and_v(v:pkh(wife), older(1)),      // Wife can spend after 1 block
    or_i(
      and_v(v:pkh(daughter), older(2)), // Daughter after 2 blocks
      and_v(v:pkh(lawyer), older(3))    // Lawyer after 3 blocks
    )
  )
)
```

This compiles into a single P2WSH address — a 125-byte witness script that Bitcoin nodes validate natively. No smart contracts. No oracles. Just Bitcoin.

**The cascade design matters.** If the wife claims, she gets the funds. If she doesn't (maybe she's also incapacitated), the daughter can claim after a longer wait. The lawyer is the backstop. It's a waterfall of priority — graceful degradation for the worst-case scenarios.

### 3. Shamir's Secret Sharing — Splitting the nsec

The owner's Nostr identity is just as valuable as their Bitcoin. NoString splits the nsec using Shamir's Secret Sharing (2-of-4 threshold):

- **Share 1** → Wife (pre-distributed, stored securely)
- **Share 2** → Daughter (pre-distributed)
- **Share 3** → Lawyer (pre-distributed)
- **Share 4** → Common "inheritance share" (delivered when claiming)

Any heir who claims their Bitcoin inheritance also receives Share 4. Combined with their pre-distributed share, they have 2-of-4 — enough to reconstruct the full nsec.

**No single share reveals anything.** A share alone is cryptographically indistinguishable from random bytes. Only the threshold combination recovers the secret.

### 4. Encrypted Delivery via Nostr DMs

Each heir's pre-distributed share is delivered as a **NIP-04 encrypted DM** from the owner's Nostr identity:

```
📨 Share DM → Wife     (event: 509694c9...)
📨 Share DM → Daughter  (event: 9898540f...)
📨 Share DM → Lawyer    (event: 25f1de3b...)
```

Only the intended heir can decrypt their share. Even if the relay is compromised, the shares are encrypted end-to-end with the heir's Nostr key.

### 5. Email Notifications

Each heir receives email notifications at key moments:

| Event | Recipients |
|-------|-----------|
| Inheritance configured | All 3 heirs |
| Timelock matured | Individual heir |
| Claim confirmed on-chain | Individual heir |

```
✉ Setup → ben+wife@bitcoinbutlers.com
✉ Setup → ben+daughter@bitcoinbutlers.com
✉ Setup → ben+lawyer@bitcoinbutlers.com
```

### 6. Funding the Inheritance

The owner sends Bitcoin to the P2WSH cascade address — 3 separate UTXOs so each heir has their own funds to claim:

```
Funding TX: [TXID_PLACEHOLDER]
├── Output 0: 3,000 sats → P2WSH (Wife's inheritance)
├── Output 1: 3,000 sats → P2WSH (Daughter's inheritance)
├── Output 2: 3,000 sats → P2WSH (Lawyer's inheritance)
└── Output 3: change → owner
```

[🔗 View on mempool.space](https://mempool.space/testnet/tx/TXID_PLACEHOLDER)

---

## The Cascade in Action

This is where it gets interesting. The owner is gone. The clock starts ticking.

### Block H+1: Wife's Turn

**Wife claims her inheritance ✅**

She constructs a transaction spending from Output 0, signs with her key, and broadcasts. The network validates:
- ✅ Her signature matches the `pkh(wife)` in the script
- ✅ The CSV timelock (1 block) has matured
- ✅ Transaction accepted and confirmed

```
Wife Claim TX: [WIFE_TXID_PLACEHOLDER]
```

**But what about the others?**

At the same block height, daughter and lawyer TRY to claim their outputs:

```
⛔ Daughter claim at H+1: REJECTED (CSV 2 not matured)
⛔ Lawyer claim at H+1:   REJECTED (CSV 3 not matured)
```

**Bitcoin consensus enforces the cascade.** The daughter's transaction is cryptographically valid (correct signature, correct key) but the timelock hasn't matured. The network won't include it in a block. Period.

### Block H+2: Daughter's Turn

One more block passes. The daughter's CSV-2 timelock matures.

**Daughter claims her inheritance ✅**

```
Daughter Claim TX: [DAUGHTER_TXID_PLACEHOLDER]
```

**Lawyer still locked out:**

```
⛔ Lawyer claim at H+2: REJECTED (CSV 3 not matured)
```

### Block H+3: Lawyer's Turn

The final timelock matures. The lawyer claims the last UTXO.

**Lawyer claims his inheritance ✅**

```
Lawyer Claim TX: [LAWYER_TXID_PLACEHOLDER]
```

### All Three Reconstruct the nsec

Each heir now combines their two Shamir shares:

```
Wife:     Share 1 (pre-distributed) + Share 4 (inheritance) → nsec ✅
Daughter: Share 2 (pre-distributed) + Share 4 (inheritance) → nsec ✅  
Lawyer:   Share 3 (pre-distributed) + Share 4 (inheritance) → nsec ✅
```

All three recover the same nsec. The owner's Nostr identity lives on.

---

## What Makes This Different

### No Trusted Third Parties

Every other inheritance solution requires trusting someone:
- **Custodians** can freeze, seize, or lose your funds
- **Multisig services** can collude or go bankrupt
- **Dead man's switches** require servers to keep running
- **Lawyers with seed phrases** are a single point of failure

NoString uses **Bitcoin script**. The rules are enforced by every node on the network. There's no server to go offline, no company to go bankrupt, no human to bribe.

### The Owner Stays in Control

The cascade has a keep-alive mechanism. As long as the owner is alive, they periodically "check in" by spending and re-locking their Bitcoin to a fresh inheritance address. This resets all the timelocks.

If the owner stops checking in (because they can't), the timelocks start counting down. The inheritance activates automatically.

### Graceful Degradation

Life is messy. What if:
- The wife is also in the accident? → Daughter inherits after a longer wait
- The whole family is unreachable? → Lawyer steps in as backstop
- The lawyer loses their key? → The earlier heirs had first priority anyway

The cascade handles all of these without any coordination between heirs.

### Your Digital Identity Survives

Bitcoin inheritance tools handle the money. NoString also handles the **identity**. Your Nostr nsec — your posts, your followers, your reputation — can be recovered by your heirs. 

The Shamir split ensures no single heir has full access to your identity until they've also proven they can claim the Bitcoin inheritance. The incentives are aligned.

---

## The Numbers

| Metric | Value |
|--------|-------|
| Witness script size | 125 bytes |
| Funding transaction | ~280 bytes |
| Claim transaction | ~235 bytes |
| On-chain cost (at 10 sat/vB) | ~$0.15 per claim |
| Shamir shares | 2-of-4 threshold |
| Heir notification channels | Email + Nostr DM |
| Trusted third parties | **Zero** |

---

## Try It Yourself

NoString is open source. The cascade demo test is at `tests/e2e/testnet_cascade_demo.rs` — you can run it yourself on testnet.

```bash
cargo test -p nostring-e2e --test testnet_cascade_demo -- --ignored --nocapture
```

**Testnet transactions from this demo:**

| Transaction | Link |
|-------------|------|
| Funding (3 UTXOs) | [TXID_PLACEHOLDER](https://mempool.space/testnet/tx/TXID_PLACEHOLDER) |
| Wife claim | [WIFE_TXID_PLACEHOLDER](https://mempool.space/testnet/tx/WIFE_TXID_PLACEHOLDER) |
| Daughter claim | [DAUGHTER_TXID_PLACEHOLDER](https://mempool.space/testnet/tx/DAUGHTER_TXID_PLACEHOLDER) |
| Lawyer claim | [LAWYER_TXID_PLACEHOLDER](https://mempool.space/testnet/tx/LAWYER_TXID_PLACEHOLDER) |

---

## What's Next

- **Mainnet hardening** — code signing, security audit, production config
- **CLI wizard** — guided setup for non-developers
- **Hardware wallet integration** — sign check-ins with your ColdCard/Jade
- **Codex32 backup** — paper-based Shamir shares that survive digital catastrophe
- **nostring.xyz** — landing page and documentation

---

*NoString: Hold your own keys in life. Pass them on in death.*

*No custodians. No services. Just Bitcoin script and math.*
