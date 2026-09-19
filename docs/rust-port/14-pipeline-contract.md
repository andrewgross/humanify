# 14 — Pipeline contract: what the binary must produce, what the harness consumes

**Status: WP0.1 deliverable (2026-09-19), written by the implementing agent.**
Reviewed against a real run's artifacts — eval label `main-2026-09-18`
(`experiments/034-eval-harness/results/main-2026-09-18/`, scored at commit
`1813577`, all four pairs `exitCode 0`, preflight ok), with the emitted trees
of `/tmp/eval-work/main-2026-09-18/2.1.216/` and
`/work/exp050-cold/2.1.216/`. Every shape below was read from those files and
from the code that writes them; citations are `file:line` on branch
`rust-port` at WP0.1.

The harness (eval, neutrality, boot gates, walk driver) drives the pipeline
as a subprocess over file trees and reads JSON. The pipeline's side of that
contract — flags, exit codes, `ERROR:` blocks, tree layout, JSON shapes — is
what the Rust binary must reproduce so the harness cannot tell what language
produced it (02 §8). This document is that side, stated as a contract:
nothing here is optional, and every deviation is a gate failure, not a
compatibility negotiation (02 §9: the harness contract survives phase 6
because the measurement stack is not being rewritten — not as a TS-compat
promise).

## 1. Invocation and flags

Entry: `src/index.ts:7-16` — commander program, `cli()` from
`src/cli.ts:19-26` (`showHelpAfterError(true)`), `.version(pkg.version)`;
two commands: the pipeline (`configureUnifiedCommand`,
`src/commands/unified.ts:1415-1628`) and `env-reads`
(`src/commands/env-reads.ts:26-59`). Positional: `<input>` (the minified
JavaScript file).

**Pipeline flags** (declared in `unified.ts:1415-1628`; parse site / consume
site cited for each):

| flag                                       | short | type, default                                              | consume                                                                           |
| ------------------------------------------ | ----- | ---------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `--endpoint <url>`                         |       | string, `"https://api.openai.com/v1"`                      | provider, unified.ts:1032-1039                                                    |
| `--api-key <key>`                          |       | string, optional (env fallbacks below)                     | settings.ts:316-318                                                               |
| `--model <m>`                              | `-m`  | string, `"gpt-4o-mini"`                                    | provider                                                                          |
| `--output-dir <dir>`                       | `-o`  | string, `"output"`                                         | whole output tree                                                                 |
| `-v, --verbose`                            |       | count (repeatable; `-vv` = 2)                              | verbose.level 0..2, clamped `src/verbose.ts:59-64`; level 2 enables debug         |
| `-c, --concurrency <n>`                    | `-c`  | number, `50` (`DEFAULT_CONCURRENCY`, default-args.ts:3)    | LLM lane size + rate limiter                                                      |
| `--module-concurrency <n>`                 |       | number, default from bundler (esbuild 40 / other 20)       | settings deliberately leaves undefined                                            |
| `--max-tokens <n>`                         |       | number, optional                                           | provider + cache config                                                           |
| `--ambiguity-probe <path>`                 |       | path                                                       | prior-version/ambiguity-probe.ts:84-99                                            |
| `--disable <names>`                        |       | comma-separated                                            | kill-switch registry (below)                                                      |
| `--probe <names>`                          |       | comma-separated                                            | kill-switch registry                                                              |
| `--retries <n>`                            |       | number, `3`                                                | rate limiter                                                                      |
| `--timeout <ms>`                           |       | number, `300_000`                                          | provider                                                                          |
| `--llm-cache <dir>`                        |       | dir path, optional                                         | CachedLLMProvider outermost wrapper, unified.ts:1051-1057                         |
| `--reasoning-effort <l>`                   |       | enum `low\|medium\|high`, optional                         | parseReasoningEffort throws on bad value                                          |
| `--skip-libraries` / `--no-skip-libraries` |       | bool, `true`                                               | rename plugin + unminify                                                          |
| `--log-file <path>`                        |       | path; implies `-vv`                                        | appends debug+verbose stream, raises level to >= 2                                |
| `--diagnostics <path>`                     |       | path                                                       | arms three trails + writes report (§5.4)                                          |
| `--stats-json <path>`                      |       | path                                                       | writeEvalStats (§5.2)                                                             |
| `--bundler <t>`                            |       | enum `webpack, browserify, rollup, esbuild, parcel, bun`   | overrides detection                                                               |
| `--minifier <t>`                           |       | enum `terser, esbuild, swc, bun, none`                     | overrides detection                                                               |
| `--batch-size <n>`                         |       | number, `10`                                               | rename plugin                                                                     |
| `--max-retries <n>`                        |       | number, `2`                                                | maxRetriesPerIdentifier                                                           |
| `--max-free-retries <n>`                   |       | number, `100`                                              | maxFreeRetries                                                                    |
| `--lane-threshold <n>`                     |       | number, `25`                                               | laneThreshold                                                                     |
| `--split`                                  |       | bool, false                                                | the split path (§4)                                                               |
| `--prior-version <path>`                   |       | path, optional                                             | prior code, ledger discovery, vendor names, manifest factories, reconcile, relink |
| `--reconcile-prior-diff` / `--no-`         |       | bool, `true`                                               | settings lever                                                                    |
| `--naming-floor` / `--no-`                 |       | bool, `true`                                               | settings lever                                                                    |
| `--naming-floor-sweep` / `--no-`           |       | bool, `true`                                               | settings lever (explicit-only tracking)                                           |
| `--split-ledger <path>`                    |       | path, optional (auto-discovered next to `--prior-version`) | loadPriorSplitLedger                                                              |
| `--split-pure`                             |       | bool, false; requires `--split`                            | review tree instead of runnable CJS graph                                         |
| `--rename-ledger <dir>`                    |       | dir, optional                                              | writeRenameLedger (unified.ts:284-298)                                            |
| `--profile <path>`                         |       | path, optional                                             | Chrome Trace Events writer, unified.ts:1397-1413                                  |

