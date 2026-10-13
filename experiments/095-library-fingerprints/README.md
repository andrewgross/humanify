# exp095: library fingerprints (could we ship an index of npm library modules?)

> **In the repo: scripts and write-ups only.** The lab data (installed packages, builds, labels, indexes; about 20 GB) lives in `/work/exp095-library-fingerprints/` on the experiment box and is not committed. Claude Code extracts are never committed. Scripts reference `/work/...` paths as they ran.

## STATUS (2026-10-11)

- **State.** EXPLORATION COMPLETE and verified. No pipeline change was made: the humanify repo is untouched and nothing is committed.
- **Verdict.**
  - **Naming vendor files from a fingerprint index: viable,** with guards (two kinds of evidence, stop-lists, package families, a size floor).
  - **Finding library code inside `src/` (Step B): viable as a candidate list only.** Every method put a library name on some real Claude Code app code once tested on app code it was not tuned on: 0–2.6% of claims.
  - **Index size is small** (about 5–30 MB for the top 1,000 packages). Covering the exact versions apps ship is what decides recall.
- **Read first:** `SYNTHESIS.md`, which has the direct answers, the design and build order, and the next experiments.
- **Caveat (rule 9).** The approach folders' own READMEs and results state the ORIGINAL agents' numbers. Several were corrected by the verifiers (see `SYNTHESIS.md` and each `VERIFY.md` / `verify/`). Where they disagree, the verifier's number wins.
- **Thresholds are not final.** Every threshold was set on a lab of 176–347 packages and must be re-derived at shipping index size (SYNTHESIS §5, item 1).

## Background (outside this folder)

- `/work/library-identity-2026-10-10/bun-layout.md`: how Bun and esbuild emit modules, and what survives minification.
- `/work/library-identity-2026-10-10/declared-info.md`: a census of declared identity in Claude Code.

## Folders

| Folder                              | What it is                                                                                                                                                                                                                                                                                                         | Verifier write-up                                                                                     |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| `lab/`                              | The shared lab: 176 packages at 315 versions; 5 bundler setups (Bun 1.3.14 / 1.4.0 / 1.4.3, esbuild 0.28.2, rollup+terser), minified and not; 6,184 builds; 321,619 sourcemap-labelled spans (`labels/all.jsonl`); 6 multi-library combo apps; Claude Code positives and negatives (`realworld/`)                  | —                                                                                                     |
| `exact-module-hash/`                | Whole-module exact hashes using humanify's own hash code (`fphash/`, a Rust crate with a path dependency on humanify-core) plus a minifier-normal form (`fphash/src/norm.rs`); index-size extrapolation (`npmtop/`); highlight.js version pinning (`hljs/`); label-free segmentation (`seg/`). Run: `./run_all.sh` | `verify/` (fresh 1,221-package corpus, collision growth; no VERIFY.md)                                |
| `subtree-hashes/`                   | Per-statement, function and class unit hashes, names masked, with a loose normaliser (`scripts/canon.mjs`); a second lab of 18 CC libraries × up to 40 releases (`rwlab/`); CC scans and version guesses                                                                                                           | `VERIFY.md`, `verify/` (82 foreign packages, unseen Bun and esbuild versions)                         |
| `ngram-minhash/`                    | 5-token shingles over six token streams, Jaccard and containment, MinHash and LSH, sliding-window scans of CC (`sim.py`, `scan.py`, `minhash.py`)                                                                                                                                                                  | `verify/` (honest leave-one-package-out, `lopo.py`; no VERIFY.md)                                     |
| `immutable-literals/`               | IDF-weighted literal sets built from npm SOURCE (`scripts/lits.mjs`); per-statement Step B; the Part A reader census                                                                                                                                                                                               | `VERIFY.md`, `verify/` (75 out-of-index packages, unseen versions, audit of all 686 CC `src/` claims) |
| `bundler-variation-and-boundaries/` | A 33-rule canonical form with per-rule ablation (`scripts/canon.mjs`, `ablate.mjs`); variation across bundlers, modes and versions (`variation.py`); the Step B detector (`stepb.py`); exact-version MSAL ground truth (`exactver/`)                                                                               | `VERIFY.md`, `verify/` (held-out CC app code, version drift, esbuild-only index)                      |

Exact re-run commands are in each folder's `README.md` (and the verifiers' `verify/README.md` where present).

## Tools

- bun 1.3.14 at `~/.bun/bin/bun`.
- Bun 1.4.0 and 1.4.3, esbuild, rollup and terser are under `lab/tools/`.
- Python scripts that hash token sets need `PYTHONHASHSEED=0`.
- Wait on a PID, never on `pgrep -f`.
