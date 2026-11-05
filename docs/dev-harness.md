# The dev harness: one Rust crate for every measurement (design)

STATUS: DESIGN ONLY (exp185, 2026-11-05). Nothing here is built. Every number
below was read off main at `701d7631a` on 2026-11-05. Where this doc says
"verified", the claim was checked against the code or the data on this box.
Where it says "not verified", it was not.

Andrew's decisions (2026-11-05), the brief for this doc:

- The TypeScript, bash and Python measurement harness goes. The harness
  becomes pure Rust, in ONE committed, standardized, tested place.
- It is a separate workspace crate with its own binary, `humanify-ag-dev`
  (`publish = false`, never part of the release artifact, built optimised,
  run as `cargo dev ...`). The product crates are being renamed
  `humanify-*` -> `humanify-ag-*` by a parallel lane, so this doc uses the
  new names throughout.
- No living instrument reads code out of an old experiment folder any more.
  Living harness code under `experiments/` moves into the crate. Old
  experiments' CODE is deleted later, after a git tag. Their README and
  RESULTS files stay.
- The only JS/TS left afterwards is code that runs bundlers and tools
  (webpack / esbuild / rollup / bun / webcrack shims), never harness logic.
- The gate (`npm run check`, `scripts/check.ts`) becomes `cargo dev check`.

## 0. Totals

| What                                                                    |   Files |       Lines |
| ----------------------------------------------------------------------- | ------: | ----------: |
| `scripts/` (TS + sh, including `scripts/lib/stub-llm.ts`)               |      12 |       4,535 |
| `experiments/lib/**` (TS + sh)                                          |      36 |       8,737 |
| `experiments/034-eval-harness/*.ts` + `run.sh`                          |      28 |       8,166 |
| `experiments/096-library-matching-score/*.py` + `run.sh`                |      14 |       2,887 |
| `test/*.test.ts`                                                        |      12 |       2,560 |
| Code in OTHER experiment folders that a living piece imports or runs    |      17 |       3,795 |
| Library truth-layer builders + corpus freezers (run 2026-11-02, exp181) |      15 |       2,439 |
| Index-build orchestration (exp152 + exp181 scripts)                     |       6 |         309 |
| Version walk (`experiments/walk/`, outside the eval registry)           |       5 |       1,315 |
| **Total living harness**                                                | **145** | **~34,700** |

How it lands, in six lanes (section 5): a prerequisite lane (L0, record what
is not yet recorded), a skeleton lane (L1), then four lanes in parallel:
the eval scorer (L2a), the library scorer (L2b), the runners (L3), and the
gate, censuses and e2e (L4). Deletion and docs come last (L5).

There are four byte-identity proofs (section 4). Every one has its data on
this box today. Two of those data sets sit on volatile or easily confused
paths and must be copied first (L0):

- the reference TREES are only in `/tmp/eval-work/`, an overlay that does not
  survive a container rebuild;
- the current reference CARDS are only in `/work/exp171-names-default/...`.
  The main checkout's copies of `ref-scratch-0f338ffa-*` are the older
  2026-10-06 cards and differ (verified with `cmp`).

Of the 12 test-review findings: 10 are verified as stated, 1 is verified with
a correction (the stub servers), and 1 is NOT moot as claimed (the wrapper
spelling rule). See section 6.

## 1. Inventory

"Fate" says what replaces each piece: a `cargo dev` verb (section 2), a
SHIM (stays JS/sh, runs a tool, holds no harness logic), RETIRE (no living
caller: deleted with the tag), or DATA (moves, not ported).

### 1.1 The gate and its stages

| Piece                                                                  | Lines | What it does                                                                                                                                                              | Called by                              | Committed output a reference depends on                         | Fate                                                               |
| ---------------------------------------------------------------------- | ----: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------- | --------------------------------------------------------------- | ------------------------------------------------------------------ |
| `scripts/check.ts`                                                     |   238 | THE gate: 13 stages in order, fail-fast, prints which ran; `--only` = PARTIAL                                                                                             | `npm run check` / `check:all` / `test` | none (the stage list is the contract)                           | `check`                                                            |
| `scripts/clone-census.ts`                                              |   551 | Cross-file twin functions in the harness TS (Babel tokens), ADVISORY; holds its own allowlist                                                                             | check `census:clones`                  | allowlist (inline in the file)                                  | `census clones`, now over `crates/` (§6, §7)                       |
| `scripts/concurrency-census.ts`                                        |   735 | Every concurrency site in non-test `crates/**/*.rs` vs the allowlist; FAILS an unlisted or stale one                                                                      | check, pre-commit (lint-staged `*.rs`) | `scripts/concurrency-allowlist.json`                            | `census concurrency`                                               |
| `scripts/concurrency-allowlist.json`                                   |     — | The allowlist                                                                                                                                                             | the census                             | itself                                                          | DATA                                                               |
| `scripts/e2e.ts`                                                       | 1,133 | Release binary on `test/e2e/fixtures/*`: fresh, prior, determinism x2, Node boot (exact export surface), sequential vs `test/golden/legacy-default/`, known-gap mechanism | check `e2e`                            | `test/golden/legacy-default/**`, fixture `fixture.config.json`s | `e2e` (+ a boot-probe SHIM, §8)                                    |
| `scripts/lib/stub-llm.ts`                                              |   175 | Deterministic OpenAI-compatible stub (answers `<id>Renamed`)                                                                                                              | e2e, ask-trace, exp166 `stub-run.mts`  | the legacy goldens were captured with it                        | the one stub (§6)                                                  |
| `scripts/format-golden.sh`                                             |    29 | `humanify format-check` vs `test/parity/format-goldens.json` + a planted perturbation that must be caught                                                                 | check `rust:format-golden`             | `test/parity/format-goldens.json`                               | `format-golden`                                                    |
| `scripts/rust-env-check.sh`                                            |    45 | Toolchain pin, clippy/fmt present, MSRV = pin                                                                                                                             | by hand (RUNBOOK)                      | none                                                            | `env-check`                                                        |
| `scripts/rustfmt-staged.sh`                                            |     6 | rustfmt on staged `.rs`                                                                                                                                                   | `.husky/pre-commit` -> lint-staged     | none                                                            | `precommit`                                                        |
| `scripts/webcrack-shim.ts`                                             |   159 | Runs webcrack for the binary's webpack unpack                                                                                                                             | the pipeline binary                    | none                                                            | SHIM (stays)                                                       |
| `package.json` scripts                                                 |     — | `typecheck`, `lint*`, `knip*`, `test:unit`, `test:e2e`, `eval`, `census:*`, `ask-trace`, `diff-asks`, `match-truth`                                                       | people, agents, check.ts               | none                                                            | all become `cargo dev` verbs; `package.json` keeps `webcrack` only |
| `knip.json`, `biome.json`, prettier config, `tsconfig.json`, `.husky/` |     — | TS tooling config                                                                                                                                                         | the TS stages                          | none                                                            | RETIRE (shims get a small lint, §2.4)                              |

### 1.2 The eval dispatcher and the 034 KPI scorer