**Not present (verified by grep, zero hits):** `--dump-artifacts` does not
exist yet (WP0.2 adds it); `--heap-mb` is NOT a pipeline flag — heap is set
by callers via `NODE_OPTIONS=--max-old-space-size=...` (run-pipeline.ts:192-193,
walk.sh:56).

**Flag preconditions** (`checkFlagInvariants`, unified.ts:144-186, applied
:1574-1578): `--split-pure` requires `--split`; `--split-ledger` requires
`--split`; `--naming-floor-sweep` requires `--naming-floor` (explicit-only);
`--bundler`/`--minifier` enum-checked. Violations print one lowercase
`Error: ...` line each and exit 1 (§2).

**`env-reads` subcommand** (env-reads.ts:26-59): positional `<path>`,
`--markdown`, `-o/--output <file>`; exit 1 on a missing path.

**Kill switches** (`src/kill-switches.ts:49-120`; generation 2, 2026-08-12:
no env vars, flags only). `--disable a,b` / `--probe c`, validated against
the registry at parse time; unknown or wrong-kind name →
`Error: <flag>: unknown <kind> switch "<name>" — valid: <list>` then exit 1
(unified.ts:213-215). Registry, in order: `family-permute`, `shingle-probe`,
`content-anchor`, `anchor-preempt`, `anchor-nearident`, `allsame-vote`,
`empty-decl-hash-guard`, `registrar-exemption`, `emit-align`, `name-align`,
`vendor-inherit`, `manifest-prior-order`, `post-split-reconcile`,
`fossil-split`. `switchOn(name)` is typed to the registry (a typo is a
compile error); `activeKillSwitches()` feeds the run manifest.

