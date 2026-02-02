# Heir Onboarding Guide

*For designated heirs of a NoString inheritance plan.*

---

## What Is NoString?

NoString is a Bitcoin inheritance system that lets someone pass their Bitcoin to you if something happens to them. It uses Bitcoin's built-in timelock feature — no lawyers, no courts, no third parties.

**How it works:**
1. The owner sets up a policy naming you as an heir
2. The owner periodically "checks in" to prove they're still in control
3. If they stop checking in for a set period (e.g., 6 months), you can claim the Bitcoin

You don't need to do anything until it's time to claim. But you do need to set up a few things now.

---

## What You Need to Prepare

### 1. A Bitcoin Wallet (BIP-84 SegWit)

You need a wallet that supports **native SegWit** addresses (addresses starting with `bc1q...`). Any BIP-84 compatible wallet will work.

**Recommended: [Sparrow Wallet](https://sparrowwallet.com/)**
- Desktop app (macOS, Windows, Linux)
- Full-featured, privacy-focused
- Excellent for inheritance scenarios
- Free and open source

**Alternatives:**
- **[Electrum](https://electrum.org/)** — Lightweight, battle-tested, desktop
- **[BlueWallet](https://bluewallet.io/)** — Mobile-friendly (iOS/Android), beginner-friendly
- **[Specter Desktop](https://specter.solutions/)** — Advanced, hardware wallet focused

Any wallet that shows you an **xpub** (extended public key) will work.

### 2. Your Extended Public Key (xpub)

The owner needs your **xpub** to set up the inheritance. This is NOT your private key — it's safe to share with the owner.

**What is an xpub?**
- A special key that lets someone generate receive addresses for you
- Cannot be used to spend your Bitcoin
- Starts with `xpub...` (or `zpub...` for some wallets)

**How to export your xpub (Sparrow):**
1. Open Sparrow Wallet
2. Go to **Settings** → **Export**
3. Click **Show** next to "Master Public Key"
4. Copy the xpub string OR scan the QR code

**How to export your xpub (Electrum):**
1. Open Electrum
2. Go to **Wallet** → **Information**
3. Copy the "Master Public Key"

**How to export your xpub (BlueWallet):**
1. Open your wallet
2. Tap the **⋯** menu
3. Select **Export xPub**
4. Show the QR code to the owner

### 3. Secure Your Seed Phrase

Your wallet has a 12 or 24-word seed phrase. This is your master key.

**Critical:**
- Write it down on paper (not digital)
- Store it somewhere safe (fireproof, waterproof)
- Never share it with anyone
- Consider a steel backup for fire/flood protection

If you lose your seed phrase and need to claim the inheritance, you won't be able to access the Bitcoin.

---

## What the Owner Will Give You

After setup, the owner may provide:

### 1. **Claim Instructions**
A document explaining:
- The inheritance address(es)
- The timelock duration
- How to claim when the time comes

### 2. **Shamir Shares (Optional)**
If the owner uses Shamir backup, they may give you one or more **Codex32 shares**. These are strings that look like:
```
ms1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq
```

**Important:** A single share cannot recover the Bitcoin. You need a threshold number of shares (e.g., 2 of 3) working together with other heirs.

### 3. **Recovery Information**
Details you'll need to claim:
- The exact script/policy used
- Any multi-sig requirements
- Contact info for other heirs (if applicable)

---

## What Happens If the Owner Passes

1. **Wait for the timelock to expire**
   - The owner sets a check-in period (e.g., 6 months)
   - After they miss check-ins for this period, the timelock unlocks

2. **Use the CLAIM_GUIDE.md**
   - Follow the step-by-step claiming process
   - You'll need your wallet and any shares/info the owner provided

3. **Broadcast the claim transaction**
   - NoString helps you build a transaction that moves the Bitcoin to your wallet
   - Once broadcast and confirmed, the Bitcoin is yours

---

## Security Reminders

- **Your xpub is safe to share** with the owner (it's view-only)
- **Your seed phrase is NEVER safe to share** — it controls your funds
- **Store claim documents securely** — they're sensitive info
- **Test your wallet** — send a small amount to yourself to verify it works

---

## Questions?

If you're unsure about anything, ask the owner to walk you through:
- How to export your xpub
- What the timelock duration is
- What you'll need to do to claim

The owner should also give you a copy of **CLAIM_GUIDE.md** which has the step-by-step claiming process.

---

*NoString: Sovereign Bitcoin inheritance. No trusted third parties.*
