<script lang="ts">
  import { cosignerRegistered, cosignerLabel, navigate, appError } from '../lib/stores';
  import { registerCosigner } from '../lib/tauri';

  let pubkeyInput = $state('');
  let chainCodeInput = $state('');
  let labelInput = $state('');
  let loading = $state(false);

  async function handleRegister() {
    if (!pubkeyInput.trim() || !chainCodeInput.trim() || !labelInput.trim()) {
      appError.set('All fields are required');
      return;
    }

    loading = true;
    try {
      const result = await registerCosigner(
        pubkeyInput.trim(),
        chainCodeInput.trim(),
        labelInput.trim(),
      );
      if (result.success) {
        cosignerRegistered.set(true);
        cosignerLabel.set(labelInput.trim());
        appError.set(null);
        navigate('heirs');
      } else {
        appError.set(result.error || 'Registration failed');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    loading = false;
  }
</script>

<div class="screen">
  <h1>Setup</h1>
  <p class="subtitle">Register your co-signer for collaborative custody.</p>

  {#if $cosignerRegistered}
    <div class="success-card">
      <span class="check">✓</span>
      <div>
        <strong>Co-signer registered</strong>
        <p>{$cosignerLabel}</p>
      </div>
    </div>
    <button class="btn primary" onclick={() => navigate('heirs')}>
      Next: Add Heirs →
    </button>
  {:else}
    <div class="info-box">
      <p>Chain Code Delegation (CCD) lets you and a co-signer share custody without the co-signer ever seeing your transactions.</p>
      <p>The co-signer provides their <strong>compressed public key</strong>. You generate a <strong>chain code</strong> that stays secret from them.</p>
    </div>

    <div class="form">
      <label>
        <span>Co-signer Label</span>
        <input
          type="text"
          bind:value={labelInput}
          placeholder="e.g., Uncle Bob's ColdCard"
          maxlength="64"
        />
      </label>

      <label>
        <span>Co-signer Public Key (33-byte compressed, hex)</span>
        <input
          type="text"
          bind:value={pubkeyInput}
          placeholder="02a1633cafcc01ebfb6d..."
        />
        <p class="help">Ask your co-signer for their compressed public key (66 hex chars).</p>
      </label>

      <label>
        <span>Chain Code (32 bytes, hex)</span>
        <div class="chain-code-row">
          <input
            type="text"
            bind:value={chainCodeInput}
            placeholder="Random 32-byte chain code..."
          />
          <button class="btn secondary btn-small" onclick={() => {
            const arr = new Uint8Array(32);
            crypto.getRandomValues(arr);
            chainCodeInput = Array.from(arr).map(b => b.toString(16).padStart(2, '0')).join('');
          }}>
            🎲 Generate
          </button>
        </div>
        <p class="help">
          The chain code is YOUR secret. It enables deterministic key derivation
          without the co-signer knowing the derivation tree. Never share it.
        </p>
      </label>

      <button class="btn primary" onclick={handleRegister} disabled={loading}>
        {loading ? 'Registering...' : 'Register Co-signer'}
      </button>
    </div>
  {/if}
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 0.25rem; }
  .subtitle { color: var(--text-muted); margin-bottom: 2rem; }

  .info-box {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 1.5rem;
  }

  .info-box p { color: var(--text-muted); line-height: 1.6; margin: 0.5rem 0; font-size: 0.9rem; }

  .form {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  label span {
    font-size: 0.85rem;
    color: var(--text-muted);
    font-weight: 500;
  }

  .help { font-size: 0.8rem; color: var(--text-muted); margin: 0; line-height: 1.5; }

  input {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.75rem;
    color: var(--text);
    font-size: 0.95rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  input:focus { outline: none; border-color: var(--gold-light); }

  .chain-code-row {
    display: flex;
    gap: 0.5rem;
  }

  .chain-code-row input { flex: 1; }

  .btn {
    padding: 0.75rem 1.5rem;
    border: none;
    border-radius: 6px;
    font-size: 0.95rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .btn.primary { background: var(--gold-light); color: #000; }
  .btn.primary:hover { background: var(--gold); }
  .btn.secondary { background: var(--surface-variant); color: var(--text); }
  .btn.secondary:hover { background: var(--border); }
  .btn.btn-small { padding: 0.5rem 0.75rem; font-size: 0.85rem; }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .success-card {
    display: flex;
    align-items: center;
    gap: 1rem;
    background: #0d2818;
    border: 1px solid #1a5c2e;
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 1.5rem;
  }

  .success-card .check { font-size: 1.5rem; color: #4ade80; }
  .success-card p {
    margin: 0.25rem 0 0;
    color: var(--text-muted);
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.85rem;
  }
</style>