**Environment variables read by the pipeline:** only the API-key fallbacks
— `HUMANIFY_API_KEY`, then `OPENAI_API_KEY` (settings.ts:126 via
`src/env.ts:31-34`, which also loads `.env` via dotenv at module load) — and
the two generated-runner variables `HUMANIFY_STRIP_USING` and
`__HUMANIFY_USING_REEXEC`, which execute in the EMITTED TREE's `run.cjs`, not
in the pipeline (scoped out of the env guard per kill-switches.ts:23-26).
The `src/kill-switches.test.ts:112-138` guard enforces "no process.env
outside the allowlist" (`env.ts`, `runnable-scaffold.ts`, the env-reads
modules). Everything else (`NODE_OPTIONS`, `EVAL_HEAP`, `WRITE_TREES`,
experiment knobs) is caller-side harness territory, not pipeline contract.

## 2. Exit codes and outcome classes

Three outcome classes; the third exists only in the Rust era.

| class              | how it looks                                                                                                                                   | what the harness does                                                                                                                                                      |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| success            | `exitCode 0`                                                                                                                                   | artifacts are scored; status `{version, exitCode: 0, errors: []}`                                                                                                          |
| documented failure | nonzero exit, stdout carries an `ERROR:` block (headline line + indented continuation lines)                                                   | `run.sh` records status, prints the first 5 `ERROR:` lines, and CONTINUES the sweep ("Scoring anyway", run.sh:263-267) — a failed pair is logged and skipped, never aborts |
| crash              | process dies without a documented `ERROR:` block: OOM kill, uncaught exception, or — in the Rust era — a panic (exit 101, backtrace on stderr) | same recording, but `errors` will NOT start with `ERROR:` — the extraction must be able to tell the classes apart                                                          |

The TS side has two classes; the Rust side genuinely adds the third (a
`panic!` is a bug, not a documented failure). The run-status extraction
(experiments/lib/invariants.ts:69-84) is line-based and language-agnostic, so
it records whatever is there; the contract obligation is on the binary: a
documented failure MUST print an `ERROR:`-headline block; a panic must NOT be
disguised as one. `run-pipeline.ts:235-241` separately records the first 10
`ERROR:`-prefixed lines (no continuation) into the manifest's `errors` — the
two error lists differ by construction (60-line cap with continuations vs
10-line cap without; both counts are LINES).

**Producers of `ERROR:` blocks (all land in `<v>.stdout` because the harness
merges stderr into the same fd, run-pipeline.ts:185-196):**

- `src/commands/unified.ts:1346,1351` — output not valid JavaScript
  (`reportParseFailures`); sets `process.exitCode = 1`.
- `src/commands/unified.ts:1372,1375` — rename-invariant violations, rejected
  files preserved under `FAILED_OUTPUT_DIR/` (`reportSemanticFailures`);
  exit 1.
- `src/commands/unified.ts:1392` — internal errors during renaming
  (`reportInternalErrors`); exit 1.
- `src/output-validation.ts:363-365` — the token-divergence detail is
  deliberately INDENTED under the headline so `extractErrorBlocks` keeps it
  attached; that file's comment documents this contract. **This is the
  indentation rule the Rust side must reproduce: a continuation line starts
  with whitespace + non-whitespace (`/^\s+\S/`), any unindented line ends the
  block.**

`ERROR:` blocks were verified against real failures in sibling labels
(`results/exp048-cold/2.1.86.stdout:65-66`,
`results/exp050-cold/2.1.86.stdout:64-65`): headline on one line, indented
token-divergence detail beneath.

**Exact exit-code surface of the TS pipeline** (all sites, 2026-09-19):

- **Exit 0**: the default — nothing calls `process.exit(0)`.
- **Exit 1**: every documented-failure and usage-error path:
  `err()` in `src/cli-error.ts:1-4` (missing input file, red message to
  stderr); unknown/wrong-kind kill switch (unified.ts:213-215); flag
  invariants (unified.ts:225-226); `resolveSettings` throw (unified.ts:1607-1609);
  the three `process.exitCode = 1` report sites (unified.ts:1353, :1377,
  :1394); and every uncaught throw / unhandled rejection — there are NO
  `process.on("uncaughtException")`/`"unhandledRejection"` handlers in `src/`,
  so Node's default applies: exit 1, stack trace on stderr, AFTER the
  `try/finally` at unified.ts:1614-1627 still runs `finalizeProfile`,
  `renderer.finish()`, `finalizeLogStream`.
