<script lang="ts">
  import type { HeirInfo } from '../lib/tauri';

  let { heir, onRemove }: {
    heir: HeirInfo;
    onRemove: (fingerprint: string, label: string) => void;
  } = $props();
</script>

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
  <button class="btn-remove" onclick={() => onRemove(heir.fingerprint, heir.label)} title="Remove heir">✕</button>
</div>

<style>
  .heir-card {
    display: flex; align-items: center; justify-content: space-between;
    background: var(--surface); border: 1px solid var(--border);
    border-radius: var(--radius); padding: 0.75rem 1rem;
  }
  .heir-info { display: flex; align-items: center; gap: 0.75rem; }
  .heir-icon { font-size: 1.2rem; }
  .heir-name { font-weight: 500; display: block; }
  .heir-fp { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.75rem; color: var(--text-muted); display: block; }
  .heir-npub { font-size: 0.75rem; color: var(--success); display: block; }
  .heir-no-npub { font-size: 0.75rem; color: var(--text-muted); display: block; font-style: italic; }
  .btn-remove { background: none; border: none; color: var(--text-muted); cursor: pointer; font-size: 1.1rem; padding: 0.25rem; }
  .btn-remove:hover { color: var(--error); }
</style>