| Piece                                                                                                                                                         |     Lines | What it does                                                                                                                                                                                                         | Called by                                                             | Committed output a reference depends on                                                                                                                                         | Fate                      |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------: | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- |
| `scripts/eval.ts`                                                                                                                                             |     1,009 | THE dispatcher: 9 verbs (score, neutrality, diff, vendor-precision, summarize, bands, library, library-leaderboard, leaderboard), each with proves / cannotProve; the one flag parser; writes ONE run config per run | `npm run eval`                                                        | `<workdir>/<label>.run-config.json` (sha in `pipeline.json`); `noise-bands.json` (via `bands`)                                                                                  | `eval *`, `library *`     |
| `experiments/034-eval-harness/run.sh`                                                                                                                         |       594 | One scored label: build/record binary, rebase each v-1 (scratch / seeded / archive), scored leg, invariants, analyze, report page, boot gate, cold + warm self-hop, summarize                                        | `eval score`                                                          | `results/<label>/{commit.txt, pipeline.json, <v>.json, <v>-run.json, <v>-run-status.json, <v>-boot.json, <v>-report.html, <v>.stats.json, 2.1.216-self-hop.json, summary.json}` | `eval score`              |
| `.../analyze.ts`                                                                                                                                              |       437 | ONE scorecard per pair (KPIs: novel, realLn, noise, reloc, mints, vendor, surfaces, layout)                                                                                                                          | run.sh, rescore.ts                                                    | `<v>.json` cards                                                                                                                                                                | scorer (L2a)              |
| `.../kpis.ts`                                                                                                                                                 |       530 | The card schema and KPI definitions                                                                                                                                                                                  | analyze, summarize, leaderboard, noise-bands                          | cards, `summary.json`                                                                                                                                                           | scorer                    |
| `.../surfaces.ts`                                                                                                                                             |       693 | exp170 app / vendor / cross-boundary split                                                                                                                                                                           | analyze, kpis                                                         | the `churn.surfaces` block of every card since 2026-10-31                                                                                                                       | scorer                    |
| `.../statements.ts`                                                                                                                                           |        41 | The statement extractor (throwing owner) over `trees.ts`                                                                                                                                                             | analyze + every one-off                                               | every statement-keyed KPI                                                                                                                                                       | scorer                    |
| `.../summarize.ts`                                                                                                                                            |       735 | Cards + run status + boot + self-hop -> `summary.json` + table + banner                                                                                                                                              | run.sh, `eval summarize`                                              | `summary.json` (read by leaderboard and `bands`)                                                                                                                                | `eval summarize`          |
| `.../leaderboard.ts`                                                                                                                                          |       411 | Labels side by side, bands applied, refuses mixed base / naming mode                                                                                                                                                 | `eval leaderboard`, CLAUDE.md                                         | none (text)                                                                                                                                                                     | `eval leaderboard`        |
| `.../noise-bands.ts` + `noise-bands.json`                                                                                                                     |       133 | Bands from same-commit repeats, per naming mode                                                                                                                                                                      | `eval bands`, leaderboard                                             | `noise-bands.json`                                                                                                                                                              | `eval bands`; DATA        |
| `.../trail-report.ts`, `diff-ledger.ts`                                                                                                                       |       864 | The per-pair HTML evidence page from `--diagnostics`                                                                                                                                                                 | run.sh                                                                | `<v>-report.html` (committed beside cards)                                                                                                                                      | scorer                    |
| `.../build-constant-churn.ts`, `name-only-churn.ts`                                                                                                           |       305 | Advisory card blocks                                                                                                                                                                                                 | analyze                                                               | card fields                                                                                                                                                                     | scorer                    |
| `.../run-launch.test.ts` + 2 goldens                                                                                                                          | 645 + 490 | Runs run.sh for real with a recorder in place of npx and the binary; launches must equal the goldens                                                                                                                 | unit                                                                  | `run-launch.golden.txt`, `run-launch.seeded-base.golden.txt`                                                                                                                    | proof (c); goldens = DATA |
| `pairs.json`, `self-hop-reference.json`                                                                                                                       |         — | The 4 pairs, path defaults, the names-mode index, the LLM settings; the cold self-hop reference range                                                                                                                | eval.ts (twice), run.sh, launch_parity_test.rs                        | themselves                                                                                                                                                                      | DATA, parsed once (§2.3)  |
| `.../analyze.test.ts`, `clean-diff.test.ts`, `leaderboard.test.ts`, `noise-bands.test.ts`, `surfaces.test.ts`                                                 |     1,048 | Scorer tests                                                                                                                                                                                                         | unit                                                                  | —                                                                                                                                                                               | ported as Rust tests      |
| One-offs: `attribute-roots`, `ceiling-*` (x4), `diagnose-residual-shape`, `residual-classify`, `residual-lever1`, `simulate-root-inherit`, `name-flow-sankey` |     1,730 | Closed experiments' ceilings and diagnostics                                                                                                                                                                         | nobody living (only RESULTS.md cites them; `clone-census` scans them) | none                                                                                                                                                                            | RETIRE                    |

### 1.3 `experiments/lib/` (the shared harness library)

| Piece                                                          | Lines | What it does                                                                                                         | Called by                                      | Output a reference depends on                        | Fate                                       |
| -------------------------------------------------------------- | ----: | -------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- | ---------------------------------------------------- | ------------------------------------------ | -------------------- |
| `neutrality.sh`                                                |   358 | Byte-identity A/B: candidate at HEAD + baseline at a ref in a detached worktree, warm shared cache                   | `eval neutrality`                              | none committed (verdicts live in experiment READMEs) | `eval neutrality`                          |
| `build-bin.sh`                                                 |    27 | `cargo build --release --locked -p humanify-cli` in a root, prints the binary path                                   | neutrality.sh                                  | —                                                    | `build-bin`                                |
| `pipeline-bin.ts`                                              |   187 | Builds + records a binary: sha256, build commit, dirty tree; refuses a mismatch                                      | run.sh, walk.sh                                | `pipeline.json` `pipeline` block, run manifests      | `build-bin` + run identity                 |
| `pipeline-config.ts`                                           |   133 | THE writer of a launch's `--config` TOML; checks `--pipeline-set` FORM only                                          | run.sh, neutrality.sh, walk.sh, e2e, ask-trace | the `humanify-config                                 | ` lines in the goldens                     | settings writer (L1) |
| `run-pipeline.ts`                                              |   339 | Runs the scored leg, measures wall + peak RSS + cache writes, writes the run manifest                                | run.sh                                         | `<v>-run.json`                                       | runner (L3)                                |
| `run-manifest.ts`                                              |   618 | The run manifest schema; label base / naming mode readers                                                            | run-pipeline, eval.ts, leaderboard, invariants | `<v>-run.json`                                       | run identity (L1)                          |
| `invariants.ts`                                                |   762 | Post-run invariants -> `<v>-run-status.json`; self-hop cold range verdict                                            | run.sh, summarize                              | `<v>-run-status.json`, verdicts in `summary.json`    | runner                                     |
| `read-tracking.ts`                                             |   139 | Which recorded files the leaderboard actually read                                                                   | leaderboard                                    | —                                                    | leaderboard                                |
| `boot-gate.sh`                                                 |   104 | Boots a humanified Claude Code tree (`--version` + live `-p`), pinned `BOOT_GATE_MODEL`, env hygiene                 | run.sh, walk.sh                                | `<v>-boot.json`                                      | `boot-gate`                                |
| `llm-endpoint.sh`                                              |    51 | Reads the endpoint from the git-ignored `.humanify.local.json`                                                       | run.sh, neutrality.sh, walk.sh                 | —                                                    | registry default (L1)                      |
| `trees.ts`, `diff.ts`                                          |   274 | Tree walking, bundle statements, changed-line counting (the declared owner)                                          | everything that counts                         | every line KPI                                       | scorer                                     |
| `js/statement-hash.ts`                                         |   128 | THE KPI hash (Babel node types + `VISITOR_KEYS` order, sha256, 16 hex). Frozen: deliberately NOT the pipeline's hash | statements.ts, 037                             | every `novel` / `realLn` / `noise` on record         | scorer, byte-for-byte (proof a)            |
| `js/babel.ts`                                                  |    57 | `parseSync(sourceType: unambiguous)` + traverse shim                                                                 | all Babel users                                | (parse acceptance decides every card)                | Babel-shape adapter (§8 risk 1)            |
| `js/structural-tokens.ts`                                      |   481 | Rename-invariant token stream using Babel SCOPE resolution                                                           | 046 vendor-churn, clone census                 | vendor KPI columns                                   | scorer (§8 risk 2)                         |
| `js/line-diff.ts`                                              |   308 | Line diff via the SYSTEM `diff` binary + identifier tokenizer                                                        | diff.ts, 055 real-ledger                       | every changed-line count                             | scorer; keep shelling out to `diff`        |
| `js/tree-layout.ts`, `js/wrapper.ts`, `js/wrapper-spelling.ts` |   449 | Tree layout, module-wrapper recognition, the arrow<->function spelling rule (advisory column)                        | trees, 037, match-truth                        | `spellingIdenticalLines` (advisory)                  | scorer                                     |
| `match-truth/*`                                                | 1,419 | exp092 matcher ground truth: downloads a pinned corpus, runs `humanify match`, scores dumps                          | `npm run match-truth`                          | none committed on main (scorecards are per run)      | `match-truth`                              |
| `detect-census.ts`                                             |   134 | `humanify detect` confusion table                                                                                    | nobody (verified: no caller)                   | none                                                 | RETIRE (or port as a small verb if wanted) |
| `*.test.ts` (10 files, + 2 under `js/`, `match-truth/`)        | 2,431 | Lib tests, including `recorded-facts.test.ts` (summaries reproduce from recorded cards)                              | unit                                           | —                                                    | ported as Rust tests                       |

