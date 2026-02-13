<script lang="ts">
  let { title, message, detail = '', confirmLabel = 'Confirm', loading = false, onConfirm, onCancel }: {
    title: string;
    message: string;
    detail?: string;
    confirmLabel?: string;
    loading?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();
</script>

<div class="overlay" role="dialog">
  <div class="dialog">
    <h3>{title}</h3>
    <p>{message}</p>
    {#if detail}
      <p class="detail">{detail}</p>
    {/if}
    <div class="actions">
      <button class="btn btn-primary" onclick={onConfirm} disabled={loading}>
        {loading ? 'Processing...' : confirmLabel}
      </button>
      <button class="btn btn-outline" onclick={onCancel}>Go Back</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex; align-items: center; justify-content: center;
    z-index: 200;
  }
  .dialog {
    background: var(--surface); border: 1px solid #444;
    border-radius: var(--radius-lg); padding: 1.5rem;
    max-width: 420px; width: 90%;
  }
  h3 { margin-top: 0; color: var(--text); }
  p { color: var(--text-muted); line-height: 1.5; }
  .detail { font-size: 0.85rem; }
  .actions { display: flex; gap: 1rem; margin-top: 1rem; }
</style>
