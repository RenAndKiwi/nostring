<script lang="ts">
  let { label, type = 'info' }: { label: string; type?: 'success' | 'warning' | 'error' | 'info' } = $props();

  const colorMap = {
    success: '#10B981',
    warning: '#F59E0B',
    error: '#EF4444',
    info: '#3B82F6',
  };

  let color = $derived(colorMap[type]);
</script>

<span class="badge" style="color: {color}; background: {color}15; border-color: {color}30;">
  {label}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 10px;
    border-radius: 16px;
    border: 1px solid;
    font-size: 12px;
    font-weight: 600;
  }
</style>
