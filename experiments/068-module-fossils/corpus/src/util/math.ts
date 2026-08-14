export const clamp = (n: number, lo: number, hi: number) =>
  Math.min(hi, Math.max(lo, n));
export const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);
