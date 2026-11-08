# exp202 — harness lane: the library remainder in Rust (layers, freezers, npm caches, stub batches)

STATUS: see the merge report at the bottom (2026-11-08), branch
`exp202-library-remainder`, rebased on main `a63341fe5` (L5 landed). Scope:
the library-harness pieces lane L2b (exp197) left in Python / TS / sh, so the
last 34 files L5 kept (experiments/203-harness-l5/README.md §4) can go. They
are deleted in this lane.

## Totals

- **Every truth-layer builder is ported, and all reproduce.** `cargo dev
  library layers <corpus>` now re-freezes all ten built layers (origins,
  origins7/10/12, chained, chained9/11, **copies2, misattr13, mapdefect14**):
  on every corpus whose data is on this host (bun-dev, bun-held,
  kind-esbuild, kind-webpack, kind-rollup, cc) **60 of 60 layer manifests and
  every per-entry `.json.gz` sha are identical** to the committed ones (§3).
- **mapdefect14's tokenizer is a port of acorn 8.19's tokenizer**
  (`library::acorn`: token contexts, regex-vs-division guess, templates,
  escapes, strict-mode octals, HTML comments, the regex fallback), with regex
  PATTERN validation by `oxc_regular_expression`. oxc's own lexer could not
  be used: it is not public and needs parser feedback for regexes, which a
  tokenizer-only stream does not have. **Token streams were compared with
  Node + acorn over every installed file the five source-mapped corpora read
  (275,197 files): 0 differ**, order included; plus 32 hand-written edge cases
  kept as a test (`testdata/tokens14-vectors.json`).
- **Corpus freezers:** `cargo dev library corpus freeze <corpus> --out DIR`
  re-freezes kind-esbuild, kind-webpack, kind-rollup, bun-dev, bun-held and cc
  from their inputs on this host; each manifest is identical to the committed
  one and the origin layer re-frozen from the new data reproduces (§4).
  `library corpus build` builds the bundles (esbuild / webpack / rollup / Bun)
  from the apps' installs (§4).
- **npm cache fillers:** `cargo dev library npm <corpus>` (needs, owners,
  registry manifests, tarball license collections). needs/owners equal the
  Python's on kind-rollup and bun-held; a 12-package fill into a scratch cache
  wrote 30 files, **30 / 30 byte-identical** to the existing cache (§5).
- **Stub-run batches:** `eval vendor-precision bun-dev`, `eval cjs-placement
  runs|trees` on L3's `run stub` (§6).
- **34 files deleted** (§1). No new scorer or layer version.

## 1. What was ported (old path -> verb) — all deleted in this lane