### 1.4 `experiments/096-library-matching-score/` (the library scorer, scorer 14)

| Piece                                                                                                                       | Lines | What it does                                                                                                                                                       | Called by                        | Output a reference depends on                                                      | Fate                                         |
| --------------------------------------------------------------------------------------------------------------------------- | ----: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------- | ---------------------------------------------------------------------------------- | -------------------------------------------- |
| `run.sh` -> `run.py`                                                                                                        |   223 | Builds the binary at a commit (detached worktree under `/work/exp096-data/builds/`), runs `humanify discover --unit-prints` per entry under `taskset`, then scores | `eval library`                   | `<DATA>/results/<label>/{pipeline.json, discover.toml, reports/}` (not committed)  | `library score`                              |
| `score.py`                                                                                                                  |   753 | One card per entry + one summary; `SCORER_VERSION = "exp096-score/14"`; truths, scopes, misses                                                                     | run.py                           | `results/<label>/{cards/*.json, summary.json}` (516 committed scorer-14 summaries) | library scorer (L2b)                         |
| `lib096.py`                                                                                                                 |   202 | 16 corpora (manifest + layer files + data dir), the one JSON spelling (`sort_keys, indent=1, ensure_ascii`)                                                        | all                              | every card's bytes                                                                 | library scorer                               |
| `truth_audit.py`                                                                                                            |   543 | Truth corrections, scorers 4-14 (copies, carried, mapdefect, misattr, ...)                                                                                         | score.py                         | card labels                                                                        | library scorer                               |
| `decoder.py`, `origins.py`, `families_lib.py`                                                                               |   433 | Source-map decoder (UTF-16 columns), origin layer, package families (imports exp095 `libix`)                                                                       | score.py, tests                  | card labels                                                                        | library scorer                               |
| `leaderboard.py`, `determinism.py`                                                                                          |   187 | Compare labels (refuses mixed corpus manifest / scorer version); two-label byte check                                                                              | `eval library-leaderboard`       | `bands.json` (band 0)                                                              | `library leaderboard`, `library determinism` |
| `corpus.py`                                                                                                                 |   305 | Freeze / verify a corpus and its origin layer                                                                                                                      | by hand, exp181 `kind_corpus.py` | `corpus-*.json`, `origins-*.json`                                                  | `library corpus`                             |
| `handcheck_draw.py`, `lazy_modules.py`, `merge_index.py`                                                                    |   241 | Hand-check draw, lazy-module census, index merge helper                                                                                                            | by hand (closed experiments)     | `handcheck.json`                                                                   | RETIRE                                       |
| `*.json` (corpus / origins / copies / chained / misattr13 / mapdefect14 / ... x 16 corpora), `bands.json`, `handcheck.json` |     — | Frozen corpus manifests and truth-layer manifests (each layer's sha)                                                                                               | every score                      | themselves                                                                         | DATA                                         |

### 1.5 Code in other experiment folders that a living piece imports or runs

This is the list the brief asked to find. Each row was found by import
graph or by a living launcher's command line, not by grepping prose.

| Piece                                                                                                     | Lines | Why it is living                                                                                                           | Fate                                                     |
| --------------------------------------------------------------------------------------------------------- | ----: | -------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `037-noise-source-decomposition/diff-composition.ts`                                                      |   887 | imported by `analyze.ts`, `surfaces.ts`, `055 real-ledger.ts`; listed in `docs/responsibility.md`                          | scorer                                                   |
| `046-vendor-noise/vendor-churn.ts` + `relocation-pairing.ts`                                              |   809 | imported by `analyze.ts`, `surfaces.ts` (the vendor columns)                                                               | scorer                                                   |
| `037/*.test.ts`, `046/vendor-churn.test.ts`                                                               |     ~ | in the `unit` stage (it finds every `*.test.ts` under `experiments/`)                                                      | ported tests                                             |
| `029-graph-clustering-split/lib/metrics.test.ts`                                                          |     ~ | ALSO in the `unit` stage, for no living reason                                                                             | RETIRE                                                   |
| `055-residual-recount/real-ledger.ts`                                                                     |   220 | `eval diff` launches it; in knip's entries                                                                                 | `eval diff`                                              |
| `166-restore-or-ask/vendor_precision.py`, `bundev-precision.sh`, `stub-run.mts`, `aggregate_precision.py` |   398 | `eval vendor-precision` launches them; exp176 and exp183 used `stub-run.mts`; `vendor_precision.py` imports the 096 scorer | `eval vendor-precision`; `stub-run` becomes a `run` verb |
| `165-claim-boundary/bun-dev-list.tsv`                                                                     |  DATA | read by `bundev-precision.sh` and `172/bundev-runs.sh`                                                                     | DATA                                                     |
| `170-app-vendor-split/rescore.ts`                                                                         |   246 | CLAUDE.md tells people to run it (re-score old labels); imports `kpis.ts`                                                  | `eval rescore` (and proof a)                             |
| `172-cjs-app-placement/score.py` + `bundev-runs.sh` (+ `trees.py`)                                        |   175 | exp183's gate scored both arms with it against `/work/exp172-data/bundev-census.jsonl`                                     | `eval cjs-placement`                                     |
| `095-library-fingerprints/misses/scripts/mmlib.py`, `decompose.py`, `truth.py`                            |   613 | imported by `score.py` (`lib096` puts the folder on `sys.path`)                                                            | library scorer                                           |
| `095-library-fingerprints/selection/scripts/libix.py`, `closure.py`                                       |   447 | `libix` imported by `families_lib.py` (needs numpy / scipy: the exp095 venv); `closure` by `kind_corpus.py`                | library scorer / corpus                                  |
| `137-scorer-mapdefect/scorer14/mapdefect14.py`                                                            |   424 | run by `test/library-truth-mapdefect14.test.ts` (unit stage); freezes scorer 14's layer                                    | `library layers`                                         |

The Python virtualenv itself, `/work/exp095-library-fingerprints/scale/.venv`,
is named in `eval.ts`, `run.sh` and `lib096.py`: an old experiment folder
that the living scorer cannot run without.

### 1.6 Library truth-layer builders and corpus freezers (living, by hand)

Not imported by the scorer at score time: they FREEZE the layers the scorer
reads, and each layer's sha is committed (`mapdefect14-*.json` etc.). They ran
on 2026-11-02 to add exp181's `kind-*` corpora, so they are living for the
next corpus.

| Chain                                                                                                                                                                                                                                                                                                                                                                  |  Lines | Fate                                 |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -----: | ------------------------------------ |
| `150-bun-testbed/scripts/layers.sh` (+ `layers2.sh`) -> `106/scorer5/chained.py`, `112/scorer7/{origins7,bundled}.py`, `104/fetch_metadata.py`, `120/scorer9/chained9.py`, `125/scorer10/origins10.py`, `128/scorer11/chained11.py`, `129/scorer12/origins12.py`, `129/copies2.py`, `132/scorer13/misattr13.py`, `137/scorer14/{mapdefect14.py, tokens14.mjs (acorn)}` | ~2,100 | `library layers <corpus>`            |
| `181-index-all-bundlers/scripts/kind_corpus.py` (+ `prep.sh`), `100-test-split/scripts/build.py`, `096/corpus.py`                                                                                                                                                                                                                                                      |   ~660 | `library corpus build/freeze/verify` |

### 1.7 Index-build orchestration

| Piece                                                                                                    | Lines | What                                                                                                                     | Fate                       |
| -------------------------------------------------------------------------------------------------------- | ----: | ------------------------------------------------------------------------------------------------------------------------ | -------------------------- |
| `scripts/build-index.sh` (on branch `exp184-index-spec`, NOT on main)                                    |    81 | Spec-driven chunked build: N chunks in parallel, one `--merge-only`, holds `/work/heavy.lock` + `/work/index-build.lock` | `index build`              |
| `152-full-bun-index/scripts/{build-full.sh, make_shims.sh, timed.sh}` + `step_times.py`, `root_roles.py` |   277 | How the standing index `cb64c1f5…` was built (chunks, npm / Bun timing shims, `build.json` sidecar)                      | `index build` (shims stay) |
| `181-index-all-bundlers/scripts/ix.sh`                                                                   |    32 | One build of every recipe, then a merge-only index per kind                                                              | `index build --per-kind`   |
| `097/101/095/105 build-index.sh`, `build-slices.sh`                                                      |     — | Superseded copies                                                                                                        | RETIRE                     |

### 1.8 The version walk and the tests in `test/`

| Piece                                                                                                                                           | Lines | What                                                                                                                                                                                           | Fate                                                                |
| ----------------------------------------------------------------------------------------------------------------------------------------------- | ----: | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| `experiments/walk/{walk.sh, history.sh, report.ts, hunks.ts}` (+ test)                                                                          | 1,315 | Real version walk, hop N on hop N-1's own output; still carries a dead `--pipeline ts` mode. It is in knip's entries but NOT a verb of `eval.ts`, so by CLAUDE.md's rule it is "not supported" | DECISION for Andrew: port as `eval walk` (rust mode only) or retire |
| `test/measurement-owners.test.ts`                                                                                                               |   372 | 15 guard rules for the measurement stack (verdict files consumed, no ambient env, one parser, ...)                                                                                             | each rule ported as a Rust test of the crate, or moot (table in §6) |
| `test/no-private-addresses.test.ts`                                                                                                             |    97 | No RFC 1918 address in a tracked file                                                                                                                                                          | `check` stage `public-repo`                                         |
| `test/eval-score-args`, `eval-library-args`, `boot-gate-env`, `e2e-known-gaps`, `ask-trace-harness`, `clone-census`, `concurrency-census` tests | 1,557 | Tests of the instruments                                                                                                                                                                       | ported with their instrument                                        |
| `test/library-truth-{decoder,origins7,mapdefect14}.test.ts`                                                                                     |   534 | Spawn `python3` on the 096 decoder, origins and the 137 layer                                                                                                                                  | ported with the library scorer                                      |
| `scripts/ask-trace.ts`, `scripts/diff-asks.ts`                                                                                                  |   455 | Run the binary with `--dump-asks` against the stub; diff two ask dumps                                                                                                                         | `ask-trace`, `diff-asks`                                            |

## 2. Target: the `humanify-ag-dev` crate

### 2.1 Shape

```
crates/humanify-ag-dev/          publish = false; [[bin]] humanify-ag-dev
  src/main.rs                    the verb table (one entry per verb: usage, proves, cannotProve)
  src/settings/                  the harness key registry + its one parser (2.2)
  src/committed/                 typed loaders for every committed config file (2.3)
  src/identity/                  the run identity record + the ONE comparison guard (3)
  src/env.rs                     the ONE environment read (HOME, PATH, the local endpoint file)
  src/bin_build.rs               build-bin for any commit: humanify / humanify-ag (5)
  src/launch/                    every launch line the harness makes, built in one place
  src/eval/  src/library/  src/gate/  src/census/  src/e2e/ ...
crates/humanify-ag-testkit/      publish = false: the ONE stub LLM server + the shuffled fake provider (6)
dev/                             committed harness DATA (moved, never edited by the move):
  eval/pairs.json  eval/noise-bands.json  eval/self-hop-reference.json  eval/goldens/*.txt
  eval/results/<label>/          (today experiments/034-eval-harness/results/)
  library/corpora/*.json  library/bands.json  library/results/<label>/
  census/concurrency-allowlist.json  census/clone-allowlist.json
tools/                           the SHIMS only: webcrack-shim.ts, the e2e boot probe, npm/Bun timing shims
.cargo/config.toml               [alias] dev = "run --profile dev-harness --locked -p humanify-ag-dev --"
```

- Built optimised: a `[profile.dev-harness]` that inherits `release` but sets
  `lto = false` and `codegen-units = 16`. The release profile's thin LTO and
  `codegen-units = 1` would make every harness edit a long rebuild, and the
  scorer does not need them. The release artifact stays
  `cargo build --release --locked -p humanify-ag-cli`, which this crate is not part of.
- The crate depends on `humanify-ag-cli` (it is a lib + bin today), so the
  harness can use the CLI's own key registry (`config/keys.rs`) to validate
  `--pipeline-set` keys and read defaults, instead of hand copies (§6).
- It sits in `crates/`, so clippy, `cargo fmt`, `cargo test --workspace` and
  the concurrency census all cover it. Its subprocesses, its parallel
  scoring and its deadlines go on the concurrency allowlist like any other
  site. Parallel CPU work goes through `humanify-ag-core`'s `par`, not a new
  pool. The one `std::env` read carries the same `#[allow]` the CLI's env
  module does.

### 2.2 One settings registry for the harness

The CLI's model, applied to the harness. One table, `HARNESS_KEYS`, declares
every setting once: section, name, type and range, default, meaning, and the
flag it generates. Every key is a flag, every flag is a key, and there is ONE
parse point. An unknown key or flag fails before anything runs, with a "did
you mean". Retired spellings (`--no-vendor`, `--pipeline-arg`, `--heap-mb`)
are refused with what replaced them, as today. A run's settings are written
once, as one run config file, and its sha goes into the run identity.

The generic machinery (key struct, typed values, edit-distance suggestion,
building clap from the table) already exists in `humanify-ag-cli`
(`config/keys.rs`, `config/file.rs` with its "did you mean", `surface.rs`).
L1 extracts it into a module both binaries use rather than copying it. That
refactor touches the product CLI, so it needs a warm neutrality run (it must
change no output byte).

The keys, from today's four flag tables (`SCORE_FLAGS`, `NEUTRALITY_FLAGS`,
`LIBRARY_FLAGS`, `walk.sh`'s header) plus the env folklore they replaced:

| Section      | Keys                                                                                                                                                                                                 |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `run`        | `workdir`, `force_mixed`, `bin`, `pipeline_set` (repeatable, ordered)                                                                                                                                |
| `eval`       | `base_mode` (scratch / seeded / archive: one key, not two exclusive flags), `pairs`, `llm_cache`, `layout`, `vendor_score`, `boot_prompt`, `self_hop`, `inputs_base`, `priors_base`, `library_index` |
| `neutrality` | `baseline`, `pair`, `cache`, `priors`                                                                                                                                                                |
| `libscore`   | `index`, `rule`, `commit`, `jobs`, `cpus`, `corpus`, `sets`                                                                                                                                          |
| `llm`        | `endpoint` (default: the git-ignored `.humanify.local.json`), `model`, `reasoning_effort`, `concurrency` (defaults: `pairs.json` `llm`)                                                              |
| `boot`       | `model` (today `BOOT_GATE_MODEL` inside `boot-gate.sh`)                                                                                                                                              |
| `index`      | `spec`, `registry`, `store`, `work`, `out`, `chunks`, `jobs`, `merge_jobs` (today `JOBS` / `MERGE_JOBS` env reads in `build-index.sh`)                                                               |

`run.pipeline_set` entries are PIPELINE keys. They are validated against the
registry of the binary that will run them, not against this crate's
compiled-in copy: a neutrality baseline from an old commit has an older
registry. For HEAD's binary that is the linked `humanify-ag-cli` registry. For
any other commit, a small `humanify-ag config check <file>` verb in the
product binary would answer it. That verb does not exist today; until it
does, an old binary's keys are checked at its first launch, as they are now.

### 2.3 Committed config files, parsed and validated once

One typed loader per file (`serde`, `deny_unknown_fields`), called once per
run. Every reader goes through it. The loaded file's sha goes into the run
identity.

| File (new home)                                                                               | Read today by                                                                                       |
| --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `dev/eval/pairs.json`                                                                         | `eval.ts:260` and `:344`, `run.sh:49,184-213`, `launch_parity_test.rs:454` (four readers, verified) |
| `dev/eval/noise-bands.json`                                                                   | leaderboard, `eval bands` (writer)                                                                  |
| `dev/eval/self-hop-reference.json`                                                            | run.sh (`jq`), invariants.ts                                                                        |
| `dev/eval/goldens/*.golden.txt`                                                               | run-launch.test.ts, launch_parity_test.rs                                                           |
| `dev/library/corpora/{corpus,origins,copies,chained,misattr13,mapdefect14,...}-<corpus>.json` | lib096 `CORPORA` (16 corpora, data dirs hard-coded)                                                 |
| `dev/library/bands.json`                                                                      | leaderboard.py                                                                                      |
| `dev/census/concurrency-allowlist.json`                                                       | concurrency census                                                                                  |
| `dev/census/clone-allowlist.json`                                                             | today an array inside `clone-census.ts`                                                             |
| `test/e2e/fixtures/*/fixture.config.json`                                                     | e2e.ts                                                                                              |
| `.humanify.local.json` (git-ignored, per host)                                                | `llm-endpoint.sh`                                                                                   |

The library corpora's DATA directories (`/work/exp150-data/...`,
`/work/cc-library-truth/frozen`, ...) are per-host paths. They move out of
code and into the corpus manifest, overridable by one `libscore.data_root`
key. That is the same treatment `pairs.json` gives `inputsBase`.

