<script lang="ts">
  import { cosignerRegistered, cosignerLabel, navigate, appError } from '../lib/stores';
  import { registerCosigner } from '../lib/tauri';

  let xpubInput = $state('');
  let labelInput = $state('');
  let loading = $state(false);

  async function handleRegister() {
    if (!xpubInput.trim() || !labelInput.trim()) {
      appError.set('Please enter both an xpub and a label');
      return;
    }

    loading = true;
    try {
      const result = await registerCosigner(xpubInput.trim(), labelInput.trim());
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
  <p class="subtitle">Register your co-signer to enable collaborative custody.</p>

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
    <div class="form">
      <label>
        <span>Co-signer Label</span>
        <input
          type="text"
          bind:value={labelInput}
          placeholder="e.g., My ColdCard"
          maxlength="64"
        />
      </label>

      <label>
        <span>Co-signer xpub</span>
        <textarea
          bind:value={xpubInput}
          placeholder="xpub6D..."
          rows="3"
        ></textarea>
      </label>

      <button class="btn primary" onclick={handleRegister} disabled={loading}>
        {loading ? 'Registering...' : 'Register Co-signer'}
      </button>
    </div>
  {/if}
</div>

<style>
  .screen {
    max-width: 600px;
  }

  h1 {
    font-size: 1.8rem;
    margin-bottom: 0.25rem;
  }

  .subtitle {
    color: #888;
    margin-bottom: 2rem;
  }

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
    color: #aaa;
    font-weight: 500;
  }

  input, textarea {
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 0.75rem;
    color: #e0e0e0;
    font-size: 0.95rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  input:focus, textarea:focus {
    outline: none;
    border-color: #f7931a;
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

  .btn.primary {
    background: #f7931a;
    color: #000;
  }

  .btn.primary:hover {
    background: #f9a84d;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

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

  .success-card .check {
    font-size: 1.5rem;
    color: #4ade80;
  }

  .success-card p {
    margin: 0.25rem 0 0;
    color: #888;
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.85rem;
  }
</style>
