import ms from "ms";

export function formatDuration(milliseconds: number): string {
  return ms(milliseconds);
}

export function parseDuration(str: string): number {
  return ms(str);
}