### 2.4 The subcommand map

| `cargo dev ...`                                        | Replaces                                                                                                                                                                                          |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `check [--only a,b]` (PARTIAL when `--only`)           | `scripts/check.ts`, the `package.json` scripts                                                                                                                                                    |
| `eval score <label>`                                   | `eval.ts score`, `034/run.sh`, `pipeline-bin.ts`, `pipeline-config.ts`, `run-pipeline.ts`, `run-manifest.ts`, `invariants.ts`, `llm-endpoint.sh`, `analyze.ts`, `trail-report.ts`, `summarize.ts` |
| `eval rescore <label> --work <dir>`                    | `170-app-vendor-split/rescore.ts` (also the tool behind proof a)                                                                                                                                  |
| `eval summarize <label>`                               | `summarize.ts`                                                                                                                                                                                    |
| `eval leaderboard <label>...`                          | `leaderboard.ts`, `read-tracking.ts`                                                                                                                                                              |
| `eval bands <label> <label>...`                        | `eval.ts bands`, `noise-bands.ts`                                                                                                                                                                 |
| `eval diff <prior> <fresh>`                            | `055/real-ledger.ts`, `037/diff-composition.ts`                                                                                                                                                   |
| `eval neutrality <ref> [from:to]`                      | `neutrality.sh`, `build-bin.sh`                                                                                                                                                                   |
| `eval vendor-precision check\|bun-dev`                 | `166/vendor_precision.py`, `bundev-precision.sh`, `aggregate_precision.py`                                                                                                                        |
| `eval cjs-placement`                                   | `172/score.py`, `bundev-runs.sh`, `trees.py`                                                                                                                                                      |
| `eval walk` (if kept)                                  | `experiments/walk/*`                                                                                                                                                                              |
| `run stub <bin> <bundle> <out> [--mode]`               | `166/stub-run.mts` (a binary run against the one stub, no LLM)                                                                                                                                    |
| `library score <label>`                                | `096/run.sh`, `run.py`, `score.py`, `lib096.py`, `truth_audit.py`, `decoder.py`, `origins.py`, `families_lib.py`, 095 `mmlib` / `decompose` / `truth` / `libix`                                   |
| `library leaderboard <label>...`                       | `096/leaderboard.py`                                                                                                                                                                              |
| `library determinism <a> <b>`                          | `096/determinism.py`                                                                                                                                                                              |
| `library corpus build\|freeze\|verify <corpus>`        | `096/corpus.py`, `181/kind_corpus.py`, `100/build.py`, `095/closure.py`                                                                                                                           |
| `library layers <corpus>`                              | `150/layers.sh` and the 11 layer builders of §1.6                                                                                                                                                 |
| `index build <spec>`                                   | `scripts/build-index.sh` (exp184 branch), `152/build-full.sh`, `181/ix.sh` (lock handling: ONE lock layer, §8)                                                                                    |
| `boot-gate <tree> <version>`                           | `boot-gate.sh` (bun and the walked CLI stay external tools)                                                                                                                                       |
| `build-bin <ref> [--root]`                             | `build-bin.sh`, `pipeline-bin.ts`, `run.py`'s build, `walk.sh`'s build: ONE owner                                                                                                                 |
| `e2e`                                                  | `scripts/e2e.ts`                                                                                                                                                                                  |
| `format-golden`                                        | `scripts/format-golden.sh`                                                                                                                                                                        |
| `census clones` / `census concurrency` / `census slop` | `clone-census.ts` (now over `crates/`), `concurrency-census.ts`, new (§7)                                                                                                                         |
| `ask-trace` / `diff-asks`                              | `scripts/ask-trace.ts`, `scripts/diff-asks.ts`                                                                                                                                                    |
| `match-truth`                                          | `experiments/lib/match-truth/*` (its corpus cache moves out of `experiments/092-*/.corpus` to `/work/match-truth-corpus`)                                                                         |
| `env-check`                                            | `scripts/rust-env-check.sh`                                                                                                                                                                       |
| `precommit`                                            | `.husky/pre-commit` + lint-staged + `rustfmt-staged.sh` + `concurrency-census --files` (hook installed via `core.hooksPath` = `.githooks/`)                                                       |
| `stub-llm serve [--port]`                              | `scripts/lib/stub-llm.ts` as a process (for by-hand runs)                                                                                                                                         |

