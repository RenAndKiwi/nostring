<script lang="ts">
  let { step, secondsRemaining }: {
    step: number;
    secondsRemaining: number;
  } = $props();

  const timerWarning = $derived(secondsRemaining <= 600);
  const timerCritical = $derived(secondsRemaining <= 120);
  const timerMinutes = $derived(Math.floor(secondsRemaining / 60));
  const timerSeconds = $derived(secondsRemaining % 60);
  const timerDisplay = $derived(`${timerMinutes}:${timerSeconds.toString().padStart(2, '0')}`);
</script>

<div class="progress-bar">
  <div class="progress-steps">
    <div class="step-dot" class:active={step >= 1} class:current={step === 1}>1</div>
    <div class="step-line" class:active={step >= 2}></div>
    <div class="step-dot" class:active={step >= 2} class:current={step === 2}>2</div>
    <div class="step-line" class:active={step >= 3}></div>
    <div class="step-dot" class:active={step >= 3} class:current={step === 3}>3</div>
  </div>
  <div class="progress-labels">
    <span>Nonces</span>
    <span>Sign</span>
    <span>Broadcast</span>
  </div>
  <div class="session-timer" class:timer-warning={timerWarning} class:timer-critical={timerCritical}>
    Session expires in {timerDisplay}
  </div>
</div>

<style>
  .progress-bar {
    margin-bottom: 1.5rem;
    padding: 1rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .progress-steps { display: flex; align-items: center; justify-content: center; }
  .step-dot {
    width: 28px; height: 28px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    font-size: 0.8rem; font-weight: 700;
    background: var(--surface-variant); color: var(--text-muted);
    transition: all 0.2s;
  }
  .step-dot.active { background: var(--gold-light); color: #000; }
  .step-dot.current { box-shadow: 0 0 0 3px rgba(251, 220, 123, 0.3); }
  .step-line { width: 60px; height: 2px; background: var(--surface-variant); transition: background 0.2s; }
  .step-line.active { background: var(--gold-light); }
  .progress-labels {
    display: flex; justify-content: space-between;
    padding: 0 0.5rem; margin-top: 0.5rem;
    font-size: 0.75rem; color: var(--text-muted);
  }
  .session-timer {
    text-align: center; margin-top: 0.5rem;
    font-size: 0.8rem; color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .session-timer.timer-warning { color: var(--gold-light); }
  .session-timer.timer-critical { color: var(--error); font-weight: 600; }
</style>
