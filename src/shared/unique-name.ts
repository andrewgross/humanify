/**
 * Case-folding name disambiguation, shared by every place that writes files
 * into the split output tree (the clustered app/vendor assignment and the
 * Bun unpack adapter's factory extraction). A case-INSENSITIVE filesystem
 * (macOS, Windows) collapses two names that differ only in case, so the
 * single source of truth for "make this name unique" must fold case.
 */

/**
 * Return `stem` (plus optional `ext`) made unique among the names already in
 * `usedLower` UNDER CASE-FOLDING, appending `-2`, `-3`, … before the
 * extension on a collision (`foo.js` → `foo-2.js`). Records the chosen name's
 * lowercase form in `usedLower`, so `usedLower` must be a set of lowercased
 * names shared across all calls for one directory.
 */
export function uniqueCaseInsensitiveName(
  stem: string,
  usedLower: Set<string>,
  ext = ""
): string {
  let name = `${stem}${ext}`;
  for (let k = 2; usedLower.has(name.toLowerCase()); k++) {
    name = `${stem}-${k}${ext}`;
  }
  usedLower.add(name.toLowerCase());
  return name;
}
