<script lang="ts">
  let {
    label,
    tone = 'green',
    wide = false,
    disabled = false,
    onclick,
  }: {
    label: string;
    tone?: string;
    wide?: boolean;
    disabled?: boolean;
    onclick: () => void;
  } = $props();
</script>

<button
  type="button"
  {disabled}
  {onclick}
  class:wide
  style:--tone={`var(--${tone})`}>[ {label} ]</button
>

<style>
  button {
    padding: 7px 10px;
    border: 1px solid color-mix(in srgb, var(--tone) 70%, transparent);
    border-radius: 4px;
    background: color-mix(in srgb, var(--tone) 6%, transparent);
    color: var(--tone);
    font-weight: 700;
    transition:
      background 0.12s,
      color 0.12s,
      box-shadow 0.12s;
  }
  button:hover:not(:disabled) {
    background: var(--tone);
    color: var(--bg);
    box-shadow: 0 0 8px color-mix(in srgb, var(--tone) 45%, transparent);
  }
  button:active:not(:disabled) {
    background: color-mix(in srgb, var(--tone) 25%, var(--bg));
  }
  button:disabled {
    color: var(--faint);
    border-color: var(--faint);
  }
  .wide {
    width: 100%;
  }
</style>
