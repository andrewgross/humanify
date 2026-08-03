/**
 * Output-tree layout for a --split run: where humanified code, vendored
 * libraries, and generated metadata live inside the output directory.
 * These constants are the ONLY place the folder names are defined — the
 * cluster assignment, the Bun unpack adapter, the runnable emitter, and
 * the CLI orchestration all import them.
 *
 *   <outputDir>/
 *   ├── index.js / run.cjs / package.json   generated runnable harness
 *   ├── .humanify/                          generated metadata + runtimes
 *   │   ├── split-ledger.json               cross-release layout memory
 *   │   ├── humanified.js                   full single-file output; the
 *   │   │                                   next release's --prior-version
 *   │   ├── _bundle.js                      shared wrapper-context runtime
 *   │   └── __bun-runtime.js                Bun factory helpers (re-link)
 *   ├── src/                                humanified app code
 *   └── vendor/                             vendored libraries, untouched
 */

import fs from "node:fs";
import path from "node:path";

/** Folder holding the humanified app code (the nested split tree). */
export const CODE_DIR = "src";

/** Folder holding vendored libraries (Bun CJS factories), one file each. */
export const VENDOR_DIR = "vendor";

/** Folder holding generated metadata and runtime shims — artifacts a
 * reader never reviews. */
export const METADATA_DIR = ".humanify";

/** The split ledger's filename within the metadata folder. */
export const SPLIT_LEDGER_FILENAME = "split-ledger.json";

/** The split ledger's path within the output tree. */
export const SPLIT_LEDGER_PATH = `${METADATA_DIR}/${SPLIT_LEDGER_FILENAME}`;

/** The full single-file humanified output, written per release so the NEXT
 * release can point `--prior-version` at it (its `.code` is what the rename
 * reuse pass diffs against) and inherit the split ledger sitting beside it. */
export const HUMANIFIED_SOURCE_PATH = `${METADATA_DIR}/humanified.js`;

/**
 * Per-tier placement counts for the run, beside the ledger.
 *
 * The counts were computed for every split and rendered ONLY into a prose log
 * line ("10935 via hashes, 6 via identity preempts, ..."), so comparing where
 * statements landed across two runs meant grepping two multi-GB logs and
 * parsing English. They are structured data and belong on disk as such.
 *
 * Lives under `.humanify/`, which every tree walk and every diff skips
 * (`treeFiles`, `jsFilesUnder`, and the eval's src/vendor diffs), so recording
 * it cannot move a KPI.
 */
export const PLACEMENT_STATS_PATH = `${METADATA_DIR}/placement-stats.json`;

/** Pre-.humanify ledger filename; still discovered next to --prior-version
 * so an older output can seed cross-release inheritance. */
export const LEGACY_SPLIT_LEDGER_FILENAME = "_split-ledger.json";

/**
 * Discover a split ledger under directory `dir`, checking every layout a
 * release may have written it in: directly in the metadata folder itself
 * (`dir` IS `.humanify/`, e.g. beside a prior `humanified.js`), under a
 * `.humanify/` child (`dir` is the tree root), or the pre-.humanify flat
 * filename. Returns the first that exists, or undefined. This candidate
 * order is the ONLY place the ledger lineage is encoded.
 */
export function findSplitLedgerIn(dir: string): string | undefined {
  return [
    path.join(dir, SPLIT_LEDGER_FILENAME),
    path.join(dir, SPLIT_LEDGER_PATH),
    path.join(dir, LEGACY_SPLIT_LEDGER_FILENAME)
  ].find((candidate) => fs.existsSync(candidate));
}

/**
 * Discover the split ledger to inherit from, given the file `--prior-version`
 * points at (normally a prior release's `.humanify/humanified.js`, whose
 * ledger is its sibling). Resolves relative to that file's directory.
 */
export function findSplitLedgerPath(priorFile: string): string | undefined {
  return findSplitLedgerIn(path.dirname(priorFile));
}

/**
 * The ROOT of the tree `--prior-version` belongs to — the directory the split
 * ledger's relative paths resolve against, so a pass can read the prior text of
 * the file at the same path. `--prior-version` normally points at
 * `<root>/.humanify/humanified.js`, hence the step out of the metadata folder;
 * a flat pre-`.humanify` layout is its own root.
 *
 * Deliberately NOT `findPriorTreeRoot` from the Bun adapter, which answers a
 * narrower question — "where is the prior tree that has VENDORED BODIES" — by
 * requiring a vendor manifest. Reusing that here would make a source-level pass
 * silently no-op on any tree without vendored modules.
 */
export function splitTreeRootOf(priorFile: string): string {
  const dir = path.dirname(priorFile);
  return path.basename(dir) === METADATA_DIR ? path.dirname(dir) : dir;
}
