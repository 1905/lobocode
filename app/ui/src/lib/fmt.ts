export function duration(s: number): string {
  const t = Math.max(0, Math.round(Number.isFinite(s) ? s : 0));
  const pad = (n: number) => String(n).padStart(2, '0');
  return t >= 3600
    ? `${Math.floor(t / 3600)}:${pad(Math.floor(t / 60) % 60)}:${pad(t % 60)}`
    : `${Math.floor(t / 60)}:${pad(t % 60)}`;
}
export const gb = (bytes: number) => (bytes / 1e9).toFixed(1);
export function bar(frac: number, width: number): string {
  const n = Math.round(
    Math.min(1, Math.max(0, Number.isFinite(frac) ? frac : 0)) * width,
  );
  return '▓'.repeat(n) + '░'.repeat(width - n);
}
export const tps = (v?: number) =>
  v === undefined ? '—' : v.toFixed(v >= 100 ? 0 : 1);
export const usd = (v: number) => `$${v.toFixed(2)}`;