Every verb keeps today's `proves` / `cannotProve` pair, printed by
`cargo dev` with no arguments. A test fails a verb without both, as
`measurement-owners.test.ts` does today.

## 3. Run identity and the one comparison guard

### 3.1 The record

Every card (eval pair, library entry, neutrality leg, walk hop, e2e run)
gets ONE `identity.json` written beside it, in one schema:

| Field              | Content                                                                                                            |
| ------------------ | ------------------------------------------------------------------------------------------------------------------ |
| `schema`           | identity schema version                                                                                            |
| `kind`             | eval-pair / library-entry / neutrality-leg / walk-hop / e2e                                                        |
| `harness`          | the harness commit + dirty flag (the commit `cargo dev` was built from)                                            |
| `binary`           | path, sha256, build commit, dirty flag, binary name (`humanify` or `humanify-ag`)                                  |
| `baseMode`         | scratch / seeded / archive (eval only)                                                                             |
| `namingMode`       | names / off: the EFFECTIVE mode, read from the binary's own stats record, not inferred from the harness's settings |
| `libraryIndex`     | index id (manifest sha), dir, bytes; null in off mode                                                              |
| `pipelineSettings` | the `--pipeline-set` list in order, plus the launch's config TOML sha                                              |
| `llm`              | model, reasoning effort, concurrency, `endpointId`, cache (none, or its dir + entries before / after)              |
| `corpus`           | name, manifest sha256, each truth layer's sha (library); pair list + input shas (eval)                             |
| `scorer`           | name + version (`034-kpi/<n>`, `exp096-score/14`)                                                                  |
| `bootGateModel`    | the pinned model                                                                                                   |
| `runConfigSha256`  | the one run config file                                                                                            |

`endpointId` is a sha256 of the endpoint URL, never the URL. The repo is
public, and the endpoint is a private address that
`no-private-addresses.test.ts` exists to keep out.

A LABEL's identity is derived from its cards, never written over. Today
`run.sh:161` rewrites `pipeline.json` on every run, so a forced partial
re-run silently replaces the record of the earlier cards (verified). Under
this design a label is the set of its cards' identities.

### 3.2 The one guard

```rust
fn comparable(purpose: Purpose, a: &Identity, b: &Identity) -> Result<(), Vec<Mismatch>>
```

There is ONE function, plus a table that says which fields each purpose
requires to be equal:

