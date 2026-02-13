<script lang="ts">
  let { label, value, mono = false }: { label: string; value: string; mono?: boolean } = $props();
</script>

<div class="info-row">
  <span class="label">{label}</span>
  <span class="value" class:mono>{value}</span>
</div>

<style>
  .info-row {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 4px 0;
    gap: 12px;
  }
  .label {
    color: #9CA3AF;
    font-size: 13px;
    min-width: 100px;
    flex-shrink: 0;
  }
  .value {
    color: #FBDC7B;
    font-size: 13px;
    font-weight: 500;
    text-align: right;
    word-break: break-all;
  }
  .mono {
    font-family: monospace;
  }
</style>
