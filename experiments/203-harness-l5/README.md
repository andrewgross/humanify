# exp203 — harness migration lane L5: delete the old harness, move the data to `dev/`, switch the gate

STATUS: DONE, NOT MERGED (2026-11-08), branch `harness-l5-deletion` (pushed
for Andrew's review of the CLAUDE.md rewrite and the deletion list). Scope:
`docs/dev-harness.md` §5 lane L5 under the 2026-11-07 decisions (keep the
webcrack shim and node; the scorer is a new version; the walk is ported).
Tag **`experiments-code-final`** = `23ee9b9da` (main before this lane) holds
every deleted file.

## Totals

- **Deleted: 782 files, about 95,000 lines.** 776 harness and old-experiment
  code files (92,899 lines) plus the TypeScript toolchain's six config files
  (tsconfig, knip, biome, prettier x2, husky); package-lock.json shrank from
  217 to 112 entries (-1,760 lines, a pure subset: no version moved).
- **Moved byte for byte: 28,785 files** (every one `R100` in git) into
  `dev/`: 28,581 committed label files (578 eval, 28,003 library), 183
  library corpus / layer manifests, 13 launch goldens, 5 config files, 2
  census allowlists, 1 `.gitignore`. One
  more rename: `scripts/webcrack-shim.ts` -> `tools/webcrack-shim.ts`.
- **Kept, and why:** experiment READMEs, RESULTS, data, corpora and fixtures
  (lab notebooks); `tools/webcrack-shim.ts` (the binary runs it); and **34
  files (6,320 lines) left for exp202**, which is porting them now (§4).
- **The gate is `cargo dev check`** (11 stages). `npm run check` and every npm
  script are gone; `package.json` holds only `webcrack` + `tsx`.
- **Docs rewritten:** CLAUDE.md (Checks, Validating, references, code style),
  README.md, docs/responsibility.md (29 harness owners re-pointed, 6 dead rows
  removed), docs/concurrency.md, docs/measurement-pitfalls.md,
  docs/dev-harness.md (status), docs/design/library-index.md,
  experiments/README.md, the 034 and 096 READMEs, three test/e2e READMEs;
  docs/e2e/ marked historical.

## 1. What was deleted

Commit `643072333` (code) and `f8abed570` (toolchain). By lane README:

| Ported by | Deleted                                                                                                                                                                                                                                                                                                         |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| L2a       | `experiments/034-eval-harness/{analyze,statements,kpis,surfaces,build-constant-churn,name-only-churn,trail-report,diff-ledger,summarize,leaderboard,noise-bands}.ts` (+ tests), `experiments/170/rescore.ts`, `055/real-ledger.ts`, `037/diff-composition.ts`, `046/{vendor-churn,relocation-pairing}.ts`, `experiments/lib/{diff,trees}.ts`, `lib/js/*` |
| L2b       | `096/{leaderboard,determinism}.py`, `106/scorer5/chained.py`, `120/scorer9/chained9.py`, `128/scorer11/chained11.py`, `125/scorer10/origins10.py`, `129/scorer12/origins12.py`, `172/score.py`, `test/library-truth-decoder.test.ts`                                                                                                      |
| L3        | `scripts/eval.ts`, `scripts/build-index.sh`, `034/run.sh`, `034/run-launch.test.ts`, `096/{run.sh,run.py}`, `experiments/lib/{pipeline-bin,pipeline-config,run-pipeline,run-manifest,invariants}.ts`, `lib/{neutrality,build-bin,boot-gate,llm-endpoint}.sh`, `experiments/walk/*`, `test/{launch-goldens,eval-score-args,boot-gate-env}.test.ts` |
| L4        | `scripts/{check,e2e,clone-census,concurrency-census,ask-trace,diff-asks}.ts`, `scripts/{format-golden,rust-env-check,rustfmt-staged}.sh`, `scripts/lib/launch-golden.ts`, `experiments/lib/match-truth/*`, `test/{measurement-owners,no-private-addresses,clone-census,concurrency-census,e2e-known-gaps,ask-trace-harness,launch-golden,eval-library-args}.test.ts`, `test/launch-recorder.ts`, `test/fixtures/clone-census/`, `.husky/` + lint-staged |
| RETIRE    | `034` ceiling / residual / sankey one-offs, `experiments/lib/detect-census.ts`, `096/{handcheck_draw,lazy_modules,merge_index}.py`, `152-full-bun-index/scripts/*`, `181/scripts/ix.sh` (L3: not ported, no living caller), `029/lib/metrics.test.ts` |
| old code  | every other `.py .ts .mts .mjs .cjs .sh .rs`, proto `Cargo.toml`/`Cargo.lock` and `.patch` under `experiments/NNN-*/` (no living caller), except the corpus and fixture data and exp202's files                                                                                                                                                                                  |

Before deleting, every remaining file's references were checked: no crate
reads an experiment script (the only string mentions are comments naming a
port's origin and a fixture's recorded path), and the kept exp202 files
import only kept files (closure computed by import resolution, §4).

## 2. What moved where (commit `c3015d4ae`, all R100; loaders in `95efb22c5`)

| From                                                                                         | To                                |
| -------------------------------------------------------------------------------------------- | --------------------------------- |
| `experiments/034-eval-harness/{pairs,noise-bands,self-hop-reference}.json`                   | `dev/eval/`                       |
| `experiments/034-eval-harness/run-launch*.golden.txt`, `launch-goldens/*`                    | `dev/eval/goldens/` (all 13)      |
| `experiments/034-eval-harness/results/` (+ its `.gitignore`)                                 | `dev/eval/results/`               |
| `experiments/096-library-matching-score/{corpus,origins*,copies*,chained*,misattr13*,mapdefect14*}*.json` | `dev/library/corpora/` (183) |
| `experiments/096-library-matching-score/{bands,data-roots}.json`, `results/`                 | `dev/library/`                    |
| `scripts/{concurrency,clone}-allowlist.json`                                                 | `dev/census/`                     |
| `scripts/webcrack-shim.ts`                                                                   | `tools/webcrack-shim.ts`          |

- `crates/humanify-ag-dev/src/paths.rs` is still the only place a path is
  named (new: `LIBRARY_CORPORA`); the golden loader reads one directory.
- **One stated content change** after the move: the two run-launch goldens
  and the cli `launch-parity.json` fixture name the results directory, so
  `experiments/034-eval-harness/results/` -> `dev/eval/results/` (40 lines).
- **New test** `every_committed_dev_file_is_read_by_its_loader`: every file
  under `dev/` except the labels must be returned by a typed loader. Red seen
  twice: a planted stray file, and the corpora path left at the old home.
- The product's default shim path (`unminify.rs`), its `--help` text and help
  golden, and the `shims` stage follow the shim to `tools/`.
- `.gitignore`: `dev/{eval,library}/results/*` ignored like
  `experiments/*/results/*` (references are force-added, as before).
- The exp202-pending Python reads and writes under `dev/library/`
  (`lib096.CORPORA_DIR` / `RESULTS_DIR`; one-line edits in truth_audit,
  origins7, score, copies2, misattr13, mapdefect14, vendor_precision).
  Checked: every bun-dev layer path resolves, `corpus.py verify --corpus
  kind-rollup` reads the moved manifest (`corpus OK`).

## 3. The gate and the hook

- `cargo dev check` is THE gate. `check_test`'s check.ts-equality test went
  with check.ts (its own comment said so); the 13-stage `SUCCESSORS` mapping
  test stays and passes.
- `package.json`: `webcrack` + `tsx` (the binary runs `npx tsx
  tools/webcrack-shim.ts`), no scripts, no devDependencies. Deleted:
  tsconfig.json, knip.json, biome.json, .prettierrc, .prettierignore, .husky/.
- The hook: `.githooks/pre-commit` -> `cargo dev precommit`. **Install once
  per clone: `git config core.hooksPath .githooks`** (CLAUDE.md "Setup").
  This lane's own commits ran it (`git -c core.hooksPath=.githooks commit`):
  33, 10, 3 and 19 staged files ok. The shared git config was NOT changed
  (other lanes' worktrees share it and still run husky until they rebase).
- `main.rs`: the stale `today:` strings naming deleted commands are blanked
  (two left: `library corpus` / `library layers`, whose Python exp202 is
  porting). Follow-up, not done to keep the shared verb table conflict-free
  for exp202: with every verb ported, the `lane` / `today` fields are dead.

## 4. Left for exp202 (34 files, 6,320 lines)

exp202 is porting these now; a final small cleanup deletes them when it lands.
The list is exp202's brief plus the import closure of its Python (computed by
resolving every `import` against the `sys.path` inserts the files make):

- `experiments/096-library-matching-score/{score,lib096,truth_audit,decoder,origins,families_lib,corpus}.py`
- `experiments/095-library-fingerprints/misses/scripts/{mmlib,decompose,truth}.py`, `selection/scripts/{libix,closure}.py`
- `experiments/129-helpers-copies/copies2.py`, `132-factory-precision/scorer13/misattr13.py`,
  `137-scorer-mapdefect/scorer14/{mapdefect14.py,tokens14.mjs}`
- `experiments/112-test-precision/scorer7/{origins7,bundled}.py`, `104-truth-audit/fetch_metadata.py`
- `experiments/181-index-all-bundlers/scripts/{kind_corpus.py,prep.sh}`,
  `100-test-split/scripts/{build.py,freeze_test.py,webpack-build.cjs}`
- `experiments/150-bun-testbed/scripts/{layers.sh,layers2.sh}` (the layer
  order exp202 ports; `layers.sh` also calls the L2b-ported builders, now deleted)
- `experiments/166-restore-or-ask/{bundev-precision.sh,stub-run.mts,vendor_precision.py,aggregate_precision.py}`,
  `172-cjs-app-placement/bundev-runs.sh`
- `scripts/lib/stub-llm.ts` (stub-run.mts imports it)
- `test/library-truth-{mapdefect14,origins7}.test.ts` (tests of the above; no
  runner since the TS gate went)

The exp095 venv (`/work/exp095-library-fingerprints/scale/.venv`) is untouched.

## 5. Proof

| Proof                                                                                                          | Result                                                                                                                                                                                                                                  |
| -------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo dev check` in the worktree (`f8abed570`: data moved, code deleted, toolchain dropped)                   | **ALL 11 STAGES PASSED** (census:clones REVIEW, advisory: 45 findings); rust:unit 307 s, rust:build 136 s, e2e 80 s: 19 fixture pairs, 152 launches equal to `dev/eval/goldens/e2e.golden.txt`                                          |
| `cargo dev check` from a CLEAN checkout (`6df3350d6`, the last code commit; fresh `npm ci --ignore-scripts`)  | **ALL 11 STAGES PASSED** (census:clones REVIEW, 45); rust:unit 312 s (`cargo test --workspace`), rust:build 136 s, e2e 80 s |
| `cargo test --workspace`                                                                                        | inside both gate runs (rust:unit); `cargo test -p humanify-ag-dev -p humanify-ag-cli` also run alone after the deletions: all pass once node_modules was installed (the one red was the webcrack test without `webcrack`, as expected) |
| Every file under `dev/` read by its loader (new test)                                                          | pass; red seen with a planted stray file and with `LIBRARY_CORPORA` at the old home                                                                                                                                                    |
| `eval leaderboard` on a moved LEGACY label set (v1: `session-2026-08-05 exp049-cold-control exp049-cold --force-mixed`) | the main binary (`23ee9b9da`, labels at `experiments/034-eval-harness/results/`) and this branch's (labels at `dev/eval/results/`) print **byte-identical** output (71 lines)                                                             |
| `eval leaderboard` on the v2 refs (`ref-names-0357696e-r1/r2/r3-s2`)                                           | **byte-identical** (44 lines): main reading them from the main checkout with `--results`, this branch from `dev/eval/results/` (copied there, gitignored); novel 4,191 (=) / realLn 416,386 (=)                                        |
| A real run after the move: `cargo dev eval score l5-smoke-6df3350d --pairs 85->86` (cold, names mode, scratch base, scorer v2) | exit 0; written under `dev/eval/results/` (ignored); boot OK; warm self-hop byte-identical with 0 cache writes; **novel 787 / realLn 78,791**, equal to L3's TS-vs-Rust run on the same pair (exp198)                                     |
| The webcrack shim at `tools/`                                                                                  | `npx tsx tools/webcrack-shim.ts` unpacks (prints its JSON line); the cli's webpack pipeline test and e2e pass with the new default path                                                                                                |
| The exp202 Python against the moved data                                                                       | every bun-dev layer path resolves under `dev/library/corpora/`; `corpus.py verify --corpus kind-rollup` prints `corpus OK`                                                                                                             |
| The pre-commit hook (`.githooks/pre-commit`)                                                                   | ran on this lane's commits: 33, 10, 3 and 19 staged files ok                                                                                                                                                                           |


## 6. `git grep -nE "npm run|npx tsx|python3? .*\.py"`: every hit explained

758 hits (`git grep -nE "npm run|npx tsx|python3? .*\.py"` on the branch head):

| Where                                                                                     | Hits | Why it stays                                                                                                                                                                         |
| ----------------------------------------------------------------------------------------- | ---: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `experiments/NNN-*/**/*.md` (READMEs, RESULTS)                                            |  493 | historical lab notebooks; experiments/README.md says such a command names a script at tag `experiments-code-final`                                                                  |
| exp202's kept files (`truth.py`, `closure.py`, `build.py`, `fetch_metadata.py`, `bundled.py`, `layers.sh`, `166/*`, `172/bundev-runs.sh`, `kind_corpus.py`) |   17 | left for exp202 (§4): their own usage lines                                                                                                                                          |
| `experiments/153/155 results/sim-*.json`                                                   |    3 | recorded data (a command line captured in a result)                                                                                                                                  |
| `dev/eval/goldens/library-{agreement-sets,structural}.golden.txt`                          |    4 | recorded launch goldens (the TS runner's `python :: … corpus.py / score.py` lines), held by L3's `library_test` as recorded; moved byte for byte                                    |
| `docs/rust-port/*`, `docs/spec/*`, `PORTING.md`, dated plans / issues / reviews / audits, `docs/perf-inventory.md`, `docs/config-consolidation.md`, `docs/matching-architecture.md`, `docs/roadmap-noise-reduction.md`, `docs/library-matching-config.md`, `docs/design/library-names-plan.md`, `docs/plugin-spec.md` | ~190 | historical records (port ledger, dated designs, "the gate passed" logs); rewriting history would falsify them                                                                       |
| `docs/e2e/*`                                                                               |   31 | the pre-cutover TS fingerprint harness; each file now carries a HISTORICAL note                                                                                                      |
| `docs/dev-harness.md`                                                                      |    4 | the design's inventory of the old tools (STATUS now says built)                                                                                                                      |
| `docs/concurrency.md:199`, `crates/humanify-ag-core/src/{subprocess.rs,subprocess/subprocess_test.rs,unpack/webcrack.rs}`, `crates/humanify-ag-cli/src/main.rs`, `tools/webcrack-shim.ts`, `CLAUDE.md:51` | 7 | the webcrack shim: the binary runs `npx tsx tools/webcrack-shim.ts` (kept by decision)                                                                                               |
| `CLAUDE.md:27`, `experiments/README.md:29,38`                                              |    3 | name the deleted commands to say they are gone                                                                                                                                       |
| `crates/humanify-ag-dev/src/{main.rs:231, runners/library_test.rs, match_truth/canonical_test.rs}` | 3 | `library corpus`'s `today:` (exp202's corpus.py); comments on how a golden / vector was recorded                                                                                    |
| `crates/humanify-ag-core/src/modules/vendor_names_test.rs`, `crates/humanify-ag-index/src/cli.rs`, `crates/humanify-ag-cli/tests/fixtures/runtime-build-esbuild/bundle.js` | 3 | a comment on a recorded TS-era request; "npm run inside each install" (the index builder runs npm); minified fixture bytes that happen to match                                    |


## 7. CLAUDE.md, what changed

177 lines in, 112 out (`git diff main -- CLAUDE.md`):

- **Checks**, rewritten: `cargo dev check` and its 11 stages (what each
  proves), the two ADVISORY stages, `--only` = PARTIAL, `STAGES` in
  `gate/check.rs`, the one-time setup (`git config core.hooksPath .githooks`,
  `npm ci --ignore-scripts` for the shim), what `cargo dev` is and tag
  `experiments-code-final`, the only remaining JavaScript, the individual
  stage commands as `cargo`/`cargo dev` spellings, the dev tools.
- **Validating**, commands: `cargo dev eval score|neutrality|leaderboard|walk`,
  `cargo dev library score`; the verb table and the harness key registry are
  the places to look; `measurement_owners.rs` replaces measurement-owners.test.ts;
  `eval neutrality`, `cargo dev build-bin`, `boot.model`, `--base-mode
  seeded|archive`, `/work/eval-work`; the retired `--no-layout` /
  `--no-vendor-score`.
- **New paragraph: scorer v2** (L2a's proposed text): new scorer version,
  `scorerVersion`, the 3-statement v1/v2 difference, the leaderboard refuses
  mixed versions, `eval rescore`, `byScorer.2` bands.
- **References**: a new first bullet for the v2 refs `ref-names-0357696e-r1/r2/r3-s2`
  and `ref-scratch-0f338ffa-r1/r2/r3-s2` (novel 4,191 / realLn 416,386), and
  that labels live (uncommitted) under `dev/eval/results/`; the example
  leaderboard command now uses `ref-names-0357696e-r1-s2`. Every measurement
  rule, number and reference history below it is unchanged.
- Paths: `dev/eval/{noise-bands,self-hop-reference}.json`,
  `dev/eval/goldens/run-launch*.golden.txt`, `dev/census/*-allowlist.json`.
- **Code style**: Biome and TS tests gone (clippy covers the harness), twelve
  concurrency patterns, a new bullet on `dev/` and its loaders (and the test
  that fails an unread file), e2e is `cargo dev e2e`.


## 8. After the merge (the main checkout)

1. `git config core.hooksPath .githooks` (today it names the main
   checkout's `.husky`, which the merge deletes; git then runs no hook).
2. `npm ci --ignore-scripts` (the old node_modules is a superset; harmless).
3. Move the LOCAL labels: git moves only the tracked ones, so the untracked
   labels (the v1 and v2 references, every local run) and the logs beside
   committed ones stay at the old paths:
   `rsync -a --ignore-existing --remove-source-files experiments/034-eval-harness/results/ dev/eval/results/`
   and the same for `experiments/096-library-matching-score/results/` ->
   `dev/library/results/`, then `find <old dir> -type d -empty -delete`.

## 9. What I could not prove

- The pre-commit hook as the INSTALLED hook of the shared clone: I ran it
  per commit (`git -c core.hooksPath=.githooks`), and did not change the
  shared git config (§3).
- exp202's builders end to end against the moved data: only path resolution
  and `corpus.py verify` were run; a full layer build is exp202's own proof.
- The tokenizer `tokens14.mjs` needs `acorn`, which is not in this repo's
  lock (it was not before either); where exp202 resolves it from is theirs.
- The eval's summary prints "NOTE: self-hop diverged for 2.1.86 (0 diff
  lines)" on a 0-line cold self-hop: a wording bug in `eval summarize`,
  older than this lane (not touched here).
- Leaderboard identity was checked on one legacy set and one v2 set, not on
  every label; the JSON-spellings and backfill tests read every moved label
  through its loader (`json_test`, `backfill_test`), and they pass.