- **Exit 130**: SIGINT under the TTY dashboard (progress.ts:131-134).
- **Exit 2**: NOWHERE in `src/` — only the harness layer uses 2
  (run-pipeline.ts:195-197 missing config arg; walk.sh:44 unknown arg). The
  Rust binary should keep it that way.

**A second, lowercase `Error:` message family** (not captured by the
run-status extractor): flag/switch/settings failures print
`Error: <message>` via `console.error` with no stack (unified.ts:213, :225,
:1608). A run that dies this way records `exitCode 1` with `errors: []` in
`-run-status.json` — failed with no explanation lines. The contract keeps
this shape: usage errors are lowercase `Error:` + exit 1; runtime
declarations are uppercase `ERROR:` blocks + exit 1.

## 3. stdout/stderr surface

`<v>.stdout` (the merged stream) contains, in order: progress/prose output
(`Prior version: loaded from ...`), the coverage summary, `Split complete:
written to ...`, `Diagnostics written to ...`, `Eval stats written to ...`
(real: `results/main-2026-09-18/2.1.216.stdout`, ~28 KB). Progress UI writes
to stderr (`src/ui/progress.ts:152,300`). No contract obligation beyond:
`ERROR:` blocks parseable per §2, and the final `ERROR:`/success state
matching the exit code.

## 4. The output tree

Top level of an emitted tree (verified on `/tmp/eval-work/main-2026-09-18/2.1.216/`):

| entry          | kind | writer                                                                                                                                                                                                               |
| -------------- | ---- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `.humanify/`   | dir  | the next release's prior-version side (below)                                                                                                                                                                        |
| `RUNNABLE.md`  | file | runnable scaffold (src/split/runnable-scaffold.ts)                                                                                                                                                                   |
| `index.js`     | file | the runnable entry — ONLY on a runnable tree (below)                                                                                                                                                                 |
| `package.json` | file | runnable scaffold — `"name": "humanified-runnable"`, `"scripts": {"start": "node run.cjs"}`, best-effort `"*"` dependencies                                                                                          |
| `run.cjs`      | file | runnable scaffold — CJS loader shim; handles `using`/`await using` (skip under Bun; Node >= 24 runs natively; else re-exec with `--js-explicit-resource-management`; else fail loud unless `HUMANIFY_STRIP_USING=1`) |
| `src/`         | dir  | split source files (4,851 files / 685 dirs on 2.1.216)                                                                                                                                                               |
| `vendor/`      | dir  | vendor files (1,648 files)                                                                                                                                                                                           |

**Two tree flavors** (unified.ts:876-902): `--split` emits the runnable
live-binding CommonJS module graph by DEFAULT; `--split-pure` keeps the
byte-exact review slices. A runnable-emit decline or failure falls back to
the review tree LOUDLY (`Runnable emit declined: <reason> — writing
byte-exact review tree instead`) without losing the run. So:
runnable tree = `--split` without `--split-pure` and no decline → has
`index.js` + `run.cjs` + `package.json` + `RUNNABLE.md`; review tree = no
scaffold, no entry file, byte-exact slices. The reviewed reference run's
trees are runnable trees.

`.humanify/` contents (sizes on 2.1.216):

