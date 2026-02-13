<script lang="ts">
  import type { DeliveryReport } from '../lib/tauri';

  let { report }: { report: DeliveryReport } = $props();
</script>

<div class="report">
  {#if report.delivered.length > 0}
    <div class="report-section success-box">
      <h3>✅ Delivered</h3>
      <div class="tags">
        {#each report.delivered as heir}
          <span class="tag">{heir}</span>
        {/each}
      </div>
    </div>
  {/if}

  {#if report.skipped.length > 0}
    <div class="report-section warning-box">
      <h3>⏭️ Skipped (no npub)</h3>
      <div class="tags">
        {#each report.skipped as heir}
          <span class="tag">{heir}</span>
        {/each}
      </div>
      <p class="help">Export the backup manually for these heirs.</p>
    </div>
  {/if}

  {#if report.failed.length > 0}
    <div class="report-section error-box">
      <h3>❌ Failed</h3>
      {#each report.failed as f}
        <div class="fail-item">
          <span class="tag">{f.heir_label}</span>
          <span class="fail-reason">{f.error}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .report { display: flex; flex-direction: column; gap: 1rem; margin-bottom: 2rem; }
  .report-section { border-radius: var(--radius); padding: 1rem; }
  .report-section h3 { margin: 0 0 0.5rem; font-size: 0.95rem; }
  .tags { display: flex; flex-wrap: wrap; gap: 0.25rem; }
  .tag {
    display: inline-block; background: var(--surface-variant);
    padding: 0.25rem 0.5rem; border-radius: var(--radius-sm);
    font-size: 0.85rem;
  }
  .help { font-size: 0.8rem; color: var(--text-muted); margin: 0.5rem 0 0; }
  .fail-item { margin: 0.5rem 0; }
  .fail-reason { color: var(--text-muted); font-size: 0.8rem; margin-left: 0.5rem; }
</style>
