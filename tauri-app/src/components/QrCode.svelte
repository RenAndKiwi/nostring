<script lang="ts">
  import { onMount } from 'svelte';
  import QRCode from 'qrcode';

  let { data, size = 300 }: { data: string; size?: number } = $props();

  let canvas: HTMLCanvasElement;
  let error = $state('');

  async function render() {
    if (!canvas || !data) return;
    error = '';
    try {
      await QRCode.toCanvas(canvas, data, {
        width: size,
        margin: 2,
        color: { dark: '#000000', light: '#FFFFFF' },
        errorCorrectionLevel: 'L',
      });
    } catch (e: any) {
      error = e.message || 'QR generation failed';
    }
  }

  onMount(() => { render(); });
  $effect(() => { data; render(); });
</script>

<div class="qr-container">
  {#if error}
    <p class="qr-error">QR failed: {error}. Copy the backup JSON instead.</p>
  {:else}
    <canvas bind:this={canvas}></canvas>
  {/if}
</div>

<style>
  .qr-container {
    display: flex; justify-content: center;
    padding: 1rem; background: #fff;
    border-radius: var(--radius); margin: 1rem 0;
  }
  canvas { border-radius: 4px; }
  .qr-error { color: var(--error); font-size: 0.85rem; text-align: center; margin: 0; }
</style>