| file                             | size    | load-bearing?                                                                                                                                                                                            |
| -------------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `humanified.js`                  | 33.9 MB | YES — the canonical text; next run's `--prior-version` input; compared byte-wise by neutrality (never exempt)                                                                                            |
| `split-ledger.json`              | 19.0 MB | YES — §5; self-hop byte-compares it (`cmp -s`, run.sh:400)                                                                                                                                               |
| `placement-stats.json`           | 296 B   | diagnostic-only (neutrality DIAGNOSTIC_ONLY); read back by run-pipeline.ts:222-233 into the manifest's `placement`                                                                                       |
| `stage-hashes.json`              | 80 B    | diagnostic-only (DIAGNOSTIC_ONLY)                                                                                                                                                                        |
| `prior-match-map.json`           | 193 B   | "Never load-bearing" (writer's own comment); written only when debug (`-vv`) is on AND the map is non-empty, unified.ts:507-514                                                                          |
| `_bundle.js`, `__bun-runtime.js` | <1 KB   | bun factory helper shims (`_bundle.js` only when the plan has a bundle context, cjs-emit.ts:1608-1621; `__bun-runtime.js` written by bun-relink.ts:234-236, removed on the pure path unified.ts:645-649) |
| `failed/`                        | dir     | only on a failed run: rejected outputs + pre-rename sources preserved (`FAILED_OUTPUT_DIR`, failed-output.ts:39)                                                                                         |

Diagnostic-only exclusion is by BASENAME and VISIBLE
(`experiments/lib/neutrality.sh:243`): `placement-stats.json`,
`stage-hashes.json`. Everything else in the tree is byte-compared by the
neutrality gate; `split-ledger.json` and `vendor/_bun-modules.json` are the
hash-bearing exceptions that phase 5a compares via
`humanify-parity compare-ledger` instead (07 §6).

## 5. JSON shapes the harness reads

### 5.1 `.humanify/split-ledger.json`

Writer: `src/commands/unified.ts:425-429` (`writeSplitLedger`, called at
:724 and :902) — `JSON.stringify(ledger)` COMPACT, no indent. Reader:
`loadSplitLedger` unified.ts:330-334 (rejects other `version` values);
consumers: analyze.ts, run.sh artifact checks, self-hop `cmp -s`.

Top-level keys in order (verified on 2.1.216):

| key             | shape                                                                                |
| --------------- | ------------------------------------------------------------------------------------ |
| `version`       | number, `1`                                                                          |
| `files`         | array of N files (4,851), tree-relative paths                                        |
| `nameToFiles`   | object keyed by exported binding name → array of file paths (55,098 keys on 2.1.216) |
| `order`         | array of 35,903 strings (statement → file)                                           |
| `hashes`        | array of 35,903 16-hex strings                                                       |
| `emitHashes`    | array of 35,903                                                                      |
| `emitNames`     | array of 35,903 comma-joined name strings                                            |
| `hashVersion`   | number, `1`                                                                          |
| `fossilModules` | array of ~4,850 objects `{file, hashes[], imports[], declared[], ...}`               |
| `aliases`       | object keyed by file path → alias string                                             |
| `emitIndexes`   | array of 35,903 integers                                                             |

Ledger structure is defined at `src/split/stable-split.ts:207-312`. In the
Rust era these persist as UTF-8 byte-offset spans converted at dump time
(07 §1) — the TS dumper converts; the ledger format itself does not change
during the port.

### 5.2 `<v>.stats.json` (`--stats-json`)

Writer: `src/commands/unified.ts:452-505` (`writeEvalStats`, called at
:1282-1290 when `opts.statsJson` and coverage data exist), compact
`JSON.stringify(stats, null, 2)`. Top-level keys in order: `coverage,
transferStats, priorVersionApplied, priorVersionAlreadyNamed,
priorVersionBindingsApplied, namingFloor, closeMatchStats, resolutionStats,
bindingResolutionStats, vendorNaming, renameClaims, selection`.

- `coverage` → `functions`, `moduleBindings`, `identifiers` (same key set;
  `identifiers` adds `skippedBySkipList`), `llm` (`totalCalls, retries,
avgResponseTimeMs, totalTokens, inputTokens, outputTokens`), `elapsedMs`,
  `mintedCensus`.
- `transferStats` → `exactMatch`, `closeMatch`, `statementTwin`, `retry`,
  each `{attempted, applied, skipped, rejected{...}}` (`retry` has no
  `rejected`).
- `resolutionStats` / `bindingResolutionStats` → the ResolutionStats counter
  set (`src/analysis/types.ts:515`) including `singletonRejected`,
  `singletonUnguarded`, `injectivityDemoted`, `crossedContainerRevoked`,
  `stillAmbiguous`, `unmatched`, `propagationByRung{...}`,
  `enclosingStmtAbstain{...}`.
- `vendorNaming` is OMITTED ENTIRELY when the vendor namer was never asked.
- `renameClaims` → `{ledgerOnlyRejections, byGuard{...}, claimsRecorded}`.
- `selection` → `{bundler, bundlerTier, minifier, unpackAdapter}` (the
  PipelineSelectionRecord from `src/pipeline/selection-record.ts:4-11`,
  deterministic from the input's detection result plus overrides).

`renameClaims` is written even when all-zero; `vendorNaming` is dropped when
the namer was never asked. Both facts are shape-load-bearing: a consumer
must treat an absent `vendorNaming` as "never attempted", not as zeros.

### 5.3 `.humanify/placement-stats.json` and `stage-hashes.json`

- `placement-stats.json` (writer `writePlacementStats` unified.ts:402-424,
  called :931): `{statements, files, folders, inherited, residueLocality,
byTier}` where `byTier` is keyed by the ten placement tiers `hash, preempt,
anchorPreempt, ordinal, name, allsame, fill, anchor, conflict, novote`
  (`src/split/layout.ts:56`). Counters only, no per-item rows — the
  per-item record is the diagnostics report's `placementTrails`.
- `stage-hashes.json` (writer `writeStageHashes` unified.ts:384-392, called
  :910): `{"afterNaming": <16-hex>, "afterPlacement": <16-hex>}` — hashes of
  the ledger serialization (stageFingerprint, unified.ts:912).

### 5.4 `--diagnostics` report

Writer: `src/rename/diagnostics.ts` (`DiagnosticsReport` interface :184-238,
`writeDiagnosticsFile` :397-401, `JSON.stringify(report, null, 2)` + trailing
newline); written at unified.ts:1265-1280 ONLY when `--diagnostics` is
passed; trails armed at unified.ts:1125-1134
(`strategyTrail/placementTrail/nameContention .reset(Boolean(opts.diagnostics))`).
Real size: 62–108 MB per pair.

Top-level keys in order: `timestamp, coverage, transferStats, unrenamed,
renamed, strategyTrails, placementTrails, nameContention, identifierLedger,
patterns`.

- `strategyTrails` → `{trails[], funnel{}}` — the per-identifier attempt
  trail (202,625 trails on 2.1.216); funnel keys are the tier names.
- `placementTrails` → `{tiers{}, trails[]}` — 35,903 entries
  `{index, names[], placedBy, file}`; NOTE the tiers vocabulary here is
  `fossil:*` (placement-trail's own registry) while placement-stats.json's
  `byTier` uses the layout.ts registry — two different tier vocabularies in
  the same run; the dump schema (WP0.2) keys placement rows by statement span
  and carries BOTH vocabularies explicitly rather than guessing one.
- `nameContention` → `{events[]}` (`requested, resolvedTo, oldName, site`).
- `identifierLedger` → `{totalBindings, transferSettled{...}, llmNamed,
libraryPrefix, fallback, notRenamed, remainingMinted, terminalState}`.

### 5.5 `.humanify/prior-match-map.json`

Writer: `writePriorMatchMapDebug` unified.ts:507-514, called :903, gated on
debug.enabled AND a non-empty map. Flat object finalName → priorName.
Declared "Never load-bearing" — a debug artifact; the dump schema (WP0.2)
span-keys the same data instead.

## 6. Harness-written records (consumers, not binary outputs)

These are written BY THE HARNESS around the pipeline, from the pipeline's
observables. The binary does not produce them; the contract lists them
because their extraction rules constrain what the binary may print and exit.

### 6.1 `<v>-run.json` — run manifest

Writer chain: `run.sh:234-251` writes a runcfg JSON
(`/tmp/eval-work/<label>/<v>.runcfg.json`); `run-pipeline.ts` spawns
`npx tsx <repo>/src/index.ts <args>` (:185-196), samples peak RSS from
/proc VmHWM every 250 ms (:200-208), counts cache entries before/after
(:182, :217), extracts errors (:235-241), reads `.humanify/placement-stats.json`
(:222-233), and calls `writeManifest` (:289 → run-manifest.ts:189-198).

`RunManifest` shape (run-manifest.ts:32-89), keys in order: `pair, label,
startedAt, wallSeconds, provenance{commit, dirty, node, bun},
inputs{input, prior, priorKind}, config{endpoint, model, reasoningEffort,
concurrency, heapMb, killSwitches[], cache{enabled, entriesBefore,
entriesAfter, written}}, outcome{exitCode, errors[], peakRssMb?,
artifacts[{path, bytes}]}`, optional `placement{statements, files, folders,
inherited, residueLocality, byTier}`. `priorKind` ∈ `"rebased" | "archive" |
"unknown"` (regex `/-rebased(\/|$)/`, run-manifest.ts:102-106).
`manifestWarnings` (:236-310) runs six checks: cache-replayed, archive-prior,
kill-switch-active, dirty-tree, heap-headroom, nonzero-exit. Consumer:
summarize.ts; the mixed-commit guard reads `commit.txt`, not the manifest
(scripts/eval.ts:72-92).

### 6.2 `<v>-run-status.json` — pair run status

`PairRunStatus` (invariants.ts:37-44): `{version, exitCode, errors[]}`.
Writer CLI: `npx tsx experiments/lib/invariants.ts <results-dir> <version>
<exit-code>` (run.sh:261-262, `PIPELINE_RC` captured at :253). Reads
`<v>.stdout` only when `exitCode !== 0`; `errors` = `extractErrorBlocks`
(:69-84): a block starts at a line beginning `ERROR:`; continuation lines
match `/^\s+\S/`; any unindented line ends the block; cap 60 LINES.

### 6.3 Boot and self-hop verdicts

- `<v>-boot.json`: `{"boot":{"version","prompt","ok"[,"rerecorded"]}}` —
  printf-written at run.sh:344-345; `bun run.cjs --version` (:329) and
  optional live prompt round-trip (:335) with `BOOT_GATE_MODEL`
  (experiments/lib/boot-gate.sh:41, default `claude-haiku-4-5-20251001`).
  The `rerecorded` field is added BY HAND after re-recording (no code writes
  it); readers ignore extra fields.
- `<v>-self-hop.json`: `{"selfHop":{"version","ran","identical","diffLines"}}`
  (run.sh:407-408; missing-artifact branch `{"ran":false,"identical":null,
"diffLines":null}` :390-391). The self-hop leg re-runs the pipeline on its
  own output WITHOUT `-vv`/`--log-file`/`--stats-json`/`--diagnostics`
  (run.sh:371-376) and byte-compares the split ledger (`cmp -s`, :400).
  Expected cold-identical is FALSE on cold runs (rule 10); only reachable warm.

### 6.4 Scorecard and summary

- `<v>.json` — analyze.ts's stdout redirected by run.sh:305-309; shape
  `{pair, determinism{...}, churn{...}}` (analyze.ts:337). Optional
  `layout`/`vendor` blocks present only when the tree paths exist.
- `summary.json` — summarize.ts:232-243; `{model, provenance{...}, pairs[4],
totals{31 flat counters}, runStatuses[4], verdicts{boots[], selfHops[],
preflight}}`.
- `commit.txt` — run.sh:158, the short sha the mixed-commit guard compares
  (scripts/eval.ts:72-92). **Known trap, observed in the reviewed run:
  `commit.txt` said `1813577` while all four manifests recorded `c4efd3e`
  (a commit landed mid-sweep; `c4efd3e` is a child of `1813577`) and three
  trees carried `dirty: true`. The manifest is the newer, more truthful
  record; the guard reads the stale one. A Rust-era re-run must not rely on
  `commit.txt` being current.**
- `preflight-status.json` — run.sh:121-122;
  `{"preflight":{"verdict":"ok|regressed|not-verified|skipped","status":<n>}}`.

## 7. What the harness passes the pipeline (verified invocations)

Scored leg argv (run.sh:228-233, verbatim flag order): `<INPUT> --split
--endpoint <ep> --model <m> --api-key <k> --reasoning-effort <e> -c <conc>
-o <OUT> [--llm-cache <dir>] --prior-version <PRIOR> --stats-json <STATS>
-vv --log-file <LOG> --diagnostics <DIAG>`. Rebase leg (run.sh:198-203) and
self-hop leg (run.sh:371-376) pass subsets; walk driver
(experiments/076-statement-placement/walk.sh:76) its own set.

The walk driver's per-hop invocation
(experiments/076-statement-placement/walk.sh:76-82) is the same flag set with
`--stats-json`, optional `--prior-version` (hop 1 cold) and optional
`--disable <names>`; after each hop it requires
`$OUT/.humanify/humanified.js` to exist (walk.sh:84-87), runs the boot gate
on `$OUT/run.cjs`, and chains `PRIOR=$OUT/.humanify/humanified.js` — the
lineage. No LLM cache on gate runs (walk.sh:30-31, rule 10). Known doc drift
in that file (2026-09-19): its header comment names a
`--disable fossil-settled-anchor` switch that is NOT in `KILL_SWITCHES` —
passing it would be a fatal unknown-switch error; the walk's default
invocation passes nothing.

The scored leg also passes `-vv` and `--diagnostics`; the self-hop leg
passes NEITHER (plus no `--stats-json`, no `--log-file`, run.sh:371-376) —
so any code that changes behavior when trails are armed is caught by the
self-hop byte-compare on the ledger. That is the design property WP0.2's
dump flag must share: trails armed by `--diagnostics` (and later
`--dump-artifacts`) must not change decisions, and the self-hop leg is one
of the instruments that would catch a violation.

## 8. The contract's obligations on the Rust binary, summarized

1. Flags: the full invocation surface in §7 must parse identically (flag
   parity table, 02 §9's migration-scaffolding rule: kept because the TS
   measurement stack spawns it).
2. Exit codes: 0 success; 1 documented failure (with `ERROR:` blocks per §2);
   the panic class (§2 third row) must remain distinguishable — never emit a
   fake `ERROR:` headline from a panic path.
3. Tree layout: §4 exactly, including `.humanify/` filenames and the
   diagnostic-only basenames.
4. JSON shapes: §5 exactly — `JSON.stringify(..., null, 2)` indent-2 for
   stats/diagnostics/status files, compact for split-ledger.json (no trailing
   newline), indent-2 + trailing newline for the diagnostics report.
5. stdout: `ERROR:` headline + indented-continuation convention (§2);
   progress on stderr.
6. Everything else in the run (log prose, timing, token metrics) is not part
   of the contract (07 §11).

## 9. Observations from the reviewed run (rule-9 notes for the owning docs)

- The `main-2026-09-18` sweep crossed a commit mid-run
  (`commit.txt` `1813577` vs manifests `c4efd3e`, three `dirty: true`
  trees). The manifests are the per-run truth; CLAUDE.md's "check a label's
  run-status before citing it" should extend to reading the manifests' own
  commit fields. Recorded here; CLAUDE.md itself is not edited by this WP.
- The `-boot.json` `rerecorded` field is hand-added, not code-written —
  anything tooling that parses boot verdicts must ignore unknown fields
  (the current reader does).
- `prior-match-map.json` in the tree exists only under `-vv` AND a non-empty
  map — its absence is not a failure; the self-hop leg legitimately lacks it.
- The self-hop leg's `identical: false, diffLines: 96` on this reference is
  expected on a cold leg and does not invalidate the label (rule 10).