| Old                                                                                                                                                          | New (`cargo dev ...`, module)                                                                       |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------- |
| `129-helpers-copies/copies2.py` (hashes, rows, derive)                                                                                                       | `library layers <c> [--layers copies2] [--units L,..] [--index D]` (`library::copies2`)             |
| `132-factory-precision/scorer13/misattr13.py`                                                                                                                | `library layers <c> --layers misattr13` (`library::misattr13`)                                      |
| `137-scorer-mapdefect/scorer14/mapdefect14.py`, `tokens14.mjs` (acorn)                                                                                       | `library layers <c> --layers mapdefect14` (`library::{mapdefect14,acorn,tokens14}`); `library tokens14` answers tokens14.mjs's stdin protocol |
| `150-bun-testbed/scripts/layers.sh`, `layers2.sh`                                                                                                            | `library layers <c>` (build order `layers::BUILT`)                                                  |
| `096-library-matching-score/corpus.py` (freeze, freeze_cc, map_truth), `181-index-all-bundlers/scripts/kind_corpus.py` (build, freeze), `prep.sh`             | `library corpus freeze|build <c>` (`library::{freeze,corpus_build}`)                                |
| `100-test-split/scripts/build.py`, `095-library-fingerprints/selection/scripts/closure.py`                                                                   | `library::corpus_build` (build_esbuild, build_bun + bundle retry loop, install, entry)              |
| `112-test-precision/scorer7/origins7.py` (needs, owners; build/verify were L2b's), `bundled.py`, `104-truth-audit/fetch_metadata.py`                         | `library npm <c> [--npm-registry D] [--npm-bundled D] [--packages ..]` (`library::npm`)             |
| `166-restore-or-ask/bundev-precision.sh`, `stub-run.mts`, `vendor_precision.py`, `aggregate_precision.py` (the last two: L2b's port), `scripts/lib/stub-llm.ts` | `eval vendor-precision bun-dev <bin> <label>` (`runners::batch`), `run stub` (L3)                   |
| `172-cjs-app-placement/bundev-runs.sh` (and the already-deleted `trees.py`)                                                                                  | `eval cjs-placement runs <bin> <label> names|default`, `eval cjs-placement trees`                   |
| `096/{score,lib096,truth_audit,decoder,origins,families_lib}.py`, `095/misses/scripts/{mmlib,decompose,truth}.py`, `095/selection/scripts/libix.py`           | L2b's port (`library::*`); kept only as oracles until now                                          |
| `test/library-truth-{mapdefect14,origins7}.test.ts`                                                                                                          | `library::{tokens14,mapdefect14,...}_test.rs`                                                       |
| `100-test-split/scripts/freeze_test.py`, `webpack-build.cjs`                                                                                                 | retired: the `test` corpus's inputs are gone from this host (`/work/exp100-test-data`), and the kind corpora build with the index store's own recipe scripts |

Not ported: the dev corpus's freeze recipe (corpus.py `freeze` without
`--corpus`: its inputs, `/work/exp095-index-v2-data/eval`, are gone); an
Electron / VS Code app's INSTALL (its dependencies came from GitHub through
exp108's rule, whose code L5 deleted): `corpus build` refuses an uninstalled
non-CLI app, and every planned app is installed; bun_corpus.py's draw
(`plan` / `replace`, Python's seeded RNG — L5 deleted it; its `plan.json` is
the input).

## 2. How

- **copies2**: the Python's three steps in one call; the index's unit tables
  are streamed (`line[9:25]` as the hash, as the Python read them), the
  commonness bound applied once per table. Units read through the label's
  reports (`unitsFrom` by default), the index named by the committed manifest
  (`/work/library-index/<name>`, refused unless its manifest.json sha is the
  recorded one), the Audit built with no copies / misattr13 / mapdefect14
  (layers2.sh ran it with `RULES13`/`RULES14` off) — `truth::AuditLayers`.
- **misattr13**: the rule as written, the installed-file walk being
  `os.walk` with node_modules pruned and symlinks skipped.
- **mapdefect14**: the evidence (tokens14) in-process; tokens are WTF-8 bytes
  (Python's code-point order; a lone surrogate never matches decoded text and
  refuses the sha1 sample exactly where Python's `encode()` raised); the install
  tree walked as `os.walk(followlinks=True)` with each real dir / file once.
- **The acorn port** works on UTF-16 code units, reproduces `fullCharCodeAt`'s
  NaN cases, `strictDirective`, `-->` / `<!--` comments, the keyword-escape
  error raised on the NEXT token, and the regex fallback's 400-unit strings.
- **Freeze**: the corpus recipes read the same inputs as the Python (the
  plan / built records, the build outputs, the apps' installs, the cc sample
  under /work/cc-library-truth); text read with universal newlines as Python
  did; truth gz bytes via L2b's `gz`.
- **npm**: curl (with its own retry on a transient error; no sleeps) for the
  registry, `npm pack` + an in-process tar reader for the license
  collections; files already cached are never fetched again.

## 3. Proof (b'): the layers

`cargo dev library layers <corpus> --out /work/exp202-proof/final/layers/<corpus>`
on the final binary (see §8 for its sha): every cell is entries whose
re-frozen sha equals the committed manifest's / entries, and the manifest's
canonical identity.

FINAL_LAYERS_TABLE

Corpora not checked: dev, test, test2, bundlers, apps, pop, remin, remin2,
remin3, remin4 — their frozen bundles and truths are gone from this host
(L2b §3: `library corpus verify` lists every file missing), so no builder can
re-freeze them.

## 4. Freezers and builds

FINAL_FREEZE_TABLE

`library corpus build` (one app, `cli_cross-var-efce52`, into a scratch root
at the same directory depth as the originals): FINAL_BUILD_LINE An earlier
build into a deeper scratch dir gave identical `entry.js` for all 10 builds
(2 apps x esbuild, webpack, rollup, bun-node, bun-bun) and maps differing only
in the relative `sources` depth.

## 5. npm caches

| Check                                                                 | Result                         |
| --------------------------------------------------------------------- | ------------------------------ |
| `needs.json` / `owners.json` vs origins7.py needs/owners, kind-rollup | identical (1,058 names, 1,258 versions; 48 owners) |
| same, bun-held                                                        | identical (747 / 864; 30 owners) |
| fill 12 packages into a scratch cache vs `/work/exp112-data/{registry,bundled}` | 24 manifests + 6 license files written, **30 / 30 byte-identical** (the `@latest` ones included) |

No full refill was run (the brief forbids it in a proof).

## 6. Stub-run batches

FINAL_VP_LINE

## 7. Gate

FINAL_GATE_TABLE

## 8. What I could not prove

- The ten corpora whose data is gone (§3).
- `oxc_regular_expression` accepting exactly acorn's regex patterns is proven
  on the corpora's inputs (275,197 files, every unit of the 5 corpora through
  the layer shas), not in general; a future corpus could contain a pattern the
  two judge differently. The token differential (`library tokens14` speaks
  tokens14.mjs's protocol, plus a `dump` mode) is how to check one.
- `corpus build`'s npm install path (CLI apps) was not exercised: every app was
  already installed. The cjs-placement batch was tested by unit tests, not a
  full run.

## Proposed CLAUDE.md text (not applied)

> The library truth layers, corpus freezes, bundle builds and npm caches are
> `cargo dev library layers|corpus freeze|corpus build|npm`; every committed
> layer re-freezes byte-identically on the corpora this host still has
> (exp202).
