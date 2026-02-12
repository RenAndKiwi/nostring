<script lang="ts">
  import { onMount } from 'svelte';
  import { navigate, appError } from '../lib/stores';
  import { addHeir, listHeirs, removeHeir, validateXpub } from '../lib/tauri';
  import type { HeirInfo } from '../lib/tauri';

  let heirs = $state<HeirInfo[]>([]);
  let labelInput = $state('');
  let xpubInput = $state('');
  let npubInput = $state('');
  let loading = $state(false);

  // Validation
  let labelError = $state('');
  let xpubError = $state('');
  let npubError = $state('');

  // Remove confirmation
  let confirmRemove = $state<string | null>(null);
  let confirmRemoveLabel = $state('');

  async function refresh() {
    try {
      heirs = await listHeirs();
    } catch (e: any) {
      appError.set(e.message || 'Failed to load heirs');
    }
  }

  onMount(() => { refresh(); });

  function validateInputs(): boolean {
    let valid = true;
    labelError = '';
    xpubError = '';
    npubError = '';

    if (!labelInput.trim()) {
      labelError = 'Name is required';
      valid = false;
    }

    if (!xpubInput.trim()) {
      xpubError = 'xpub is required';
      valid = false;
    }

    if (npubInput.trim() && !npubInput.trim().startsWith('npub1')) {
      npubError = 'Must start with npub1';
      valid = false;
    }

    return valid;
  }

  async function handleAdd() {
    if (!validateInputs()) return;

    loading = true;
    appError.set(null);

    // Validate xpub with backend first
    try {
      const xpubCheck = await validateXpub(xpubInput.trim());
      if (!xpubCheck.success) {
        xpubError = xpubCheck.error || 'Invalid xpub format';
        loading = false;
        return;
      }
    } catch (e: any) {
      xpubError = e.message || 'Failed to validate xpub';
      loading = false;
      return;
    }

    try {
      const result = await addHeir(
        labelInput.trim(),
        xpubInput.trim(),
        undefined,
        npubInput.trim() || undefined,
      );
      if (result.success && result.data) {
        appError.set(null);
        labelInput = '';
        xpubInput = '';
        npubInput = '';
        labelError = '';
        xpubError = '';
        npubError = '';
        await refresh();
      } else {
        appError.set(result.error || 'Failed to add heir');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    loading = false;
  }

  function requestRemove(fingerprint: string, label: string) {
    confirmRemove = fingerprint;
    confirmRemoveLabel = label;
  }

  async function confirmRemoveHeir() {
    if (!confirmRemove) return;
    try {
      const result = await removeHeir(confirmRemove);
      if (result.success) {
        await refresh();
      } else {
        appError.set(result.error || 'Failed to remove heir');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    confirmRemove = null;
    confirmRemoveLabel = '';
  }

  const heirsWithNpub = $derived(heirs.filter(h => h.npub));
  const heirsWithoutNpub = $derived(heirs.filter(h => !h.npub));
</script>

<div class="screen">
  <h1>Heirs</h1>
  <p class="subtitle">Add the people who will inherit your Bitcoin.</p>

  {#if heirs.length > 0}
    <div class="delivery-summary">
      <span class="delivery-ok">📨 {heirsWithNpub.length} can receive NIP-17 delivery</span>
      {#if heirsWithoutNpub.length > 0}
        <span class="delivery-warn">📋 {heirsWithoutNpub.length} need manual backup</span>
      {/if}
    </div>

    <div class="heir-list">
      {#each heirs as heir}
        <div class="heir-card">
          <div class="heir-info">
            <span class="heir-icon">{heir.npub ? '📨' : '👤'}</span>
            <div>
              <span class="heir-name">{heir.label}</span>
              <span class="heir-fp">{heir.fingerprint}</span>
              {#if heir.npub}
                <span class="heir-npub">{heir.npub.substring(0, 24)}...</span>
              {:else}
                <span class="heir-no-npub">No npub — manual delivery only</span>
              {/if}
            </div>
          </div>
          <button class="btn-remove" onclick={() => requestRemove(heir.fingerprint, heir.label)} title="Remove heir">✕</button>
        </div>
      {/each}
    </div>
  {:else}
    <div class="empty">
      <p>No heirs added yet. Add at least one heir before creating a vault.</p>
    </div>
  {/if}

  <div class="form">
    <h2>Add Heir</h2>

    <label>
      <span>Name</span>
      <input type="text" bind:value={labelInput} placeholder="e.g., Alice" maxlength="64" class:input-error={labelError} />
      {#if labelError}<span class="field-error">{labelError}</span>{/if}
    </label>

    <label>
      <span>xpub or descriptor</span>
      <textarea bind:value={xpubInput} placeholder="tpubD6Nz... or [fingerprint/path]xpub..." rows="3" class:input-error={xpubError}></textarea>
      {#if xpubError}<span class="field-error">{xpubError}</span>{/if}
    </label>

    <label>
      <span>Nostr npub <span class="optional">(optional, for NIP-17 backup delivery)</span></span>
      <input type="text" bind:value={npubInput} placeholder="npub1..." class:input-error={npubError} />
      {#if npubError}<span class="field-error">{npubError}</span>{/if}
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

  <!-- Remove confirmation dialog -->
  {#if confirmRemove}
    <div class="confirm-overlay">
      <div class="confirm-dialog">
        <h3>Remove Heir</h3>
        <p>Remove <strong>{confirmRemoveLabel}</strong> from your heir list?</p>
        <p class="confirm-detail">If you've already created a vault, you'll need to recreate it without this heir.</p>
        <div class="actions">
          <button class="btn danger" onclick={confirmRemoveHeir}>Remove</button>
          <button class="btn secondary" onclick={() => confirmRemove = null}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 0.25rem; }
  h2 { font-size: 1.2rem; margin-top: 1.5rem; }
  .subtitle { color: #888; margin-bottom: 1.5rem; }

  .delivery-summary {
    display: flex;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 1rem;
    font-size: 0.85rem;
  }

  .delivery-ok { color: #4caf50; }
  .delivery-warn { color: #f7931a; }

  .empty {
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 8px;
    padding: 1.5rem;
    text-align: center;
    color: #888;
  }

  .form { display: flex; flex-direction: column; gap: 1.25rem; }

  label { display: flex; flex-direction: column; gap: 0.35rem; }
  label span { font-size: 0.85rem; color: #aaa; font-weight: 500; }
  .optional { font-weight: 400; color: #666; }

  .field-error { font-size: 0.8rem; color: #ff6b6b; font-weight: 400; }

  input, textarea {
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 0.75rem;
    color: #e0e0e0;
    font-size: 0.95rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  input:focus, textarea:focus { outline: none; border-color: #f7931a; }
  .input-error { border-color: #ff6b6b !important; }

  .heir-list { display: flex; flex-direction: column; gap: 0.5rem; margin-bottom: 1rem; }

  .heir-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 0.75rem 1rem;
  }

  .heir-info { display: flex; align-items: center; gap: 0.75rem; }
  .heir-icon { font-size: 1.2rem; }
  .heir-name { font-weight: 500; display: block; }
  .heir-fp { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.75rem; color: #666; display: block; }
  .heir-npub { font-size: 0.75rem; color: #4caf50; display: block; }
  .heir-no-npub { font-size: 0.75rem; color: #888; display: block; font-style: italic; }

  .btn-remove { background: none; border: none; color: #666; cursor: pointer; font-size: 1.1rem; padding: 0.25rem; }
  .btn-remove:hover { color: #ff4444; }

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

  .actions { display: flex; gap: 1rem; margin-top: 0.5rem; }

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
  .btn.danger { background: #5c1a1a; color: #e0e0e0; }
  .btn.danger:hover { background: #7a2020; }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
