<script lang="ts">
  import { navigate, appError } from '../lib/stores';
  import { addHeir, listHeirs, removeHeir } from '../lib/tauri';
  import type { HeirInfo } from '../lib/tauri';

  let heirs = $state<HeirInfo[]>([]);
  let labelInput = $state('');
  let xpubInput = $state('');
  let npubInput = $state('');
  let loading = $state(false);

  async function refresh() {
    try {
      heirs = await listHeirs();
    } catch (e: any) {
      appError.set(e.message || 'Failed to load heirs');
    }
  }

  // Load on mount
  $effect(() => { refresh(); });

  async function handleAdd() {
    if (!labelInput.trim() || !xpubInput.trim()) {
      appError.set('Label and xpub are required');
      return;
    }

    loading = true;
    try {
      const result = await addHeir(
        labelInput.trim(),
        xpubInput.trim(),
        undefined,
        npubInput.trim() || undefined,
      );
      if (result.success && result.data) {
        // Update npub if provided
        // TODO: update_heir_contact call for npub
        appError.set(null);
        labelInput = '';
        xpubInput = '';
        npubInput = '';
        await refresh();
      } else {
        appError.set(result.error || 'Failed to add heir');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    loading = false;
  }

  async function handleRemove(fingerprint: string) {
    try {
      const result = await removeHeir(fingerprint);
      if (result.success) {
        await refresh();
      } else {
        appError.set(result.error || 'Failed to remove heir');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
  }
</script>

<div class="screen">
  <h1>Heirs</h1>
  <p class="subtitle">Add the people who will inherit your Bitcoin.</p>

  {#if heirs.length > 0}
    <div class="heir-list">
      {#each heirs as heir}
        <div class="heir-card">
          <div class="heir-info">
            <span class="heir-icon">👤</span>
            <div>
              <span class="heir-name">{heir.label}</span>
              <span class="heir-fp">{heir.fingerprint}</span>
              {#if heir.npub}
                <span class="heir-npub">📨 {heir.npub.substring(0, 20)}...</span>
              {/if}
            </div>
          </div>
          <button class="btn-remove" onclick={() => handleRemove(heir.fingerprint)}>✕</button>
        </div>
      {/each}
    </div>
  {/if}

  <div class="form">
    <h2>Add Heir</h2>

    <label>
      <span>Name</span>
      <input type="text" bind:value={labelInput} placeholder="e.g., Alice" maxlength="64" />
    </label>

    <label>
      <span>xpub or descriptor</span>
      <textarea bind:value={xpubInput} placeholder="tpubD6Nz... or [fingerprint/path]xpub..." rows="3"></textarea>
    </label>

    <label>
      <span>Nostr npub (optional, for NIP-17 backup delivery)</span>
      <input type="text" bind:value={npubInput} placeholder="npub1..." />
    </label>

    <div class="actions">
      <button class="btn secondary" onclick={handleAdd} disabled={loading}>
        {loading ? 'Adding...' : '+ Add Heir'}
      </button>
      {#if heirs.length > 0}
        <button class="btn primary" onclick={() => navigate('vault')}>
          Next: Create Vault →
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 0.25rem; }
  h2 { font-size: 1.2rem; margin-top: 1.5rem; }
  .subtitle { color: #888; margin-bottom: 2rem; }

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

  .heir-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }

  .heir-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 0.75rem 1rem;
  }

  .heir-info {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .heir-icon { font-size: 1.2rem; }
  .heir-name { font-weight: 500; display: block; }
  .heir-fp {
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.75rem;
    color: #666;
  }
  .heir-npub {
    font-size: 0.75rem;
    color: #888;
    display: block;
  }

  .btn-remove {
    background: none;
    border: none;
    color: #666;
    cursor: pointer;
    font-size: 1.1rem;
    padding: 0.25rem;
  }

  .btn-remove:hover { color: #ff4444; }

  .actions {
    display: flex;
    gap: 1rem;
    margin-top: 0.5rem;
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
  .btn.secondary { background: #333; color: #e0e0e0; }
  .btn.secondary:hover { background: #444; }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