| Purpose                      | Must be equal                                                                                              | May differ (that is the point)    |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------- | --------------------------------- |
| add a card to a label        | everything except `corpus.pairs` / entry                                                                   | —                                 |
| leaderboard / compare (eval) | `baseMode`, `namingMode`, `libraryIndex.id`, `scorer`, `llm.model`, `llm.reasoningEffort`, pair input shas | harness, binary, pipelineSettings |
| library leaderboard          | `corpus.manifest` + layer shas, `scorer`                                                                   | binary, index, settings           |
| bands (same-commit repeats)  | binary sha, everything in the leaderboard row                                                              | nothing else                      |
| neutrality legs              | `llm.*`, `pipelineSettings`, `libraryIndex.id`, `namingMode`                                               | binary (the change under test)    |

`--force-mixed` is the only override. It prints every mismatch by field name.

Legacy labels (every label scored before this lands) have no `identity.json`.
A one-time backfill reads `commit.txt`, `pipeline.json` and the run
manifests, using the same readers as today's `labelBaseModes` and
`labelNamingModes`. Any field it cannot recover is `unknown`, and `unknown`
never equals anything unless forced. That is stricter than today on purpose:
a missing record is UNKNOWN, not passing.

This replaces `guardLabel`, `guardPipeline`, `guardBaseMode` (eval.ts
:87/:656/:617), `baseModeComparison` + `namingModeComparison`
(leaderboard.ts:293-316), leaderboard.py's corpus / scorer refusal, and
pipeline-bin's commit refusal. Today none of these checks the library index,
and "add to a label" checks neither the naming mode nor the index (verified).

## 4. Byte-identity proofs for the port

The rule for all four: the TS / Python instrument, run TODAY on the same
inputs, is the oracle. Recorded cards are a second check, because some
recorded copies predate later scorer changes (proof a shows one case). Each
proof runs under `flock /work/heavy.lock` and is a committed `cargo dev`
self-test. It is not a one-off script.

| #   | Proof                                | Oracle                                                          | Data on this box                                                                                                                                                                                                                                                                                                                                                                                                                                                          | Pass condition                                                                                                              |
| --- | ------------------------------------ | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| a   | statementHash + the 034 KPI scorer   | `rescore.ts` + `summarize.ts` at the frozen TS commit           | trees: `/tmp/eval-work/ref-names-0357696e-r{1,2,3}` (1.7 GB each) and `/tmp/eval-work/ref-scratch-0f338ffa-r{1,2,3}` (1.4 GB each); cards: `/work/exp171-names-default/experiments/034-eval-harness/results/<same labels>/`                                                                                                                                                                                                                                               | `cmp -r` of every `<v>.json`, `summary.json` and `<v>-report.html`; leaderboard text equal                                  |
| b   | the exp096 library scorer, scorer 14 | `score.py` run today (exp095 venv)                              | cards + summaries: main checkout `experiments/096-library-matching-score/results/exp184-{cb64,iv,spec}-{bun-dev,bun-held,cc}/` (untracked); reports: `/work/exp150-data/{bun-dev,bun-held}/frozen/results/<label>/reports`, `/work/cc-library-truth/frozen/results/<label>/reports`; indexes: `/work/library-index/cb64c1f5…`, `/work/exp177-data/ix/full-bun-names-iv-asof20261025`, `/work/exp184-data/ix-default` (all present); index cache `/work/exp096-data/cache` | `cmp -r` of `cards/` + `summary.json` on all 9 labels; then every committed scorer-14 label whose reports are still on disk |
| b'  | truth-layer builders                 | the committed layer manifests (each layer's sha)                | the corpus data dirs above                                                                                                                                                                                                                                                                                                                                                                                                                                                | re-freezing a corpus's layers into a scratch dir reproduces every committed sha                                             |
| c   | every launch line                    | goldens RECORDED from today's launchers (L0), never hand-copied | the repo                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | each Rust runner's recorder log is byte-identical to its golden                                                             |
| d   | the gate's stage coverage            | `check.ts` `STAGES`                                             | the repo                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | a committed mapping test (below); every stage's red seen once                                                               |

**(a) How it runs.** L0 copies the six trees from `/tmp/eval-work` to
`/work/dev-harness-proof/trees/`, and the six card directories from the
exp171 worktree to `/work/dev-harness-proof/cards/`. Then it runs
`rescore.ts --cards ... --work ...` at the frozen TS commit and checks it
prints `OLD-FIELDS IDENTICAL` and equal cards. That run is the oracle. Next,
`cargo dev eval rescore <label> --work <tree> --out <dir>` for each label,
then `cmp -r <dir> <oracle>`. Each pair needs both sides' `humanified.js`,
`split-ledger.json`, `src/` and `vendor/`, plus the run's `stats.json` (in
the cards dir). On scratch bases the prior is
`<work>/<label>/<from>-rebased/`. Beneath it sits a hash-level check: every
top-level statement of all 12 trees hashed by both implementations (millions
of statements, on box), plus a committed sample file of (source, hash)
vectors for `cargo test`.

Do not use the main checkout's `results/ref-scratch-0f338ffa-*` as the
expected bytes. Those are the 2026-10-06 cards, before exp170's surfaces
block: `cmp` differs at byte 2425 of `2.1.86.json` (verified).

**(b) How it runs.** For each label: `score.py <label> --corpus <c>` today,
and confirm it reproduces the on-disk bytes. That is the oracle, and it is
also `determinism.py`'s check. Then `cargo dev library rescore <label>`,
then `cmp -r`. The `cc` corpus's truth stays in `/work/cc-library-truth`,
which is never committed. The proof reads it in place and commits nothing
derived from it.

**(c) How it runs.** Today only run.sh has launch goldens. L0 RECORDS the
missing ones with the same method `run-launch.test.ts` uses: run the real
launcher with a recorder in place of the binary (and of `npx`), normalise
paths, commit the log. The missing goldens are:

- neutrality: both legs, plus the legacy no-`--config` leg shape;
- library: `discover --unit-prints ... --config discover.toml` under `taskset`;
- e2e: all legs;
- ask-trace;
- match-truth;
- the boot gate's CLI argv and its environment scrub;
- walk, if kept.

The Rust runners then produce recorder logs that must equal those goldens
byte for byte. `launch_parity_test.rs` changes from a transcribed list,
whose neutrality entry is already stale (§6), to reading every golden.

**(d) How it runs.** A committed test maps each `check.ts` stage to its successor:

| check.ts stage                   | Successor                                                                               |
| -------------------------------- | --------------------------------------------------------------------------------------- |
| typecheck, lint, knip, knip:prod | `shims` (syntax check + prettier on `tools/` only; the harness TS they covered is gone) |
| rust:fmt, rust:clippy            | unchanged (now also cover the dev crate)                                                |
| census:clones (advisory)         | `census:clones` over `crates/` (advisory)                                               |
| census:concurrency               | `census:concurrency` (pass / fail)                                                      |
| unit                             | MOOT as TS; its tests are ported into `rust:unit`                                       |
| rust:unit                        | unchanged (includes the dev crate and the testkit)                                      |
| rust:build                       | unchanged: `cargo build --release --locked -p humanify-ag-cli`                          |
| rust:format-golden               | `format-golden`                                                                         |
| e2e                              | `e2e`                                                                                   |
| (new) public-repo                | `no-private-addresses` as a stage                                                       |
| (new, advisory) census:slop      | §7                                                                                      |

Each pass / fail stage must be shown red once, with a planted failure, the
way `format-golden` already plants one.

## 5. Phases and lanes

```
L0 record + freeze  ->  L1 skeleton  ->  { L2a eval scorer | L2b library scorer | L3 runners | L4 gate+censuses+e2e }  ->  L5 deletion + docs
```

