export function leftPad(s: string, n: number): string {
  while (s.length < n) s = ` ${s}`;
  return s;
}
