# exp095 / exact-module-hash

Whole-module fingerprints of npm library modules, measured on the shared lab (`../lab`). The findings are in
`REPORT.md`. Nothing in the humanify repo was edited. `fphash` compiles humanify-core as a path dependency,
into its own `target/`.

## What is here

| path                                        | what                                                                                                                                                                                                                                                                |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fphash/`                                   | Rust harness. It hashes JS spans with **humanify's own hash code** (`humanify_core::hash`): the structural hash (MatchKey), the Verbatim variant and the statement hash. It also has its own normaliser, `src/norm.rs`. Input and output are JSONL on stdin/stdout. |
| `fphash/src/norm.rs`                        | The "minifier-normal form": about 25 rewrite rules over the ESTree JSON, each one taken from a real diff in the lab.                                                                                                                                                |
| `scripts/load.py`                           | Joins `lab/labels/all.jsonl` with the hashes into `data/rows.pkl`.                                                                                                                                                                                                  |
| `scripts/stability.py`                      | Does the same file get the same hash under another bundler, bundler version, minification or entry mode?                                                                                                                                                            |
| `scripts/uniq.py`                           | Collisions between different files, by size, and index size per bundler group.                                                                                                                                                                                      |
| `scripts/drift.py`                          | The same file across patch, minor and major versions.                                                                                                                                                                                                               |
| `scripts/indexsize.py`                      | Hashes per package version, growth per added version, and the bytes a packed index takes.                                                                                                                                                                           |
| `scripts/extrapolate.py`                    | Index size for the npm top-15,916 (`npm-high-impact`), from registry metadata.                                                                                                                                                                                      |
| `scripts/cc_jobs.py`, `scripts/cc_match.py` | Every Claude Code 2.1.198 / 2.1.216 module looked up in the lab index; negatives and positives.                                                                                                                                                                     |
| `scripts/combo_holdout.py`                  | Index from the single-package apps, tested on the 6 combo apps, using the labels' module boundaries.                                                                                                                                                                |
| `scripts/realistic_seg.py`                  | The same test, but the combo bundles are cut up WITHOUT labels (`lab/scripts/cc-layout.mjs`), as a real bundle must be.                                                                                                                                             |
| `scripts/eager_parts.py`                    | How often eager ES modules are split into pieces, tree-shaken, or lose their `__esm` marker.                                                                                                                                                                        |
| `hljs/`                                     | `require('highlight.js')` built with Bun 1.4.0 for 11 highlight.js versions, used for recall on Claude Code's 188 grammar factories.                                                                                                                                |
| `npmtop/`                                   | `npm-high-impact` (the top-download list) and abbreviated registry metadata for 1,500 sampled packages and the 176 lab packages.                                                                                                                                    |
| `seg/`                                      | Label-free layouts of the combo bundles.                                                                                                                                                                                                                            |
| `results/*.txt / *.json`                    | Every number in REPORT.md.                                                                                                                                                                                                                                          |

## Re-run everything

```bash
cd /work/exp095-library-fingerprints/exact-module-hash
./run_all.sh          # ~6 min on 64 cores; step 6 needs network (npm registry)
```

The individual steps, as `run_all.sh` runs them:

```bash
export PATH="$HOME/.cargo/bin:$PATH"; (cd fphash && cargo build --release)
fphash/target/release/fphash < data/lab-jobs.jsonl > data/lab-hash.jsonl    # 321,619 spans, ~90 s
python3 scripts/srcsha.py
python3 scripts/cc_jobs.py && for v in 2.1.198 2.1.216; do fphash/target/release/fphash < data/cc-$v-jobs.jsonl > data/cc-$v-hash.jsonl; done
python3 scripts/stability.py; python3 scripts/uniq.py; python3 scripts/drift.py; python3 scripts/indexsize.py
python3 scripts/cc_match.py; python3 scripts/combo_holdout.py; python3 scripts/realistic_seg.py; python3 scripts/extrapolate.py
```

`run_all.sh` also adds a raw `sha` field to the cc hash rows; `cc_match.py` needs it for the raw-text column.

## fphash job format

```json
{
  "id": 1,
  "path": "/abs/span.js",
  "kind": "cjs|lazy-esm|eager-esm|other",
  "bundle": "/abs/out.js"
}
```

- `bundle` is optional. It names the bundle the span came from, so that a span's free reference to a
  bundle-level name is masked even when the name happens to be a known global (Bun minified a module to `gc`).
- `"parts": true` prints the canonical token stream of the normalised core to stderr.
- `"dump": true` prints the normalised ESTree to stderr.

Output fields, all 16 hex digits:

- **Full span:** `fullStmt`, `fullM`, `fullV`.
- **Wrapper stripped:** `coreStmt`, `coreM`, `coreV`, `coreSetStmt`, `coreSetM`.
- **Wrapper stripped and normalised:** `normStmt`, `normM`, `normV`, `normSetStmt`, `normSetM`.
- **CJS only:** `factM`, which equals the vendor manifest's `structuralHash` (checked: 209/214 positives).
- **Sizes:** `normLen` / `coreLen`, the length of the canonical stream (used as the size floor).
