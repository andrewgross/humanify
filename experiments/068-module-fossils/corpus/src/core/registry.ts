const entries = new Map<string, number>();

export function register(k: string): number {
  const n = (entries.get(k) ?? 0) + 1;
  entries.set(k, n);
  return n;
}
export function count(): number {
  return entries.size;
}
