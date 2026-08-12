/**
 * Relative-import path computation shared by the runnable-tree emitters
 * (cjs-emit.ts, bun-relink.ts). Sole survivor of the legacy clustering
 * splitter's emitter, which was deleted 2026-08-12 along with the rest of
 * the second split path.
 */
/**
 * Compute the relative import path from one output file to another.
 * Both paths are relative to the output root (e.g., "src/app.js", "lib/shared.js").
 */
export function computeRelativeImportPath(
  fromFile: string,
  toFile: string
): string {
  const fromDir = fromFile.includes("/")
    ? fromFile.slice(0, fromFile.lastIndexOf("/"))
    : "";
  const toDir = toFile.includes("/")
    ? toFile.slice(0, toFile.lastIndexOf("/"))
    : "";
  const toBasename = toFile.includes("/")
    ? toFile.slice(toFile.lastIndexOf("/") + 1)
    : toFile;

  if (fromDir === toDir) {
    return `./${toBasename}`;
  }

  // Compute relative traversal between directories
  const fromParts = fromDir ? fromDir.split("/") : [];
  const toParts = toDir ? toDir.split("/") : [];

  // Find common prefix length
  let common = 0;
  while (
    common < fromParts.length &&
    common < toParts.length &&
    fromParts[common] === toParts[common]
  ) {
    common++;
  }

  const ups = fromParts.length - common;
  const downs = toParts.slice(common);
  const segments = [
    ...Array.from({ length: ups }, () => ".."),
    ...downs,
    toBasename
  ];

  const rel = segments.join("/");
  // Only ./ and ../ mark a specifier as relative — a bare `.humanify/…`
  // would resolve as a package name.
  return rel.startsWith("../") ? rel : `./${rel}`;
}
