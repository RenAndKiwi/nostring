<script lang="ts">
  import { onMount } from 'svelte';
  import { navigate, appError } from '../lib/stores';
  import { addHeir, listHeirs, removeHeir, validateXpub } from '../lib/tauri';
  import type { HeirInfo } from '../lib/tauri';
  import HeirCard from '../components/HeirCard.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';

  let heirs = $state<HeirInfo[]>([]);
  let labelInput = $state('');
  let xpubInput = $state('');
  let npubInput = $state('');
  let loading = $state(false);
  let labelError = $state('');
  let xpubError = $state('');
  let npubError = $state('');
  let confirmRemove = $state<string | null>(null);
  let confirmRemoveLabel = $state('');

  const heirsWithNpub = $derived(heirs.filter(h => h.npub));
  const heirsWithoutNpub = $derived(heirs.filter(h => !h.npub));

  async function refresh() {
    try { heirs = await listHeirs(); }
    catch (e: any) { appError.set(e.message || 'Failed to load heirs'); }
  }

  onMount(() => { refresh(); });

  function validateInputs(): boolean {
    let valid = true;
    labelError = ''; xpubError = ''; npubError = '';
    if (!labelInput.trim()) { labelError = 'Name is required'; valid = false; }
    if (!xpubInput.trim()) { xpubError = 'xpub is required'; valid = false; }
    if (npubInput.trim() && !npubInput.trim().startsWith('npub1')) { npubError = 'Must start with npub1'; valid = false; }
    return valid;
  }

  async function handleAdd() {
    if (!validateInputs()) return;
    loading = true; appError.set(null);
    try {
      const xpubCheck = await validateXpub(xpubInput.trim());
      if (!xpubCheck.success) { xpubError = xpubCheck.error || 'Invalid xpub'; loading = false; return; }
    } catch (e: any) { xpubError = e.message || 'Validation failed'; loading = false; return; }

    try {
      const result = await addHeir(labelInput.trim(), xpubInput.trim(), undefined, npubInput.trim() || undefined);
      if (result.success && result.data) {
        labelInput = ''; xpubInput = ''; npubInput = '';
        labelError = ''; xpubError = ''; npubError = '';
        await refresh();
      } else appError.set(result.error || 'Failed to add heir');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    loading = false;
  }

  function requestRemove(fingerprint: string, label: string) {
    confirmRemove = fingerprint; confirmRemoveLabel = label;
  }

  async function confirmRemoveHeir() {
    if (!confirmRemove) return;
    try {
      const result = await removeHeir(confirmRemove);
      if (result.success) await refresh();
      else appError.set(result.error || 'Failed to remove heir');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    confirmRemove = null; confirmRemoveLabel = '';
  }
</script>

<div class="screen">
  <h1>Heirs</h1>
  <p class="subtitle">Add the people who will inherit your Bitcoin.</p>

  {#if heirs.length > 0}
    <div class="delivery-summary">
      <span class="badge-success">📨 {heirsWithNpub.length} NIP-17 ready</span>
      {#if heirsWithoutNpub.length > 0}
        <span class="badge-warning">📋 {heirsWithoutNpub.length} manual only</span>
      {/if}
    </div>

    <div class="heir-list">
      {#each heirs as heir}
        <HeirCard {heir} onRemove={requestRemove} />
      {/each}
    </div>
  {:else}
    <div class="card empty">
      <p>No heirs added yet. Add at least one heir before creating a vault.</p>
    </div>
  {/if}

  <div class="card form">
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
      <span>Nostr npub <span class="optional">(optional)</span></span>
      <input type="text" bind:value={npubInput} placeholder="npub1..." class:input-error={npubError} />
      {#if npubError}<span class="field-error">{npubError}</span>{/if}
    </label>

    <div class="actions">
      <button class="btn btn-outline" onclick={handleAdd} disabled={loading}>
        {loading ? 'Adding...' : '+ Add Heir'}
      </button>
      {#if heirs.length > 0}
        <button class="btn btn-primary" onclick={() => navigate('vault')}>Next: Create Vault →</button>
      {/if}
    </div>
  </div>

  {#if confirmRemove}
    <ConfirmDialog
      title="Remove Heir"
      message="Remove {confirmRemoveLabel} from your heir list?"
      detail="If you've already created a vault, you'll need to recreate it without this heir."
      confirmLabel="Remove"
      onConfirm={confirmRemoveHeir}
      onCancel={() => confirmRemove = null}
    />
  {/if}
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 0.25rem; }
  h2 { font-size: 1.2rem; margin-top: 0; }
  .subtitle { color: var(--text-muted); margin-bottom: 1.5rem; }

  .delivery-summary { display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 1rem; }

  .empty { text-align: center; color: var(--text-muted); }

  .form { display: flex; flex-direction: column; gap: 1rem; margin-top: 1rem; }

  label { display: flex; flex-direction: column; gap: 0.35rem; }
  label span { font-size: 0.85rem; color: var(--text-muted); font-weight: 500; }
  .optional { font-weight: 400; }
  .field-error { font-size: 0.8rem; color: var(--error); font-weight: 400; }
  .input-error { border-color: var(--error) !important; }

  .heir-list { display: flex; flex-direction: column; gap: 0.5rem; margin-bottom: 1rem; }

  .actions { display: flex; gap: 1rem; margin-top: 0.5rem; }
</style>
