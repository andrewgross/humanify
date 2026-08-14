export function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
export function kebab(s: string): string {
  return s.replace(/\s+/g, "-").toLowerCase();
}
