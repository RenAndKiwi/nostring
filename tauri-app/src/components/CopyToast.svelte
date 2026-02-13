<script lang="ts">
  let { message }: { message: string | null } = $props();
</script>

{#if message}
  <div class="copy-toast">Copied {message}!</div>
{/if}

<style>
  .copy-toast {
    position: fixed; top: 1rem; right: 1rem;
    background: #1a5c2e; border: 1px solid #2a8c4e;
    color: var(--text); padding: 0.5rem 1rem;
    border-radius: var(--radius); font-size: 0.85rem; z-index: 100;
  }
</style>
