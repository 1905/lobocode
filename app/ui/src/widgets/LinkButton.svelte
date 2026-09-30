<script lang="ts">
  let {
    label,
    tone = 'dim',
    disabled = false,
    onclick,
  }: {
    label: string;
    tone?: string;
    disabled?: boolean;
    onclick: () => void;
  } = $props();
</script>

<button type="button" {disabled} {onclick} style:color={`var(--${tone})`}
  >{label}</button
>

<style>
  button {
    background: none;
    border: 0;
    padding: 0;
    font-size: 10px;
    white-space: nowrap;
  }
  button:hover:not(:disabled) {
    color: var(--text) !important;
  }
  button:disabled {
    color: var(--faint) !important;
  }
</style>
