/**
 * Directory grouping for split output.
 *
 * When adapters produce module IDs that contain paths (like "src/helpers/util.ts"
 * from ESM detection), normalizes them into output directory paths.
 *
 * When adapters produce flat names (call-graph fallback), applies heuristic
 * grouping based on semantic prefixes extracted from camelCase names.
 */

export interface DirectoryGroupingOptions {
  /** Maximum directory nesting depth (default: 4). */
  maxDepth?: number;
}

const SPECIAL_FILES = new Set(["shared.js", "orphans.js", "index.js"]);

/**
 * Normalize adapter module IDs into output file paths.
 *
 * - Strips common directory prefix
 * - Changes .ts/.tsx extensions to .js
 * - Enforces max directory depth
 * - Preserves shared.js/orphans.js at root
 */
export function normalizeOutputPaths(
  moduleIds: Map<string, string>,
  options?: DirectoryGroupingOptions
): Map<string, string> {
  const maxDepth = options?.maxDepth ?? 4;
  const result = new Map<string, string>();

  // Separate special files from path-based files
  const pathEntries: Array<[string, string]> = [];
  for (const [fnId, moduleId] of moduleIds) {
    if (SPECIAL_FILES.has(moduleId)) {
      result.set(fnId, moduleId);
    } else {
      pathEntries.push([fnId, moduleId]);
    }
  }

  if (pathEntries.length === 0) return result;

  // Find and strip common prefix
  const paths = pathEntries.map(([, p]) => p);
  const prefix = findCommonPrefix(paths);
  const stripped = paths.map((p) => (prefix ? p.slice(prefix.length) : p));

  for (let i = 0; i < pathEntries.length; i++) {
    let outPath = stripped[i];

    // Normalize extension to .js
    outPath = normalizeExtension(outPath);

    // Enforce max depth
    outPath = enforceMaxDepth(outPath, maxDepth);

    result.set(pathEntries[i][0], outPath);
  }

  return result;
}

/**
 * Group flat output file names into directories by semantic prefix.
 *
 * Extracts a prefix from camelCase/PascalCase names (e.g., "create" from
 * "createStore.js") and groups files sharing a prefix (>= 2 files) into
 * a subdirectory named after the prefix.
 *
 * Special files (shared.js, orphans.js, index.js) are never grouped.
 */
/** Build a prefix -> entries map from candidates. Ungroupable entries go directly to result. */
function buildPrefixGroups(
  candidates: Array<[string, string]>,
  result: Map<string, string>
): Map<string, Array<[string, string]>> {
  const prefixMap = new Map<string, Array<[string, string]>>();
  for (const [fnId, fileName] of candidates) {
    const baseName = fileName.replace(/\.js$/, "");
    const prefix = extractSemanticPrefix(baseName);
    if (prefix) {
      const group = prefixMap.get(prefix) ?? [];
      group.push([fnId, fileName]);
      prefixMap.set(prefix, group);
    } else {
      result.set(fnId, fileName);
    }
  }
  return prefixMap;
}

/** Apply prefix groups: groups with >= 2 entries get a subdirectory, singletons stay flat. */
function applyPrefixGroups(
  prefixMap: Map<string, Array<[string, string]>>,
  result: Map<string, string>
): void {
  for (const [prefix, entries] of prefixMap) {
    const useDir = entries.length >= 2;
    for (const [fnId, fileName] of entries) {
      result.set(fnId, useDir ? `${prefix}/${fileName}` : fileName);
    }
  }
}

export function groupBySemanticPrefix(
  fileNames: Map<string, string>
): Map<string, string> {
  const result = new Map<string, string>();

  // Separate special files
  const candidates: Array<[string, string]> = [];
  for (const [fnId, fileName] of fileNames) {
    if (SPECIAL_FILES.has(fileName)) {
      result.set(fnId, fileName);
    } else {
      candidates.push([fnId, fileName]);
    }
  }

  const prefixMap = buildPrefixGroups(candidates, result);
  applyPrefixGroups(prefixMap, result);

  return result;
}

// ── Helpers ─────────────────────────────────────────────────────────

/** Find the common directory prefix of a list of paths. */
function findCommonPrefix(paths: string[]): string {
  if (paths.length === 0) return "";
  if (paths.length === 1) {
    const idx = paths[0].lastIndexOf("/");
    return idx >= 0 ? paths[0].slice(0, idx + 1) : "";
  }

  const parts = paths.map((p) => p.split("/"));
  const common: string[] = [];
  const minLen = Math.min(...parts.map((p) => p.length));

  for (let i = 0; i < minLen - 1; i++) {
    const segment = parts[0][i];
    if (parts.every((p) => p[i] === segment)) {
      common.push(segment);
    } else {
      break;
    }
  }

  return common.length > 0 ? `${common.join("/")}/` : "";
}

/** Normalize .ts/.tsx/.mts/.cts extensions to .js. */
function normalizeExtension(filePath: string): string {
  return filePath.replace(/\.(tsx?|mts|cts)$/, ".js");
}

/** Enforce max directory depth by collapsing deep paths. */
function enforceMaxDepth(filePath: string, maxDepth: number): string {
  const parts = filePath.split("/");
  if (parts.length - 1 <= maxDepth) return filePath;

  // Keep the first (maxDepth) directory segments + filename
  const dirs = parts.slice(0, maxDepth);
  const fileName = parts[parts.length - 1];
  return [...dirs, fileName].join("/");
}

/**
 * Extract a semantic prefix from a camelCase/PascalCase name.
 * Returns the lowercase prefix if it's >= 3 chars, else null.
 *
 * Examples:
 *   "createStore"  -> "create"
 *   "useSelector"  -> "use"
 *   "handleClick"  -> "handle"
 *   "main"         -> null (no camelCase boundary)
 *   "ab"           -> null (too short)
 */
function extractSemanticPrefix(name: string): string | null {
  // Find first uppercase letter after lowercase (camelCase boundary)
  const match = name.match(/^([a-z]{3,})[A-Z]/);
  if (match) return match[1];

  // PascalCase: first word before next uppercase
  const pascalMatch = name.match(/^([A-Z][a-z]{2,})[A-Z]/);
  if (pascalMatch) return pascalMatch[1].toLowerCase();

  return null;
}
