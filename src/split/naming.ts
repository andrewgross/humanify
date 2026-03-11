import type { Cluster } from "./types.js";

/** Minimum length for a common prefix to be considered meaningful. */
const MIN_PREFIX_LENGTH = 4;

/** Names shorter than this are considered minified-looking. */
const MIN_NAME_LENGTH = 3;

/**
 * Generate a mechanical file name for a cluster.
 *
 * Strategy:
 * 1. Single root with a humanified name → use that name
 * 2. Multiple roots → try common prefix, else join names
 * 3. Fallback → mod_<fingerprint>.js
 */
export function nameCluster(
  cluster: Cluster,
  functionNames: Map<string, string>
): string {
  const rootNames = cluster.rootFunctions
    .map(id => functionNames.get(id))
    .filter((name): name is string => !!name && name.length >= MIN_NAME_LENGTH);

  if (rootNames.length === 0) {
    return `mod_${cluster.id}.js`;
  }

  if (rootNames.length === 1) {
    return `${rootNames[0]}.js`;
  }

  // Try common prefix
  const prefix = commonPrefix(rootNames);
  if (prefix.length >= MIN_PREFIX_LENGTH) {
    // Strip trailing uppercase (partial camelCase word)
    const cleaned = prefix.replace(/[A-Z][a-z]*$/, "").replace(/[_-]$/, "");
    if (cleaned.length >= MIN_PREFIX_LENGTH) {
      return `${cleaned}.js`;
    }
  }

  // Join names (truncate if too many)
  const joined = rootNames.slice(0, 3).join("_");
  return `${joined}.js`;
}

function commonPrefix(strings: string[]): string {
  if (strings.length === 0) return "";
  let prefix = strings[0];
  for (let i = 1; i < strings.length; i++) {
    while (!strings[i].startsWith(prefix)) {
      prefix = prefix.slice(0, -1);
      if (prefix.length === 0) return "";
    }
  }
  return prefix;
}
