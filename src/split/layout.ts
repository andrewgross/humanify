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
 *   │   ├── _bundle.js                      shared wrapper-context runtime
 *   │   └── __bun-runtime.js                Bun factory helpers (re-link)
 *   ├── src/                                humanified app code
 *   └── vendor/                             vendored libraries, untouched
 */

/** Folder holding the humanified app code (the nested split tree). */
export const CODE_DIR = "src";

/** Folder holding vendored libraries (Bun CJS factories), one file each. */
export const VENDOR_DIR = "vendor";

/** Folder holding generated metadata and runtime shims — artifacts a
 * reader never reviews. */
export const METADATA_DIR = ".humanify";

/** The split ledger's path within the output tree. */
export const SPLIT_LEDGER_PATH = `${METADATA_DIR}/split-ledger.json`;

/** Pre-.humanify ledger filename; still discovered next to --prior-version
 * so an older output can seed cross-release inheritance. */
export const LEGACY_SPLIT_LEDGER_FILENAME = "_split-ledger.json";
