<script lang="ts">
  import { onDestroy } from 'svelte';
  import { signingSessionActive, navigate, appError } from '../lib/stores';
  import {
    startSigningSession,
    submitNonces,
    finalizeAndBroadcast,
    cancelSigningSession,
    buildCheckinPsbt,
  } from '../lib/tauri';
  import type { SigningSessionData, ChallengeData } from '../lib/tauri';

  type Step = 'idle' | 'nonces' | 'signing' | 'done';

  let step = $state<Step>('idle');
  let sessionData = $state<SigningSessionData | null>(null);
  let challengeData = $state<ChallengeData | null>(null);
  let txid = $state('');
  let loading = $state(false);
  let copyFeedback = $state<string | null>(null);

  // PSBT export
  let psbtBase64 = $state('');
  let psbtLoading = $state(false);

  // Round 1: nonce exchange
  let ownerNoncesInput = $state('');
  let cosignerNoncesInput = $state('');
  let ownerNoncesError = $state('');
  let cosignerNoncesError = $state('');

  // Round 2: signing
  let ownerSigsInput = $state('');
  let cosignerSigsInput = $state('');
  let ownerSigsError = $state('');
  let cosignerSigsError = $state('');

  // Session timer (1 hour = 3600s)
  let sessionStartTime = $state<number | null>(null);
  let secondsRemaining = $state(3600);
  let timerInterval = $state<ReturnType<typeof setInterval> | null>(null);

  // Pre-broadcast confirmation
  let showConfirmBroadcast = $state(false);

  // TODO: derive from vault network once AppState exposes it
  const explorerBaseUrl = 'https://mempool.space/testnet/tx';

  const timerWarning = $derived(secondsRemaining <= 600);
  const timerCritical = $derived(secondsRemaining <= 120);
  const timerMinutes = $derived(Math.floor(secondsRemaining / 60));
  const timerSeconds = $derived(secondsRemaining % 60);
  const timerDisplay = $derived(`${timerMinutes}:${timerSeconds.toString().padStart(2, '0')}`);

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
    if (timerInterval) {
      clearInterval(timerInterval);
      timerInterval = null;
    }
    sessionStartTime = null;
  }

  const stepNumber = $derived(
    step === 'idle' ? 0 :
    step === 'nonces' ? 1 :
    step === 'signing' ? 2 :
    step === 'done' ? 3 : 0
  );

  function resetSession() {
    step = 'idle';
    sessionData = null;
    challengeData = null;
    ownerNoncesInput = '';
    cosignerNoncesInput = '';
    ownerSigsInput = '';
    cosignerSigsInput = '';
    ownerNoncesError = '';
    cosignerNoncesError = '';
    ownerSigsError = '';
    cosignerSigsError = '';
    signingSessionActive.set(false);
    showConfirmBroadcast = false;
    stopTimer();
  }

  async function copyToClipboard(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      copyFeedback = label;
      setTimeout(() => { copyFeedback = null; }, 2000);
    } catch {
      appError.set('Copy failed. Please select and copy manually.');
    }
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
    } catch {
      return { valid: false, parsed: [], error: 'Invalid JSON. Expected: ["hex...", "hex..."]' };
    }
  }

  function validateJson(input: string): { valid: boolean; error: string } {
    if (!input.trim()) return { valid: false, error: 'Required' };
    try {
      JSON.parse(input);
      return { valid: true, error: '' };
    } catch {
      return { valid: false, error: 'Invalid JSON' };
    }
  }

  // PSBT Export
  async function handleExportPsbt() {
    psbtLoading = true;
    appError.set(null);
    try {
      const result = await buildCheckinPsbt();
      if (result.success && result.data) {
        psbtBase64 = result.data;
      } else {
        appError.set(result.error || 'Failed to build PSBT');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    psbtLoading = false;
  }

  // Round 0: Start Session
  async function startSession() {
    loading = true;
    appError.set(null);
    try {
      const result = await startSigningSession();
      if (result.success && result.data) {
        sessionData = result.data;
        signingSessionActive.set(true);
        startTimer();
        step = 'nonces';
      } else {
        appError.set(result.error || 'Failed to start session');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    loading = false;
  }

  // Round 1: Submit Nonces
  async function handleSubmitNonces() {
    const nonceCheck = validateHexArray(ownerNoncesInput);
    ownerNoncesError = nonceCheck.error;
    const cosignerCheck = validateJson(cosignerNoncesInput);
    cosignerNoncesError = cosignerCheck.error;

    if (!nonceCheck.valid || !cosignerCheck.valid) return;

    loading = true;
    appError.set(null);
    try {
      const result = await submitNonces(nonceCheck.parsed, cosignerNoncesInput);
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

  // Round 2: Validate inputs, then show confirmation
  function handleRequestFinalize() {
    const sigCheck = validateHexArray(ownerSigsInput);
    ownerSigsError = sigCheck.error;
    const cosignerCheck = validateJson(cosignerSigsInput);
    cosignerSigsError = cosignerCheck.error;

    if (!sigCheck.valid || !cosignerCheck.valid) return;
    showConfirmBroadcast = true;
  }

  // Actually broadcast after user confirms
  async function handleConfirmedBroadcast() {
    showConfirmBroadcast = false;
    const sigCheck = validateHexArray(ownerSigsInput);
    if (!sigCheck.valid) return;

    loading = true;
    appError.set(null);
    try {
      const result = await finalizeAndBroadcast(sigCheck.parsed, cosignerSigsInput);
      if (result.success && result.data) {
        txid = result.data;
        step = 'done';
        signingSessionActive.set(false);
        stopTimer();
      } else {
        // Stay on signing step so user can retry
        appError.set(result.error || 'Finalization failed');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    loading = false;
  }

  async function handleCancel() {
    await cancelSigningSession();
    resetSession();
  }

  onDestroy(() => stopTimer());
</script>

<div class="screen">
  <h1>Check-in</h1>
  <p class="subtitle">MuSig2 signing ceremony to reset your inheritance timelock.</p>

  <!-- Step progress indicator -->
  {#if step !== 'idle' && step !== 'done'}
    <div class="progress-bar">
      <div class="progress-steps">
        <div class="step-dot" class:active={stepNumber >= 1} class:current={stepNumber === 1}>1</div>
        <div class="step-line" class:active={stepNumber >= 2}></div>
        <div class="step-dot" class:active={stepNumber >= 2} class:current={stepNumber === 2}>2</div>
        <div class="step-line" class:active={stepNumber >= 3}></div>
        <div class="step-dot" class:active={stepNumber >= 3} class:current={stepNumber === 3}>3</div>
      </div>
      <div class="progress-labels">
        <span>Nonces</span>
        <span>Sign</span>
        <span>Broadcast</span>
      </div>
      <div class="session-timer" class:timer-warning={timerWarning} class:timer-critical={timerCritical}>
        Session expires in {timerDisplay}
      </div>
    </div>
  {/if}

  <!-- Copy feedback toast -->
  {#if copyFeedback}
    <div class="copy-toast">Copied {copyFeedback}!</div>
  {/if}

  {#if step === 'idle'}
    <div class="info-box">
      <p>A check-in spends your vault's UTXOs back to the same address, resetting the timelock clock. This proves you're still in control.</p>
      <p>You'll need your signing device and your co-signer available.</p>
    </div>

    <div class="idle-actions">
      <button class="btn primary" onclick={startSession} disabled={loading}>
        {loading ? 'Starting...' : 'Start Check-in'}
      </button>

      <div class="psbt-section">
        <button class="btn secondary" onclick={handleExportPsbt} disabled={psbtLoading}>
          {psbtLoading ? 'Building...' : 'Export Unsigned PSBT'}
        </button>
        {#if psbtBase64}
          <div class="psbt-export">
            <div class="code-header">
              <span class="code-label">Unsigned PSBT (base64)</span>
              <button class="copy-btn" onclick={() => copyToClipboard(psbtBase64, 'PSBT')}>Copy</button>
            </div>
            <pre class="code-block">{psbtBase64}</pre>
          </div>
        {/if}
      </div>
    </div>

  {:else if step === 'nonces'}
    <div class="step-card">
      <h2>Round 1: Nonce Exchange</h2>
      <p>Send this nonce request to your co-signer:</p>

      <div class="code-header">
        <span class="code-label">NonceRequest</span>
        <button class="copy-btn" onclick={() => copyToClipboard(JSON.stringify(sessionData?.nonce_request, null, 2), 'nonce request')}>Copy</button>
      </div>
      <pre class="code-block">{JSON.stringify(sessionData?.nonce_request, null, 2)}</pre>

      <label>
        <span>Your signing device's PubNonces</span>
        <span class="hint">JSON array of hex strings, one per input</span>
        <textarea
          bind:value={ownerNoncesInput}
          rows="3"
          placeholder='["03ab12cd...", "02ef56..."]'
          class:input-error={ownerNoncesError}
        ></textarea>
        {#if ownerNoncesError}
          <span class="field-error">{ownerNoncesError}</span>
        {/if}
      </label>

      <label>
        <span>Co-signer's NonceResponse</span>
        <span class="hint">JSON object from co-signer</span>
        <textarea
          bind:value={cosignerNoncesInput}
          rows="4"
          placeholder="Paste co-signer NonceResponse JSON here"
          class:input-error={cosignerNoncesError}
        ></textarea>
        {#if cosignerNoncesError}
          <span class="field-error">{cosignerNoncesError}</span>
        {/if}
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
      <h2>Round 2: Collect Signatures</h2>
      <p>Send these challenges to both signing devices:</p>

      <div class="code-header">
        <span class="code-label">Sign Challenges</span>
        <button class="copy-btn" onclick={() => copyToClipboard(JSON.stringify(challengeData?.sign_challenge, null, 2), 'challenges')}>Copy</button>
      </div>
      <pre class="code-block">{JSON.stringify(challengeData?.sign_challenge, null, 2)}</pre>

      {#if challengeData?.owner_challenges}
        <div class="code-header">
          <span class="code-label">Owner Device Challenges (sighashes + agg nonces)</span>
          <button class="copy-btn" onclick={() => copyToClipboard(JSON.stringify(challengeData?.owner_challenges, null, 2), 'owner challenges')}>Copy</button>
        </div>
        <pre class="code-block">{JSON.stringify(challengeData?.owner_challenges, null, 2)}</pre>
      {/if}

      <label>
        <span>Your signing device's partial signatures</span>
        <span class="hint">JSON array of hex strings</span>
        <textarea
          bind:value={ownerSigsInput}
          rows="3"
          placeholder='["hex...", "hex..."]'
          class:input-error={ownerSigsError}
        ></textarea>
        {#if ownerSigsError}
          <span class="field-error">{ownerSigsError}</span>
        {/if}
      </label>

      <label>
        <span>Co-signer's PartialSignatures</span>
        <span class="hint">JSON object from co-signer</span>
        <textarea
          bind:value={cosignerSigsInput}
          rows="4"
          placeholder="Paste co-signer PartialSignatures JSON here"
          class:input-error={cosignerSigsError}
        ></textarea>
        {#if cosignerSigsError}
          <span class="field-error">{cosignerSigsError}</span>
        {/if}
      </label>

      <div class="actions">
        <button class="btn primary" onclick={handleRequestFinalize} disabled={loading}>
          {loading ? 'Broadcasting...' : 'Finalize & Broadcast'}
        </button>
        <button class="btn danger" onclick={handleCancel}>Cancel</button>
      </div>
    </div>

    <!-- Pre-broadcast confirmation -->
    {#if showConfirmBroadcast}
      <div class="confirm-overlay">
        <div class="confirm-dialog">
          <h3>Confirm Broadcast</h3>
          <p>This will broadcast the check-in transaction to the Bitcoin network. This action cannot be undone.</p>
          <p class="confirm-detail">The transaction spends your vault UTXOs back to the same address, resetting your inheritance timelock.</p>
          <div class="actions">
            <button class="btn primary" onclick={handleConfirmedBroadcast} disabled={loading}>
              {loading ? 'Broadcasting...' : 'Broadcast Now'}
            </button>
            <button class="btn secondary" onclick={() => showConfirmBroadcast = false}>Go Back</button>
          </div>
        </div>
      </div>
    {/if}

  {:else if step === 'done'}
    <div class="success-card">
      <h2>Check-in Complete</h2>
      <p>Transaction broadcast successfully! Your inheritance timelock has been reset.</p>
      <div class="txid">
        <span class="label">Transaction ID</span>
        <div class="txid-row">
          <a href="{explorerBaseUrl}/{txid}" target="_blank" rel="noopener">
            {txid}
          </a>
          <button class="copy-btn" onclick={() => copyToClipboard(txid, 'txid')}>Copy</button>
        </div>
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
  .subtitle { color: #888; margin-bottom: 1.5rem; }

  /* Progress indicator */
  .progress-bar {
    margin-bottom: 1.5rem;
    padding: 1rem;
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 8px;
  }

  .progress-steps {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .step-dot {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.8rem;
    font-weight: 700;
    background: #333;
    color: #666;
    transition: all 0.2s;
  }

  .step-dot.active { background: #f7931a; color: #000; }
  .step-dot.current { box-shadow: 0 0 0 3px rgba(247, 147, 26, 0.3); }

  .step-line {
    width: 60px;
    height: 2px;
    background: #333;
    transition: background 0.2s;
  }

  .step-line.active { background: #f7931a; }

  .progress-labels {
    display: flex;
    justify-content: space-between;
    padding: 0 0.5rem;
    margin-top: 0.5rem;
    font-size: 0.75rem;
    color: #888;
  }

  /* Session timer */
  .session-timer {
    text-align: center;
    margin-top: 0.5rem;
    font-size: 0.8rem;
    color: #888;
    font-variant-numeric: tabular-nums;
  }

  .session-timer.timer-warning { color: #f7931a; }
  .session-timer.timer-critical { color: #ff6b6b; font-weight: 600; }

  /* Confirmation dialog */
  .confirm-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }

  .confirm-dialog {
    background: #1a1a1a;
    border: 1px solid #444;
    border-radius: 12px;
    padding: 1.5rem;
    max-width: 420px;
    width: 90%;
  }

  .confirm-dialog h3 { margin-top: 0; }
  .confirm-dialog p { color: #aaa; line-height: 1.5; }
  .confirm-detail { font-size: 0.85rem; color: #888; }

  /* Copy toast */
  .copy-toast {
    position: fixed;
    top: 1rem;
    right: 1rem;
    background: #1a5c2e;
    border: 1px solid #2a8c4e;
    color: #e0e0e0;
    padding: 0.5rem 1rem;
    border-radius: 6px;
    font-size: 0.85rem;
    z-index: 100;
  }

  /* Info box */
  .info-box {
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 1.5rem;
  }

  .info-box p { color: #aaa; line-height: 1.6; margin: 0.5rem 0; font-size: 0.9rem; }

  /* Idle layout */
  .idle-actions { display: flex; flex-direction: column; gap: 1.5rem; }
  .psbt-section { display: flex; flex-direction: column; gap: 0.75rem; }
  .psbt-export { margin-top: 0.5rem; }

  /* Step cards */
  .step-card {
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 8px;
    padding: 1.5rem;
  }

  /* Code blocks with copy header */
  .code-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 0.5rem;
  }

  .code-label { font-size: 0.8rem; color: #888; font-weight: 500; }

  .copy-btn {
    background: #252525;
    border: 1px solid #444;
    border-radius: 4px;
    padding: 0.25rem 0.5rem;
    color: #ccc;
    font-size: 0.75rem;
    cursor: pointer;
    transition: all 0.15s;
  }

  .copy-btn:hover { background: #333; border-color: #f7931a; }

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
    margin: 0.25rem 0 1rem;
    user-select: all;
  }

  /* Form fields */
  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin-bottom: 1rem;
  }

  label span { font-size: 0.85rem; color: #aaa; font-weight: 500; }

  .hint { font-size: 0.75rem !important; color: #666 !important; font-weight: 400 !important; }
  .field-error { font-size: 0.8rem !important; color: #ff6b6b !important; font-weight: 400 !important; }

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
  textarea.input-error { border-color: #ff6b6b; }

  .actions { display: flex; gap: 1rem; margin-top: 0.5rem; }

  /* Success card */
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

  .txid-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .txid-row a {
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.85rem;
    color: #f7931a;
    word-break: break-all;
  }

  /* Buttons */
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
  .btn.secondary { background: #252525; color: #ccc; border: 1px solid #444; }
  .btn.secondary:hover { background: #333; }
  .btn.danger { background: #5c1a1a; color: #e0e0e0; }
  .btn.danger:hover { background: #7a2020; }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
