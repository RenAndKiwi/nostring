<script lang="ts">
  let { label, content, onCopy }: {
    label: string;
    content: string;
    onCopy: (text: string, label: string) => void;
  } = $props();
</script>

<div class="code-header">
  <span class="code-label">{label}</span>
  <button class="copy-btn" onclick={() => onCopy(content, label)}>Copy</button>
</div>
<pre class="code-block">{content}</pre>

<style>
  .code-header {
    display: flex; justify-content: space-between; align-items: center;
    margin-top: 0.5rem;
  }
  .code-label { font-size: 0.8rem; color: var(--text-muted); font-weight: 500; }
  .copy-btn {
    background: var(--surface-variant); border: 1px solid #444;
    border-radius: var(--radius-sm); padding: 0.25rem 0.5rem;
    color: var(--text); font-size: 0.75rem; cursor: pointer;
    transition: all 0.15s;
  }
  .copy-btn:hover { border-color: var(--gold-light); }
  .code-block {
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 6px; padding: 0.75rem;
    font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.8rem;
    overflow-x: auto; max-height: 200px; overflow-y: auto;
    margin: 0.25rem 0 1rem; user-select: all;
  }
</style>
