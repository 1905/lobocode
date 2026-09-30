<script lang="ts">
  let {
    text,
    color,
    settled = false,
  }: { text: string; color: string; settled?: boolean } = $props();
  let shown = $state('');
  $effect(() => {
    const value = text;
    if (settled || matchMedia('(prefers-reduced-motion: reduce)').matches) {
      shown = value;
      return;
    }
    let frame = 0;
    const noise = '!<>-_\\/[]{}=+*^?#%&01';
    const timer = setInterval(() => {
      frame++;
      shown = [...value]
        .map((c, i) =>
          c === ' ' || i < (value.length * frame) / 9
            ? c
            : noise[Math.floor(Math.random() * noise.length)],
        )
        .join('');
      if (frame >= 9) clearInterval(timer);
    }, 30);
    return () => clearInterval(timer);
  });
</script>

<strong style:color={`var(--${color})`} aria-label={text}
  >{shown || text}</strong
>

<style>
  strong {
    font-size: 11px;
  }
</style>
