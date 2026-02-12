<script lang="ts">
  import { signingSessionActive, navigate, appError } from '../lib/stores';
  import {
    startSigningSession,
    submitNonces,
    finalizeAndBroadcast,
    cancelSigningSession,
  } from '../lib/tauri';

  let step = $state<'idle' | 'nonces' | 'signing' | 'done'>('idle');
  let sessionData = $state<any>(null);
  let challengeData = $state<any>(null);
  let txid = $state('');
  let loading = $state(false);

  // Round 1: start + show nonce request for cosigner
  let ownerNoncesInput = $state('');
  let cosignerNoncesInput = $state('');

  // Round 2: show challenges, collect sigs
  let ownerSigsInput = $state('');
  let cosignerSigsInput = $state('');

  async function startSession() {
    loading = true;
    try {
      const result = await startSigningSession();
      if (result.success && result.data) {
        sessionData = result.data;
        signingSessionActive.set(true);
        step = 'nonces';
      } else {
        appError.set(result.error || 'Failed to start session');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    loading = false;
  }

  async function handleSubmitNonces() {
    loading = true;
    try {
      const ownerNonces = JSON.parse(ownerNoncesInput);
      const result = await submitNonces(ownerNonces, cosignerNoncesInput);
      if (result.success && result.data) {
        challengeData = result.data;
        step = 'signing';
      } else {
        appError.set(result.error || 'Failed to submit nonces');
      }
    } catch (e: any) {
      appError.set(e.message || 'Invalid input format');
    }
    loading = false;
  }

  async function handleFinalize() {
    loading = true;
    try {
      const ownerSigs = JSON.parse(ownerSigsInput);
      const result = await finalizeAndBroadcast(ownerSigs, cosignerSigsInput);
      if (result.success && result.data) {
        txid = result.data;
        step = 'done';
        signingSessionActive.set(false);
      } else {
        appError.set(result.error || 'Finalization failed');
      }
    } catch (e: any) {
      appError.set(e.message || 'Invalid input format');
    }
    loading = false;
  }

  async function handleCancel() {
    await cancelSigningSession();
    signingSessionActive.set(false);
    step = 'idle';
    sessionData = null;
    challengeData = null;
  }
</script>

<div class="screen">
  <h1>Check-in</h1>
  <p class="subtitle">MuSig2 signing ceremony to reset your inheritance timelock.</p>

  {#if step === 'idle'}
    <div class="info-box">
      <p>A check-in spends your vault's UTXOs back to the same address, resetting the timelock clock. This proves you're still in control.</p>
      <p>You'll need your signing device and your co-signer available.</p>
    </div>
    <button class="btn primary" onclick={startSession} disabled={loading}>
      {loading ? 'Starting...' : 'Start Check-in'}
    </button>

  {:else if step === 'nonces'}
    <div class="step-card">
      <h2>Round 1: Nonce Exchange</h2>
      <p>Send this to your co-signer:</p>
      <pre class="code-block">{JSON.stringify(sessionData?.nonce_request, null, 2)}</pre>

      <label>
        <span>Your signing device's PubNonces (JSON array of hex)</span>
        <textarea bind:value={ownerNoncesInput} rows="3" placeholder='["hex...", "hex..."]'></textarea>
      </label>

      <label>
        <span>Co-signer's NonceResponse (JSON)</span>
        <textarea bind:value={cosignerNoncesInput} rows="4" placeholder="Paste co-signer NonceResponse JSON here"></textarea>
      </label>

      <div class="actions">
        <button class="btn primary" onclick={handleSubmitNonces} disabled={loading}>
          {loading ? 'Processing...' : 'Submit Nonces'}
        </button>
        <button class="btn danger" onclick={handleCancel}>Cancel</button>
      </div>
    </div>

  {:else if step === 'signing'}
    <div class="step-card">
      <h2>Round 2: Signing</h2>
      <p>Send these challenges to both signing devices:</p>
      <pre class="code-block">{JSON.stringify(challengeData?.owner_challenges, null, 2)}</pre>

      <label>
        <span>Your signing device's partial signatures (JSON array of hex)</span>
        <textarea bind:value={ownerSigsInput} rows="3" placeholder='["hex...", "hex..."]'></textarea>
      </label>

      <label>
        <span>Co-signer's PartialSignatures (JSON)</span>
        <textarea bind:value={cosignerSigsInput} rows="4" placeholder="Paste co-signer PartialSignatures JSON here"></textarea>
      </label>

      <div class="actions">
        <button class="btn primary" onclick={handleFinalize} disabled={loading}>
          {loading ? 'Finalizing...' : 'Finalize & Broadcast'}
        </button>
        <button class="btn danger" onclick={handleCancel}>Cancel</button>
      </div>
    </div>

  {:else if step === 'done'}
    <div class="success-card">
      <h2>✅ Check-in Complete</h2>
      <p>Transaction broadcast successfully!</p>
      <div class="txid">
        <span class="label">txid</span>
        <a href="https://mempool.space/testnet/tx/{txid}" target="_blank" rel="noopener">
          {txid}
        </a>
      </div>
      <button class="btn primary" onclick={() => navigate('dashboard')}>
        Back to Dashboard
      </button>
    </div>
  {/if}
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 0.25rem; }
  h2 { margin-top: 0; }
  .subtitle { color: #888; margin-bottom: 2rem; }

  .info-box {
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 1.5rem;
  }

  .info-box p { color: #aaa; line-height: 1.6; margin: 0.5rem 0; font-size: 0.9rem; }

  .step-card {
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 8px;
    padding: 1.5rem;
  }

  .code-block {
    background: #0a0a0a;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 0.75rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.8rem;
    overflow-x: auto;
    max-height: 200px;
    overflow-y: auto;
    margin: 0.5rem 0 1rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    margin-bottom: 1rem;
  }

  label span { font-size: 0.85rem; color: #aaa; font-weight: 500; }

  textarea {
    background: #0a0a0a;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 0.75rem;
    color: #e0e0e0;
    font-size: 0.85rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  textarea:focus { outline: none; border-color: #f7931a; }

  .actions { display: flex; gap: 1rem; margin-top: 0.5rem; }

  .success-card {
    background: #0d2818;
    border: 1px solid #1a5c2e;
    border-radius: 8px;
    padding: 1.5rem;
  }

  .txid {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 1rem 0;
  }

  .txid .label { font-size: 0.8rem; color: #888; }
  .txid a {
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.85rem;
    color: #f7931a;
    word-break: break-all;
  }

  .btn {
    padding: 0.75rem 1.5rem;
    border: none;
    border-radius: 6px;
    font-size: 0.95rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .btn.primary { background: #f7931a; color: #000; }
  .btn.primary:hover { background: #f9a84d; }
  .btn.danger { background: #5c1a1a; color: #e0e0e0; }
  .btn.danger:hover { background: #7a2020; }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
