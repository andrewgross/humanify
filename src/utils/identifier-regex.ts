/**
 * Whole-identifier matching for JS identifier names.
 *
 * `\b<name>\b` is wrong for minified names containing `$`: `$` is not a
 * regex word character, so `\b\$H\b` never matches `= $H +` (no boundary
 * between two non-word chars) and DOES match inside `a$H` (boundary
 * between `a` and `$`). Lookarounds over the identifier charset give the
 * correct boundary on both sides.
 */
export function identifierRegex(name: string, flags = ""): RegExp {
  const escaped = name.replace(/\$/g, "\\$");
  return new RegExp(`(?<![A-Za-z0-9_$])${escaped}(?![A-Za-z0-9_$])`, flags);
}