| Lane                                 | Scope                                                                                                                                                                                                                                                                                                                                                                              | Files                                                                                         | Proof required                                                                                                                                              | Risk                                                |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| **L0** record + freeze (TS, on main) | Record the missing launch goldens (§4 c). Copy the proof data to `/work/dev-harness-proof/`. Run each oracle once and keep its output. Tag `harness-ts-final` at the commit the oracles ran on                                                                                                                                                                                     | new goldens under `experiments/034-eval-harness/` (moved in L5); no code changes              | each new golden recorded from a live launcher, not typed                                                                                                    | low                                                 |
| **L1** skeleton                      | The crate + `.cargo` alias + profile; the settings registry (extracted from the CLI); committed-file loaders; run identity + the one guard + the legacy backfill; `env.rs`; `build-bin`; the two JSON spellings (JS `JSON.stringify(x,null,2)`, Python `dumps(sort_keys,indent=1,ensure_ascii)`); subprocess helper with deadlines; the `humanify-ag-testkit` stub + shuffled fake | `crates/humanify-ag-dev/`, `crates/humanify-ag-testkit/`, a shared registry module in the CLI | unit tests; JSON spellings round-trip every committed card and summary byte for byte; warm neutrality NEUTRAL for the CLI refactor                          | medium: the registry extraction touches the product |
| **L2a** eval scorer                  | Babel-shape AST adapter over oxc; statementHash; trees / diff (shell out to the system `diff`, as today); statements, kpis, surfaces, analyze, diff-composition, vendor-churn + relocation pairing, structural tokens (scope), build-constant / name-only churn, trail report, summarize, leaderboard, bands, real-ledger, rescore                                                 | `src/eval/**`                                                                                 | proof (a)                                                                                                                                                   | HIGH (§8 risks 1-3)                                 |
| **L2b** library scorer               | score, lib096, truth_audit, decoder, origins, families (libix's numpy / scipy part), mmlib / decompose / truth, leaderboard, determinism, vendor-precision, cjs-placement; then the corpus freezers and the layer builders (with an acorn-equivalent tokenizer for mapdefect14)                                                                                                    | `src/library/**`                                                                              | proof (b), then (b')                                                                                                                                        | HIGH (§8 risks 4-5)                                 |
| **L3** runners                       | `eval score` (run.sh), `eval neutrality`, `library score` runner, `boot-gate`, run manifest / invariants / run-status, `index build`, `run stub`, walk (if kept)                                                                                                                                                                                                                   | `src/runners/**`, `src/launch/**`                                                             | proof (c) on every runner; one real `eval score --pairs 85->86` and one warm neutrality against the TS runner's outputs                                     | medium                                              |
| **L4** gate + censuses + e2e         | `check`, `e2e` (+ the boot-probe shim), `format-golden`, `census concurrency` (port), `census clones` (Rust, over `crates/`), `census slop` (new), ask-trace, diff-asks, match-truth, env-check, precommit hook; the one stub LLM and the shuffled fake wired into every crate test; `launch_parity_test` reads goldens; the exp162 anchor fix                                     | `src/gate/**`, `src/census/**`, `src/e2e/**`, crate tests                                     | proof (d); e2e outputs byte-equal to the TS e2e's on every fixture; concurrency census verdict equal to the TS census on main and on its planted test trees | medium                                              |
| **L5** deletion + docs               | Delete the TS / Python / sh harness and the old experiments' CODE (after tag `experiments-code-final`); `git mv` data into `dev/` byte-for-byte; reduce `package.json` to `webcrack` + shims; drop knip / biome / tsconfig / husky; rewrite CLAUDE.md (proposed text in the exp185 README), `docs/responsibility.md` owners, `experiments/README.md`                               | everywhere                                                                                    | `cargo dev check` 13 -> new stage list green from a CLEAN checkout; `cargo dev eval leaderboard` reads a moved legacy label and prints the same table       | low if L2-L4 proved                                 |

L2a, L2b, L3 and L4 run in parallel after L1. They share only L1's types.
L3 can be developed against the recorder goldens before the scorers land.
Its end-to-end check calls the L2 scorers once they are proven.

**build-bin across the rename.** Neutrality builds its BASELINE from an old
commit, and the walk and the library runner build arbitrary commits too. So
`build-bin` resolves the package and binary name from the commit, not from
HEAD. It reads that commit's `crates/*/Cargo.toml` for the `[[bin]]` whose
path is the pipeline's `main.rs`: `-p humanify-cli` -> `target/release/humanify`
before the rename, `-p humanify-ag-cli` -> `target/release/humanify-ag` after.
It records the name in the identity. It must also keep today's
legacy-leg handling: a baseline binary with no `--config` flag
(`neutrality.sh` `leg_flags` probes `--help`). A ref from before the cutover
has no pipeline binary, and it is refused, as today.

## 6. Test cleanup folded in

