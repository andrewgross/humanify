/**
 * What an emitted tree looks like on disk — the facts the harness reads it
 * by. Moved from the TS pipeline (src/split/layout.ts, src/file-utils.ts and
 * the ledger type in src/split/stable-split.ts) at the cutover
 * (docs/rust-port/19-cutover.md). The Rust binary writes the same layout; its
 * ledger schema is `humanify_model`'s split-ledger type, and this interface
 * is the harness's read view of that file.
 */
import { readdirSync } from "node:fs";
import * as path from "node:path";

/** The per-tree metadata directory (ledger, bundle, stats). */
export const METADATA_DIR = ".humanify";

/**
 * Recursively list JS files under `dir`, returning paths relative to
 * `rootDir` (defaults to `dir`). Skips `node_modules` and `.humanify/`
 * (pipeline metadata: walking it double-counts every emitted file).
 */
export function listJsFilesRecursive(
  dir: string,
  rootDir: string = dir
): string[] {
  const results: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === METADATA_DIR) {
        continue;
      }
      results.push(...listJsFilesRecursive(fullPath, rootDir));
    } else if (entry.name.endsWith(".js")) {
      results.push(path.relative(rootDir, fullPath));
    }
  }
  return results;
}

/**
 * The persisted split ledger (`.humanify/split-ledger.json`) — the
 * cross-release memory. `nameToFiles` holds, per declared name, the ORDERED
 * file list of its declaration occurrences; `order` holds each wrapper-body
 * statement's file, in statement order.
 */
export interface SplitLedger {
  version: 1;
  files: string[];
  nameToFiles: Record<string, string[]>;
  order: string[];
  /** Rename-invariant statement hash per statement, BUNDLE order. */
  hashes?: string[];
  /** The same hashes in EMITTED (slot) order — the layout on disk. */
  emitHashes?: string[];
  /** Declared name per emitted slot (`null` = nothing nameable). */
  emitNames?: (string | null)[];
  /** The bundle statement index emitted into each slot. */
  emitIndexes?: number[];
  /** Statement-hash version the hashes were computed under. */
  hashVersion?: number;
  /** file → the `require` alias the runnable emit gave it. */
  aliases?: Record<string, string>;
  /** The fossil modules this release emitted (exp070). */
  fossilModules?: FossilLedgerModule[];
}

/** One fossil module as the ledger records it — see `fossilModules`. */
export interface FossilLedgerModule {
  file: string;
  hashes: string[];
  imports: number[];
  /** Names the module declares, post-rename (exp080). */
  declared?: string[];
  /** Graded shape tokens (exp078), hashed short. */
  tokens?: string[];
}
