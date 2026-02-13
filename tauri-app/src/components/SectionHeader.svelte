<script lang="ts">
  let { title, subtitle = '' }: { title: string; subtitle?: string } = $props();
</script>

<div class="section-header">
  <h2>{title}</h2>
  {#if subtitle}
    <p>{subtitle}</p>
  {/if}
</div>

<style>
  .section-header {
    margin-bottom: 16px;
  }
  h2 {
    font-size: 20px;
    font-weight: 700;
    color: #FFFFFF;
    margin: 0;
  }
  p {
    font-size: 14px;
    color: #9CA3AF;
    margin: 4px 0 0;
  }
</style>