| Finding (test review, 2026-11-05)                                                                                                                      | Verified?                                                                                                                                                                                                                                                                                               | Lane / fate                                                                                                                                                                                                                 |
| ------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `jq '... // empty'` / `run_config_flag` turn a missing field into "" / 0 (034/run.sh:60-62; neutrality.sh:81)                                          | YES. Also worse than stated: jq's `//` treats `false` as missing too                                                                                                                                                                                                                                    | MOOT with run.sh (L3): the run config is a typed struct, `deny_unknown_fields`, no defaults at the reader                                                                                                                   |
| Adding to a label checks only commit / pipeline kind / base mode (eval.ts 87/617/656/708); run.sh rewrites `pipeline.json`                             | YES (`run.sh:161` writes `> "$RESULTS/pipeline.json"`)                                                                                                                                                                                                                                                  | L1: identity + the one guard; label identity derived, never rewritten                                                                                                                                                       |
| Leaderboard guards base + naming mode only, not index (leaderboard.ts:293-316)                                                                         | YES                                                                                                                                                                                                                                                                                                     | L1 guard, used by L2a's leaderboard                                                                                                                                                                                         |
| `BINARY_DEFAULT_MATCHING` hand-copied (eval.ts:228)                                                                                                    | YES (a comment points at `keys.rs`)                                                                                                                                                                                                                                                                     | L1: HEAD's default from the linked registry; any other binary's EFFECTIVE mode from its own stats record (an old baseline's default was `off`)                                                                              |
| `pairs.json` read by two parsers (eval.ts:260, run.sh:184-213)                                                                                         | YES, and by four readers in all: `eval.ts:260`, `eval.ts:344`, `run.sh`, `launch_parity_test.rs:454`                                                                                                                                                                                                    | L1 committed loader                                                                                                                                                                                                         |
| `--pipeline-set` keys checked for form only                                                                                                            | YES (`pipeline-config.ts parseSetting`; the binary checks at the first launch)                                                                                                                                                                                                                          | L1: validated against the target binary's registry (§2.2)                                                                                                                                                                   |
| `launch_parity_test.rs` transcribed launches are hand copies (stale neutrality copy)                                                                   | YES. Its neutrality entry is `--llm-cache /work/neutrality-cache --prior-version p.js`, while today's `leg_flags` passes `--config <toml> --endpoint --model ...`                                                                                                                                       | L0 records real goldens; L4 makes the test read them                                                                                                                                                                        |
| Clone census covers TS only (`DEFAULT_ROOTS`); `matching.rs` `build_full_fingerprint` (:517) vs `build_binding_full_fingerprint` (:622) is a live twin | YES on both. The twin is a near-copy (callee shapes, caller shapes, sorted hashes, two-hop set); the binding side also hashes binding callees                                                                                                                                                           | L4: Rust census over `crates/`. The twin is unified in its own small product lane (it touches matching, so it needs warm neutrality), not inside the harness port                                                           |
| Six hand-written stub LLM servers -> one                                                                                                               | YES, with a correction: `humanify-llm stub_server.rs`, cli tests `replay_determinism.rs`, `ask_trace.rs`, `provider_errors.rs` (two listeners), core `namer_stub_test.rs`, `scripts/lib/stub-llm.ts` — plus a seventh ad hoc listener in `humanify-llm client_test.rs`                                  | L4: `humanify-ag-testkit` holds the one server; product crates dev-depend on it. The testkit depends only on `humanify-ag-model`, never on core, to avoid a dev-dependency cycle that would give two copies of core's types |
| ~20 fake name providers inherit an in-order `run_pipelined` while the real provider completes out of order -> one shared shuffled fake                 | YES: 28 test impls of `NameProvider`; 27 inherit the generational default (only `fast_test.rs` overrides). The real provider (`humanify-llm provider.rs:159`) and `llm_sim.rs` override it                                                                                                              | L4: one fake in the testkit whose `run_pipelined` completes in a seeded-shuffled order; ports the 27                                                                                                                        |
| Wrapper-spelling rule in TS and Rust with no shared vectors (moot once TS is gone — confirm)                                                           | NOT simply moot. The KPI hash deliberately does NOT apply the rule (verified in `statement-hash.ts`'s header), but the TS rule still feeds the scorer's advisory `spellingIdenticalLines` (037) and match-truth's canonical form. During the port the Rust scorer must reproduce the TS rule's verdicts | L2a: shared vectors (TS verdict vs Rust verdict on every wrapper in the reference trees) as part of proof (a). After that it is ONE rule (`hash::wrapper_spelling`), and only then moot                                     |
| `build_test.rs:1021 exp162_anchor` passes green when its env var is missing                                                                            | YES (`_ => return`). It is `#[ignore]`d, so it is green only when run with `--ignored` and no env var, which is exactly the hand-run case it exists for                                                                                                                                                 | L4: panic with the usage line when the var is missing                                                                                                                                                                       |

Also found while inventorying:

- The `unit` stage runs `experiments/029-graph-clustering-split/lib/metrics.test.ts`
  for no living reason. It is retired with the TS.
- `measurement-owners.test.ts`'s 15 rules split three ways:
  - MOOT with the TS: "the TS pipeline is gone", "TS stage-6 adapter gone",
    "no ambient eval env vars", "no `VAR:-default` in shell scripts",
    "run.sh / neutrality.sh read ONE run config";
  - ported as dev-crate tests: "verdict files consumed by the summary",
    "every verdict file run.sh writes is read", "leaderboard refuses bands
    from another commit", "every verb declares proves AND cannotProve",
    "unknown flag fails upfront", "binary cards never join a TS label" (now
    the identity guard), "statement extraction routes through the throwing
    owner";
  - made structural by the crate: "changed-line counting stays in the
    declared sites" becomes one counting module, plus the clone census.

## 7. An advisory "slop" census for `crates/` (gate lane, L4)

Definitions as given in the brief (Earendil, "Measuring the sloppiness of
code", 2026-09-10; SlopCodeBench, arXiv 2603.24755). I did not read either
source; see §8.

- Per function: `mass = CC x sqrt(SLOC)`. CC is cyclomatic complexity: 1, plus
  one for each `if` / `else if`, `while`, `for`, `loop`, `&&`, `||`, and each
  `match` arm after the first. `?` is not counted. The exact rule is written
  down and versioned with the census.
- Per crate: `erosion = sum(mass of functions with CC > 10) / sum(mass of all functions)`.
- Per crate: clone share = the share of function SLOC that sits in a clone
  group (identifier-masked token streams, a minimum size, the same detector
  as `census clones`).
- Printed per crate as a DELTA against `main`'s merge-base (computed from git
  blobs, no worktree needed): non-test code headline, test code apart,
  ADVISORY (`REVIEW`, never FAIL), like `census:clones`.
- Parsing: `syn` 2 (`full`, `visit`). It is already in `Cargo.lock`
  transitively; the census pins it in `[workspace.dependencies]`.

How it complements clippy. Clippy already denies `cognitive_complexity` above
15 per function (`clippy.toml`, `Cargo.toml` `[workspace.lints.clippy]`).
That is a per-function CEILING: binary, blind below 15, and silent about the
2 sites that carry `#[allow(clippy::cognitive_complexity)]`. The census
measures the DISTRIBUTION and its TREND instead:

- a crate drifting toward many functions at CC 11-14 passes clippy and moves
  erosion;
- cyclomatic and cognitive complexity disagree on wide `match` dispatch
  (high CC, low cognitive), which is exactly where table-driven code
  hides growth;
- clippy has no clone measure at all.

The census also lists every `#[allow(clippy::cognitive_complexity)]` site by
name. The census is advisory because a measure of style cannot be a
correctness verdict. Its job is to make a reviewer look.

## 8. Risks, and what I could not verify

**Risks, highest first.**

1. **The KPI hash is Babel's AST.** `statementHash` hashes Babel node TYPE
   NAMES and walks children in `@babel/types` `VISITOR_KEYS` order (verified).
   oxc emits ESTree, not Babel. So the Rust scorer needs a Babel-shape
   adapter that reproduces, for every node, Babel's type name and its
   visitor-key order:

   - `StringLiteral` / `NumericLiteral` vs `Literal`;
   - `ObjectProperty` / `ObjectMethod` vs `Property`;
   - `ClassMethod` vs `MethodDefinition`;
   - `OptionalMemberExpression` / `OptionalCallExpression` vs `ChainExpression`;
   - `Directive` / `DirectiveLiteral`;
   - parenthesised expressions;
   - `File` / `Program`.

   It must also reproduce each node's content string:

   - JavaScript's `String(number)` formatting;
   - the cooked string value, including lone surrogates (Node hashes them as
     U+FFFD);
   - `pattern/flags` for regexes;
   - the raw template value.

   One wrong mapping changes every hash of statements that contain it. If
   proof (a) cannot be met, the fallback is a new scorer version that
   re-scores every reference label from its trees. That is a decision for
   Andrew, not for the porting lane.

2. **Parse acceptance.** `parseSync(sourceType: "unambiguous")` decides what
   a statement IS. oxc and Babel may accept or reject different inputs (for
   example, script vs module detection), and a file Babel rejects is scored
   differently. Proof (a)'s per-statement sweep over all 12 trees is the
   check.
3. **Scope resolution.** `structural-tokens.ts` resolves bindings with
   `@babel/traverse` scope rules, including its parent-scope special case
   for function ids. The vendor columns depend on it. `oxc_semantic`'s
   scoping must be shown to give the same binding for every occurrence.
4. **Python numerics.** The library cards are Python floats printed by
   `json.dumps` (repr, e.g. `1e-05`), with `ensure_ascii` escapes and
   `sort_keys`. Python summation order, `round()` (banker's rounding) and
   numpy / scipy in `libix`'s family table must all be reproduced exactly.
   Rust's default float printing (ryu) differs from both Python repr and
   JavaScript.
5. **The acorn tokenizer** behind scorer 14's map-defect layer
   (`tokens14.mjs`), including its regex fallback on a file that does not
   tokenize. Mitigation: proof (b') checks the frozen layer shas, and the
   scorer itself only READS frozen layers.
6. **The proof data is on volatile paths.** The trees are in `/tmp` (an
   overlay that does not survive a container rebuild). The current cards
   exist only in the exp171 worktree. The exp184 library cards are untracked
   in the main checkout. L0 must copy all three before anything else.
7. **Locks.** `build-index.sh` takes `/work/heavy.lock` itself, and the
   standing rule is ONE lock layer. `cargo dev` must have one lock owner:
   long verbs take the lock themselves, with `HELD_LOCK` honoured, and are
   never wrapped in an outer `flock`.
8. **Concurrency census scope.** The dev crate's subprocesses, threads and
   waits are many. If they are allowlisted lazily, the pass / fail census
   becomes noise. Better: one subprocess helper with a deadline and drained
   pipes, allowlisted ONCE.
9. **Two lanes in parallel touching the CLI.** The crate rename lane and L1's
   registry extraction both edit `humanify-cli`. Land the rename first, or
   rebase L1 onto it.

**What I could not verify.**

- That a Babel-shape adapter over oxc can reach byte identity at all (risk 1).
  Nothing was built.
- That `score.py` run today reproduces the exp184 cards byte for byte (the
  oracle's own precondition). I did not run it: that is a heavy job.
- That `rescore.ts` at HEAD still prints `OLD-FIELDS IDENTICAL` on the six
  reference labels. Not run, for the same reason.
- That the TS and Rust wrapper-spelling predicates agree today. No shared
  vectors exist.
- The Earendil article and SlopCodeBench definitions. I used the brief's
  formulas without reading the sources.
- The exact contents of the parallel rename lane. No such branch is visible
  on this box, so the `humanify-ag-*` names here are taken from the brief.
- Whether `scripts/build-index.sh` (branch `exp184-index-spec`, "measured,
  not adopted") will land on main in that form.
- That every committed scorer-14 library label still has its raw reports on
  disk. The 9 exp184 labels do.
