<script lang="ts">
  import { onDestroy } from 'svelte';
  import { signingSessionActive, navigate, appError } from '../lib/stores';
  import {
    startSigningSession, submitNonces, finalizeAndBroadcast,
    cancelSigningSession, buildCheckinPsbt,
  } from '../lib/tauri';
  import type { SigningSessionData, ChallengeData } from '../lib/tauri';
  import CheckInProgress from '../components/CheckInProgress.svelte';
  import CodeBlock from '../components/CodeBlock.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';

  type Step = 'idle' | 'nonces' | 'signing' | 'done';

  let step = $state<Step>('idle');
  let sessionData = $state<SigningSessionData | null>(null);
  let challengeData = $state<ChallengeData | null>(null);
  let txid = $state('');
  let loading = $state(false);
  let copyFeedback = $state<string | null>(null);
  let psbtBase64 = $state('');
  let psbtLoading = $state(false);

  // Round 1 inputs
  let ownerNoncesInput = $state('');
  let cosignerNoncesInput = $state('');
  let ownerNoncesError = $state('');
  let cosignerNoncesError = $state('');

  // Round 2 inputs
  let ownerSigsInput = $state('');
  let cosignerSigsInput = $state('');
  let ownerSigsError = $state('');
  let cosignerSigsError = $state('');

  // Session timer
  let sessionStartTime = $state<number | null>(null);
  let secondsRemaining = $state(3600);
  let timerInterval = $state<ReturnType<typeof setInterval> | null>(null);
  let showConfirmBroadcast = $state(false);

  const explorerBaseUrl = 'https://mempool.space/testnet/tx';

  const stepNumber = $derived(
    step === 'idle' ? 0 : step === 'nonces' ? 1 : step === 'signing' ? 2 : step === 'done' ? 3 : 0
  );

  // --- Timer ---
  function startTimer() {
    sessionStartTime = Date.now();
    secondsRemaining = 3600;
    if (timerInterval) clearInterval(timerInterval);
    timerInterval = setInterval(() => {
      if (!sessionStartTime) return;
      const elapsed = Math.floor((Date.now() - sessionStartTime) / 1000);
      secondsRemaining = Math.max(0, 3600 - elapsed);
      if (secondsRemaining === 0) {
        stopTimer();
        appError.set('Session expired. Please start a new check-in.');
        resetSession();
      }
    }, 1000);
  }

  function stopTimer() {
    if (timerInterval) { clearInterval(timerInterval); timerInterval = null; }
    sessionStartTime = null;
  }

  function resetSession() {
    step = 'idle';
    sessionData = null;
    challengeData = null;
    ownerNoncesInput = ''; cosignerNoncesInput = '';
    ownerSigsInput = ''; cosignerSigsInput = '';
    ownerNoncesError = ''; cosignerNoncesError = '';
    ownerSigsError = ''; cosignerSigsError = '';
    signingSessionActive.set(false);
    showConfirmBroadcast = false;
    stopTimer();
  }

  // --- Helpers ---
  async function copyToClipboard(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      copyFeedback = label;
      setTimeout(() => { copyFeedback = null; }, 2000);
    } catch { appError.set('Copy failed.'); }
  }

  function validateHexArray(input: string): { valid: boolean; parsed: string[]; error: string } {
    if (!input.trim()) return { valid: false, parsed: [], error: 'Required' };
    try {
      const arr = JSON.parse(input);
      if (!Array.isArray(arr)) return { valid: false, parsed: [], error: 'Must be a JSON array' };
      if (arr.length === 0) return { valid: false, parsed: [], error: 'Array cannot be empty' };
      for (let i = 0; i < arr.length; i++) {
        if (typeof arr[i] !== 'string') return { valid: false, parsed: [], error: `Item ${i} is not a string` };
        if (!/^[0-9a-fA-F]+$/.test(arr[i])) return { valid: false, parsed: [], error: `Item ${i} is not valid hex` };
      }
      return { valid: true, parsed: arr, error: '' };
    } catch { return { valid: false, parsed: [], error: 'Invalid JSON. Expected: ["hex...", "hex..."]' }; }
  }

  function validateJson(input: string): { valid: boolean; error: string } {
    if (!input.trim()) return { valid: false, error: 'Required' };
    try { JSON.parse(input); return { valid: true, error: '' }; }
    catch { return { valid: false, error: 'Invalid JSON' }; }
  }

  // --- Handlers ---
  async function handleExportPsbt() {
    psbtLoading = true; appError.set(null);
    try {
      const result = await buildCheckinPsbt();
      if (result.success && result.data) psbtBase64 = result.data;
      else appError.set(result.error || 'Failed to build PSBT');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    psbtLoading = false;
  }

  async function startSession() {
    loading = true; appError.set(null);
    try {
      const result = await startSigningSession();
      if (result.success && result.data) {
        sessionData = result.data;
        signingSessionActive.set(true);
        startTimer();
        step = 'nonces';
      } else appError.set(result.error || 'Failed to start session');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    loading = false;
  }

  async function handleSubmitNonces() {
    const nonceCheck = validateHexArray(ownerNoncesInput);
    ownerNoncesError = nonceCheck.error;
    const cosignerCheck = validateJson(cosignerNoncesInput);
    cosignerNoncesError = cosignerCheck.error;
    if (!nonceCheck.valid || !cosignerCheck.valid) return;

    loading = true; appError.set(null);
    try {
      const result = await submitNonces(nonceCheck.parsed, cosignerNoncesInput);
      if (result.success && result.data) { challengeData = result.data; step = 'signing'; }
      else appError.set(result.error || 'Failed to submit nonces');
    } catch (e: any) { appError.set(e.message || 'Invalid input format'); }
    loading = false;
  }

  function handleRequestFinalize() {
    const sigCheck = validateHexArray(ownerSigsInput);
    ownerSigsError = sigCheck.error;
    const cosignerCheck = validateJson(cosignerSigsInput);
    cosignerSigsError = cosignerCheck.error;
    if (!sigCheck.valid || !cosignerCheck.valid) return;
    showConfirmBroadcast = true;
  }

  async function handleConfirmedBroadcast() {
    showConfirmBroadcast = false;
    const sigCheck = validateHexArray(ownerSigsInput);
    if (!sigCheck.valid) return;
    loading = true; appError.set(null);
    try {
      const result = await finalizeAndBroadcast(sigCheck.parsed, cosignerSigsInput);
      if (result.success && result.data) {
        txid = result.data; step = 'done';
        signingSessionActive.set(false); stopTimer();
      } else appError.set(result.error || 'Finalization failed');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    loading = false;
  }

  async function handleCancel() { await cancelSigningSession(); resetSession(); }

  onDestroy(() => stopTimer());
</script>

<div class="screen">
  <h1>Check-in</h1>
  <p class="subtitle">MuSig2 signing ceremony to reset your inheritance timelock.</p>

  {#if step !== 'idle' && step !== 'done'}
    <CheckInProgress step={stepNumber} {secondsRemaining} />
  {/if}

  {#if copyFeedback}
    <div class="copy-toast">Copied {copyFeedback}!</div>
  {/if}

  {#if step === 'idle'}
    <div class="card info-box">
      <p>A check-in spends your vault's UTXOs back to the same address, resetting the timelock clock.</p>
      <p>You'll need your signing device and your co-signer available.</p>
    </div>

    <div class="idle-actions">
      <button class="btn btn-primary" onclick={startSession} disabled={loading}>
        {loading ? 'Starting...' : 'Start Check-in'}
      </button>

      <button class="btn btn-outline" onclick={handleExportPsbt} disabled={psbtLoading}>
        {psbtLoading ? 'Building...' : 'Export Unsigned PSBT'}
      </button>
      {#if psbtBase64}
        <CodeBlock label="Unsigned PSBT (base64)" content={psbtBase64} onCopy={copyToClipboard} />
      {/if}
    </div>

  {:else if step === 'nonces'}
    <div class="card">
      <h2>Round 1: Nonce Exchange</h2>
      <p class="step-desc">Send this nonce request to your co-signer:</p>

      <CodeBlock label="NonceRequest" content={JSON.stringify(sessionData?.nonce_request, null, 2)} onCopy={copyToClipboard} />

      <label>
        <span>Your signing device's PubNonces</span>
        <span class="hint">JSON array of hex strings, one per input</span>
        <textarea bind:value={ownerNoncesInput} rows="3" placeholder='["03ab12cd...", "02ef56..."]' class:input-error={ownerNoncesError}></textarea>
        {#if ownerNoncesError}<span class="field-error">{ownerNoncesError}</span>{/if}
      </label>

      <label>
        <span>Co-signer's NonceResponse</span>
        <span class="hint">JSON object from co-signer</span>
        <textarea bind:value={cosignerNoncesInput} rows="4" placeholder="Paste co-signer NonceResponse JSON here" class:input-error={cosignerNoncesError}></textarea>
        {#if cosignerNoncesError}<span class="field-error">{cosignerNoncesError}</span>{/if}
      </label>

      <div class="actions">
        <button class="btn btn-primary" onclick={handleSubmitNonces} disabled={loading}>
          {loading ? 'Processing...' : 'Submit Nonces'}
        </button>
        <button class="btn btn-danger" onclick={handleCancel}>Cancel</button>
      </div>
    </div>

  {:else if step === 'signing'}
    <div class="card">
      <h2>Round 2: Collect Signatures</h2>
      <p class="step-desc">Send these challenges to both signing devices:</p>

      <CodeBlock label="Sign Challenges" content={JSON.stringify(challengeData?.sign_challenge, null, 2)} onCopy={copyToClipboard} />

      {#if challengeData?.owner_challenges}
        <CodeBlock label="Owner Device Challenges" content={JSON.stringify(challengeData?.owner_challenges, null, 2)} onCopy={copyToClipboard} />
      {/if}

      <label>
        <span>Your signing device's partial signatures</span>
        <span class="hint">JSON array of hex strings</span>
        <textarea bind:value={ownerSigsInput} rows="3" placeholder='["hex...", "hex..."]' class:input-error={ownerSigsError}></textarea>
        {#if ownerSigsError}<span class="field-error">{ownerSigsError}</span>{/if}
      </label>

      <label>
        <span>Co-signer's PartialSignatures</span>
        <span class="hint">JSON object from co-signer</span>
        <textarea bind:value={cosignerSigsInput} rows="4" placeholder="Paste co-signer PartialSignatures JSON here" class:input-error={cosignerSigsError}></textarea>
        {#if cosignerSigsError}<span class="field-error">{cosignerSigsError}</span>{/if}
      </label>

      <div class="actions">
        <button class="btn btn-primary" onclick={handleRequestFinalize} disabled={loading}>
          {loading ? 'Broadcasting...' : 'Finalize & Broadcast'}
        </button>
        <button class="btn btn-danger" onclick={handleCancel}>Cancel</button>
      </div>
    </div>

    {#if showConfirmBroadcast}
      <ConfirmDialog
        title="Confirm Broadcast"
        message="This will broadcast the check-in transaction to the Bitcoin network. This action cannot be undone."
        detail="The transaction spends your vault UTXOs back to the same address, resetting your inheritance timelock."
        confirmLabel={loading ? 'Broadcasting...' : 'Broadcast Now'}
        {loading}
        onConfirm={handleConfirmedBroadcast}
        onCancel={() => showConfirmBroadcast = false}
      />
    {/if}

  {:else if step === 'done'}
    <div class="success-box" style="padding: var(--sp-xl);">
      <h2>Check-in Complete</h2>
      <p>Transaction broadcast successfully! Your inheritance timelock has been reset.</p>
      <div class="txid-section">
        <span class="label">Transaction ID</span>
        <div class="txid-row">
          <a href="{explorerBaseUrl}/{txid}" target="_blank" rel="noopener">{txid}</a>
          <button class="copy-btn" onclick={() => copyToClipboard(txid, 'txid')}>Copy</button>
        </div>
      </div>
      <button class="btn btn-primary" onclick={() => navigate('dashboard')}>Back to Dashboard</button>
    </div>
  {/if}
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 0.25rem; }
  h2 { margin-top: 0; }
  .subtitle { color: var(--text-muted); margin-bottom: 1.5rem; }
  .step-desc { color: var(--text-muted); font-size: 0.9rem; }

  .copy-toast {
    position: fixed; top: 1rem; right: 1rem;
    background: #1a5c2e; border: 1px solid #2a8c4e;
    color: var(--text); padding: 0.5rem 1rem;
    border-radius: var(--radius); font-size: 0.85rem; z-index: 100;
  }

  .info-box p { color: var(--text-muted); line-height: 1.6; margin: 0.5rem 0; font-size: 0.9rem; }
  .idle-actions { display: flex; flex-direction: column; gap: 1rem; }

  label { display: flex; flex-direction: column; gap: 0.25rem; margin-bottom: 1rem; }
  label span { font-size: 0.85rem; color: var(--text-muted); font-weight: 500; }
  .hint { font-size: 0.75rem !important; font-weight: 400 !important; }
  .field-error { font-size: 0.8rem !important; color: var(--error) !important; font-weight: 400 !important; }
  .input-error { border-color: var(--error) !important; }
  .actions { display: flex; gap: 1rem; margin-top: 0.5rem; }

  .txid-section { display: flex; flex-direction: column; gap: 0.25rem; margin: 1rem 0; }
  .txid-row { display: flex; align-items: center; gap: 0.5rem; }
  .txid-row a {
    font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.85rem;
    color: var(--gold-light); word-break: break-all;
  }
  .copy-btn {
    background: var(--surface-variant); border: 1px solid #444;
    border-radius: var(--radius-sm); padding: 0.25rem 0.5rem;
    color: var(--text); font-size: 0.75rem; cursor: pointer;
  }
  .copy-btn:hover { border-color: var(--gold-light); }
</style>
