<script lang="ts">
  let {
    label,
    options,
    value,
    onpick,
  }: {
    label: string;
    options: string[];
    value: string;
    onpick: (v: string) => void;
  } = $props();
</script>

<div class="row picker">
  <span class="dim">&gt; {label}</span>
  <div class="row options">
    {#each options as option}<button
        type="button"
        aria-pressed={value === option}
        onclick={() => onpick(option)}
        class:green={value === option}
        >{value === option ? `[${option}]` : ` ${option} `}</button
      >{/each}
  </div>
</div>

<style>
  .picker {
    gap: 0;
  }
  .picker > span {
    width: 84px;
    flex: none;
  }
  .options {
    gap: 8px;
    flex-wrap: wrap;
  }
  button {
    background: none;
    padding: 0;
    border: 0;
    color: var(--dim);
    white-space: pre;
  }
  button.green {
    color: var(--green);
  }
  button:hover {
    color: var(--text);
  }
</style>
