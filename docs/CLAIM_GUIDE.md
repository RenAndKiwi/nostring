# Claim Guide

*Emergency procedure for heirs to claim Bitcoin after timelock expiry.*

---

## Before You Begin

**Confirm the timelock has expired:**
- The owner set a check-in period (e.g., 6 months, 1 year)
- If they haven't checked in for this full period, the timelock has unlocked
- You can verify this by checking the current Bitcoin block height

**You will need:**
- [ ] Your Bitcoin wallet (with the seed phrase you set up during onboarding)
- [ ] The inheritance policy/descriptor from the owner
- [ ] Any Shamir shares (Codex32 strings) if applicable
- [ ] Contact info for other heirs (if multi-sig)

---

## Step 1: Verify the Timelock

The inheritance uses Bitcoin's `OP_CHECKSEQUENCEVERIFY` (relative timelock). The timelock is measured in blocks, not calendar time.

**To check if the timelock has expired:**

1. Find the **UTXO creation block** (when the inheritance was funded)
2. Find the **current block height** (check any block explorer)
3. Calculate: `current_height - creation_height`
4. Compare to the **required blocks** in the policy

**Example:**
- Inheritance funded at block 930,000
- Policy requires 26,280 blocks (~6 months)
- Current block is 958,000
- Elapsed: 958,000 - 930,000 = 28,000 blocks
- 28,000 > 26,280 → **Timelock has expired ✓**

**Block explorers:**
- [mempool.space](https://mempool.space)
- [blockstream.info](https://blockstream.info)

---

## Step 2: Gather Required Keys

### Single Heir (Simple Case)
You only need your own wallet. Skip to Step 3.

### Multi-Heir (Threshold)
If the policy requires multiple heirs (e.g., 2-of-3), coordinate with other heirs:

1. **Contact other heirs** listed in the policy
2. **Agree on a receive address** for the combined claim
3. **Each heir prepares their signing device**

### Shamir Backup (Codex32)
If the owner used Shamir secret sharing:

1. **Gather the required number of shares** (e.g., 2 of 3)
2. **Enter shares into NoString** to reconstruct the seed
3. **The reconstructed seed** is used to claim

**Codex32 shares look like:**
```
MS12NAMEA320UXWFEP5CJC5M94...  (share A)
MS12NAMES6XQGUZTTXKEQNJSJZ...  (share S)
```
Each share starts with "MS1" + threshold + identifier + share index + data.

---

## Step 3: Import the Policy

Open NoString and import the inheritance policy. 

> **Note:** The NoString desktop UI is under development. These steps describe 
> the intended workflow. For current usage, you may need to use the command-line 
> tools or integrate with Sparrow/Electrum for signing.

The owner should have provided one of:

**Option A: Descriptor String**
```
wsh(or_d(pk([owner_fp/84'/0'/0']xpub.../0/*),and_v(v:pk([heir_fp/84'/0'/1']xpub.../0/*),older(26280))))
```

**Option B: Policy File**
A JSON file containing the full policy configuration.

**To import:**
1. Open NoString
2. Go to **Settings** → **Import Policy**
3. Paste the descriptor or load the file
4. Verify the details match what the owner described

---

## Step 4: Build the Claim Transaction

1. **Select "Claim Inheritance"** in NoString
2. **Choose the UTXO(s)** to claim
3. **Enter your receive address** (from your wallet)
4. **Set the fee rate** (check mempool.space for current rates)
5. **Review the transaction**:
   - Verify the output goes to YOUR address
   - Verify the fee is reasonable
   - Verify the timelock path is being used

---

## Step 5: Sign the Transaction

### Using a Hardware Wallet (Recommended)
1. NoString displays a **QR code** with the unsigned PSBT
2. Scan with your hardware wallet (SeedSigner, ColdCard, etc.)
3. **Verify on the device:**
   - Output address matches your intended receive address
   - Amount is correct
   - Fee is reasonable
4. Sign the transaction
5. Export the signed PSBT (display as QR)
6. Scan the signed PSBT back into NoString

### Using Software Wallet
1. Export the PSBT from NoString
2. Open in Sparrow/Electrum
3. Sign with your wallet
4. Export the signed PSBT
5. Import back into NoString

### Multi-Sig Signing
If multiple heirs are required:
1. First heir signs and exports partially-signed PSBT
2. Pass to second heir (via secure channel or in person)
3. Second heir signs (now fully signed)
4. Any heir can then broadcast the fully-signed transaction

---

## Step 6: Broadcast

1. **Final review** of the signed transaction
2. Click **"Broadcast"**
3. NoString sends the transaction to the Bitcoin network
4. **Save the transaction ID (txid)**

---

## Step 7: Confirm

1. Check the transaction on a block explorer
2. Wait for **at least 1 confirmation** (typically 10-60 minutes)
3. **6 confirmations** is considered fully settled (~1 hour)
4. The Bitcoin is now in your wallet

---

## Troubleshooting

### "Timelock not expired"
- The required number of blocks hasn't passed yet
- Double-check the policy's block requirement
- Wait for more blocks to be mined

### "Invalid signature"
- Make sure you're using the correct key (the one matching your xpub in the policy)
- Check that your wallet is on the correct derivation path (BIP-84: m/84'/0'/0')

### "Transaction rejected"
- Fee might be too low (increase fee rate)
- UTXO might have been spent (owner checked in, or another heir claimed first)
- Check the inheritance address on a block explorer

### "Missing co-signer"
- For multi-sig, contact other heirs
- You cannot claim alone if threshold requires multiple signatures

### "Shamir reconstruction failed"
- Make sure you have enough shares (check threshold)
- Verify shares haven't been corrupted (checksum validation)
- Shares from different groups cannot be combined

---

## After Claiming

1. **Move funds to your own wallet** if not already there
2. **Secure your wallet** with proper backup
3. **Notify other heirs** (if applicable) that the claim is complete
4. **Consider tax implications** in your jurisdiction

---

## Emergency Contacts

If the owner provided emergency contacts (lawyer, estate executor, family), consider notifying them of the claim.

---

## Technical Reference

**Timelock math:**
- 1 day ≈ 144 blocks (10 min/block average)
- 1 month ≈ 4,320 blocks
- 6 months ≈ 26,280 blocks
- 1 year ≈ 52,560 blocks

**PSBT:** Partially Signed Bitcoin Transaction — the standard format for passing unsigned/partially-signed transactions between devices.

**Descriptor:** A standardized way to describe Bitcoin output scripts, including the keys and conditions required to spend.

---

*NoString: Your keys, your Bitcoin, your inheritance.*
