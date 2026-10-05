<script lang="ts">
  import LinkButton from '../widgets/LinkButton.svelte';
  let { action }: { action: (name: string) => void } = $props();
</script>

<footer class="row">
  <LinkButton
    label="settings ⌘,"
    onclick={() => action('settings')}
  /><LinkButton label="config" onclick={() => action('reveal')} /><LinkButton
    label="logs"
    onclick={() => action('logs')}
  /><span class="grow"></span><LinkButton
    label="quit ⌘q"
    onclick={() => action('quit')}
  />
</footer>

<style>
  footer {
    gap: 14px;
    padding: 9px 14px;
    background: var(--card);
    border-top: 1px solid var(--line);
  }
</style>
