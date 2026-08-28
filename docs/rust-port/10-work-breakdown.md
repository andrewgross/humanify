# Work breakdown and sequencing

The execution plan behind `03-migration-plan.md`: the phases broken into work
packages with contents, dependencies, gates, and lanes two agents can run
concurrently. Sizing comes from a per-file census of `src/` on branch
`plan/rust-port` (non-test `*.ts`, `wc -l`). No calendar dates — magnitude
bands only, at the end. Gate definitions are `03-migration-plan.md`'s; the
runner commands and pass criteria for the parity gates live in doc 07 (parity
gates and oracle lifecycle). Where a gate is an existing instrument, its real
path is named.

## 1. Scope totals

**Total port surface: 143 non-test files, 44,393 LOC in `src/`.** Alongside
them: 116 colocated test files, 40,763 LOC (measured this branch) — the
behavior spec to mine while porting, kept green on TS until phase 6. Harness-
side tests (`test/` 672 LOC, `experiments/*/lib/` 1,398 LOC) are instruments
and do not port (`02-rust-target-architecture.md` §8).

| dir                   | files | LOC        |
| --------------------- | ----- | ---------- |
| **TOTAL src/**        | 143   | **44,393** |
| src/rename            | 34    | 14,036     |
| src/split             | 21    | 9,597      |
| src/analysis          | 13    | 6,673      |
| src/prior-version     | 5     | 3,964      |
| src/llm               | 8     | 1,980      |
| src/commands          | 4     | 1,903      |
| src/ (top-level)      | 14    | 1,832      |
| src/unpack            | 7     | 1,424      |
| src/library-detection | 6     | 547        |
| src/profiling         | 5     | 482        |
| src/detection         | 10    | 424        |
| src/env-reads         | 2     | 341        |
| src/ui                | 1     | 330        |
| src/plugins           | 3     | 283        |
| src/shared            | 4     | 214        |
| src/test              | 1     | 193        |
| src/pipeline          | 3     | 113        |
| src/utils             | 2     | 57         |

**Porting hotspots — the twelve largest files.** Half the port's risk
concentrates here; each is a work package's anchor below.

| file                                | LOC   | lands in |
| ----------------------------------- | ----- | -------- |
| src/rename/processor.ts             | 3,224 | WP4.3    |
| src/rename/prior-transfer.ts        | 1,914 | WP3.2    |
| src/prior-version/prior-version.ts  | 1,860 | WP2.4    |
| src/rename/diff-reconcile.ts        | 1,813 | WP4.4    |
| src/split/cjs-emit.ts               | 1,678 | WP5.3    |
| src/rename/plugin.ts                | 1,677 | WP4.6    |
| src/split/stable-split.ts           | 1,673 | WP5.1    |
| src/commands/unified.ts             | 1,629 | WPB.4    |
| src/analysis/fingerprint-index.ts   | 1,463 | WP2.1    |
| src/prior-version/statement-twin.ts | 1,219 | WP2.3    |
| src/analysis/structural-hash.ts     | 1,103 | WP1.3    |
| src/analysis/function-graph.ts      | 985   | WP1.4    |

**Lane totals** (every file assigned exactly once; sums reconcile to 44,393):

| lane                                  | TS LOC in scope        |
| ------------------------------------- | ---------------------- |
| **TOTAL**                             | **44,393**             |
| Phase 1 — core model                  | 5,516                  |
| Phase 2 — matching cascade            | 5,786                  |
| Phase 3 — transfer + validated rename | 4,279                  |
| Phase 4 — LLM waves + naming passes   | 12,154                 |
| Phase 5a — placement, split, emit     | 9,627                  |
| Phase 5b — formatter swap             | 0 ported (156 retired) |
| Track B — shell (CLI, detect, unpack) | 6,355                  |
| Phase 0 — TS-side instruments         | 0 (new TS work)        |
| REMAINS not line-ported (table below) | 676                    |

Not line-ported, and why:

| file                                  | LOC     | disposition                                                                                   |
| ------------------------------------- | ------- | --------------------------------------------------------------------------------------------- |
| **REMAINS total**                     | **676** |                                                                                               |
| src/babel-utils.ts                    | 245     | replaced by oxc idioms inside WP1.2, not translated                                           |
| src/analysis/analysis-cache.ts        | 82      | designed out — arenas make the per-AST cache structural (`02-rust-target-architecture.md` §1) |
| src/plugins/babel/ (babel.ts + .d.ts) | 156     | retired at WP5.6 — the beautifier is the formatter being swapped                              |
| src/test/rename.e2etest.ts            | 193     | harness-side e2e; re-pointed at the binary in phase 6                                         |

Size classes used below, anchored to TS LOC ported: **S** ≤ 300, **M**
301–1,000, **L** 1,001–2,500, **XL** > 2,500. Packages that are new design
rather than translation (WP0.2, WP0.3, WP1.2, WP5.5, WP5.6) are classed by
judgment and marked (est). Rust LOC will differ from TS LOC; the class
measures surface, not effort.

## 2. Work packages by phase

Paths in the contents column are relative to `src/`. "Differ section clean"
means the parity differ (WP0.3) reports zero divergence for that artifact
class on all four oracle pairs; the exact invocation is doc 07's.

### Phase 0 — freeze the contract and the oracle (TS pipeline work plus the first Rust crate)

| id    | contents                                                                                                                                                                                                                                                                                                                                                                 | TS-side prereq | exit gate                                                                                                                                                                                                           | depends on | parallel with | size    |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- | ------------- | ------- |
| WP0.1 | pipeline contract doc: flags, exit codes, `ERROR:` stdout blocks, tree layout, stats/ledger/trail shapes — plus the harness-written records derived from those observables (`-run.json`, experiments/lib/run-manifest.ts; `-run-status.json`, experiments/lib/invariants.ts), documented as consumers, not binary outputs                                                | none           | reviewed against a real run's artifacts                                                                                                                                                                             | none       | WP0.2         | S       |
| WP0.2 | `--dump-artifacts <dir>` in TS: graph, three hash tables, match pairs + tier, resolutionStats, transfer ledger with per-attempt rejection reasons (strategy trails already record attempts — rename/strategy-trail.ts), prompt set, placement decisions + tier, emission order, unpack manifest + vendor-name table                                                      | none           | `npm run check` green; neutrality NEUTRAL for the flag (`npm run eval -- neutrality <ref>`, wraps `experiments/lib/neutrality.sh`, warm cache — baseline leg +0 writes)                                             | WP0.1      | WP0.3         | M (est) |
| WP0.3 | parity differ: the `humanify-parity` crate — deliberately the FIRST Rust code written, exercised against TS dumps before any pipeline Rust exists (doc 07 §4; compare/compare-ledger/selftest verbs). Reads two dump dirs, reports per-artifact-class divergence; sections = the WP0.2 list. Prereq: the minimal cargo workspace scaffold from `05-rust-toolchain.md` §1 | WP0.2 shapes   | differ run against TS-vs-TS dumps of the same commit reports zero (the fixture-positive check, measurement rule 3: also run it against two DIFFERENT commits and watch it report nonzero before believing any zero) | WP0.2      | WP0.4         | M (est) |
| WP0.4 | oracle freeze: dump all four eval pairs + fixture set at the oracle commit; capture warm LLM cache at the same commit                                                                                                                                                                                                                                                    | WP0.2          | dumps + cache stored and labeled with the oracle commit                                                                                                                                                             | WP0.2      | WP0.3         | S       |

### Phase 1 — core model: parse, semantic, graph, hashes (5,516 LOC)

| id    | contents                                                                                                                                                                                                                                                                                                                          | TS-side prereq                | exit gate                                                                                                                 | depends on | parallel with | size      |
| ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------- | ---------- | ------------- | --------- |
| WP1.1 | workspace scaffold (three crates per `02-rust-target-architecture.md` §2); env/kill-switch module: kill-switches.ts 191, env.ts, debug.ts 537, verbose.ts, cli.ts, index.ts, cli-error.ts, number-utils.ts, file-utils.ts, utils/ 57, shared/regex.ts, commands/default-args.ts 52 (cycle-breaker) → `humanify-cli` + `core::env` | none                          | `cargo build` + `cargo clippy` + `cargo test` green; kill-switch guard test ported (no `std::env` outside the env module) | WP0.4      | WPB.1         | M (989)   |
| WP1.2 | ingest: oxc parse + semantic over TS-beautified text; spans; replaces babel-utils.ts (not translated) → `core::ingest`                                                                                                                                                                                                            | oracle dumps exist            | parses all four oracle pairs' beautified text with zero errors; symbol/scope counts recorded                              | WP1.1      | —             | M (est)   |
| WP1.3 | hashes: analysis/structural-hash.ts 1,103, split/statement-hash.ts 114, analysis/enclosing-statement.ts 101 → `core::hash`, three newtypes + canonical serialization (`02` §4a)                                                                                                                                                   | WP0.2 dumps the hash tables   | differ: bucket partitions equal, twin sets equal (decision parity, not byte parity — `03-migration-plan.md` phase 1)      | WP1.2      | WP1.4, WP1.5  | L (1,318) |
| WP1.4 | graph + model types: analysis/function-graph.ts 985, function-fingerprint.ts 394, types.ts 726 → `core::graph`, `core::model`                                                                                                                                                                                                     | WP0.2 dumps graph edges       | differ: graph edges + scope parents equal on all four pairs                                                               | WP1.2      | WP1.3, WP1.5  | L (2,105) |
| WP1.5 | module classification: analysis/bun-module-classification.ts 626, known-globals.ts 269, wrapper-detection.ts 119, soundness.ts 90 → `core::modules`                                                                                                                                                                               | dumps cover module boundaries | differ: module boundary sets equal                                                                                        | WP1.4      | WP1.3         | L (1,104) |

Phase 1 exit = all differ sections above clean. **`core::model` types freeze
here; later packages extend additively** — the types module is the one shared
surface two agents would otherwise collide on.

### Phase 2 — the matching cascade (5,786 LOC) — gates the go/no-go

| id    | contents                                                                                                                                                      | TS-side prereq                 | exit gate                                                                                                                  | depends on   | parallel with       | size      |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ | -------------------------------------------------------------------------------------------------------------------------- | ------------ | ------------------- | --------- |
| WP2.1 | cascade + guards: analysis/fingerprint-index.ts 1,463 (matchFunctions at :773; singleton, contradiction, injectivity) → `core::matching::cascade`             | resolutionStats in dump        | differ: match sets pair-for-pair, tier-for-tier; resolutionStats identical                                                 | WP1.3, WP1.4 | WP2.2, WP2.3, WP3.1 | L (1,463) |
| WP2.2 | close-match: analysis/close-match.ts 258, prior-version/statement-align.ts 539 (alignment gate) → `core::matching::close`                                     | close-match candidates in dump | differ: candidate sets + corroboration verdicts identical                                                                  | WP2.1        | WP2.3               | M (797)   |
| WP2.3 | twins + role evidence: prior-version/statement-twin.ts 1,219 (pairing half; apply half gates in phase 3), binding-role.ts 239 → `core::matching::twins`       | twin sets in dump              | differ: twin pair sets identical                                                                                           | WP1.3        | WP2.1, WP2.2        | L (1,458) |
| WP2.4 | prior-version orchestration + evidence: prior-version/prior-version.ts 1,860, ambiguity-probe.ts 107, rename/lifecycle.ts 101 (cycle-breaker) → `core::prior` | full match dump                | **phase 2 gate: identical match sets + identical resolutionStats, all four pairs, exact** (`03-migration-plan.md` phase 2) | WP2.1–2.3    | Track B             | L (2,068) |

This is **M1, the decision point** (section 5). If this gate needs tolerance
instead of equality, stop — that is the plan's stated early falsifier
(`03-migration-plan.md`, "What would falsify the plan early").

### Phase 3 — mechanical transfer and validated rename (4,279 LOC)

| id    | contents                                                                                                                                                                                                                                                                       | TS-side prereq                        | exit gate                                                                                                                         | depends on   | parallel with                           | size      |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | ------------ | --------------------------------------- | --------- |
| WP3.1 | validated rename + trail runner: rename/validated-rename.ts 568 (the seven rules), rename-ledger.ts 158, strategy-trail.ts 185, rename-eligibility.ts 60, five remaining small rename modules 227 → `core::rename::validated` + the generic trail-owning tier runner (`02` §5) | capture fixtures shared from TS suite | ported capture fixtures green; symbol name field private to this module (compile-checked)                                         | WP1.2        | WP2.x (build early, gate after phase 2) | L (1,198) |
| WP3.2 | transfer tiers: rename/prior-transfer.ts 1,914, function-bindings.ts 228, carried-names.ts 59, skip-list.ts 92, split/prior-carry.ts 61 (cycle-breaker) → `core::rename::transfer`                                                                                             | rejection reasons in dump (WP0.2)     | **phase 3 gate: identical rename ledgers — every applied AND every rejected rename with reason** (`03-migration-plan.md` phase 3) | WP2.4, WP3.1 | WP4.1, WP4.2                            | L (2,354) |
| WP3.3 | votes + pins: analysis/propagation.ts 457, rename/single-vote-pin.ts 127, proximity.ts 143 → `core::rename::votes`                                                                                                                                                             | vote tallies in trail dump            | differ: vote tallies + ladder outcomes identical                                                                                  | WP3.1, WP2.1 | WP3.2                                   | M (727)   |

### Phase 4 — LLM waves and the naming passes (12,154 LOC — the largest phase)

| id    | contents                                                                                                                                                                                                                                                                 | TS-side prereq                    | exit gate                                                                                                                                        | depends on          | parallel with  | size       |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------- | -------------- | ---------- |
| WP4.1 | client + cache: llm/openai-compatible.ts 286, cached-provider.ts 162, rate-limiter.ts 231, metrics.ts 365, types.ts 149, debug-wrapper.ts 62 → `humanify-llm` (same disk cache format, `02` §7)                                                                          | warm cache captured (WP0.4)       | Rust client replays a TS-written cache entry byte-for-byte; tokio client sustains configured concurrency against a stub server                   | WP1.1               | phases 2–3     | L (1,255)  |
| WP4.2 | prompts + validation: llm/prompts.ts 507, validation.ts 218 (DECORATION_WORDS owner), rename/code-window.ts 239, context-builder.ts 206 → `core::naming::prompts`                                                                                                        | prompt set dump (WP0.2)           | prompt snapshot tests green per prompt type                                                                                                      | WP1.4               | WP3.x          | L (1,170)  |
| WP4.3 | waves + processor: rename/processor.ts 3,224, wave-scheduler.ts 325, wave-profile.ts 86, coverage.ts 386 → `core::naming::waves`                                                                                                                                         | warm cache + prompt dump          | **phase 4 gate step 1: prompt byte-parity vs the TS dump** (`03-migration-plan.md` phase 4)                                                      | WP3.2, WP4.1, WP4.2 | WP4.4/4.5 prep | XL (4,021) |
| WP4.4 | reconcile-prior-diff: rename/diff-reconcile.ts 1,813, reconcile-step.ts 131 → `core::naming::reconcile`                                                                                                                                                                  | trail dump covers reconcile tiers | differ: reconcile tier outcomes identical on warm replay                                                                                         | WP4.3               | WP4.5          | L (1,944)  |
| WP4.5 | floor/sweep/permute/census: rename/minted-census.ts 398, coverage-sweep.ts 281, sweep-step.ts 110, family-permute.ts 185, family-permute-step.ts 414, prior-name-snap.ts 104, class-id-floor.ts 114, decoration-retry.ts 79, diagnostics.ts 402 → `core::naming::passes` | trail dump covers each pass       | differ: per-pass trails identical on warm replay                                                                                                 | WP4.3               | WP4.4          | L (2,087)  |
| WP4.6 | naming-stage driver: rename/plugin.ts 1,677 (wires transfer → waves → passes) → `core::naming::driver`                                                                                                                                                                   | none                              | **phase 4 gate step 2: warm-replay name equivalence, zero cache writes on the Rust leg** (rule 10's permitted use; the write count IS the proof) | WP4.1–4.5           | —              | L (1,677)  |

### Phase 5 — placement, split, emit (9,627 LOC), then the formatter swap

| id    | contents                                                                                                                                                                                                              | TS-side prereq                     | exit gate                                                                                                                                                                                                                                                                                                           | depends on     | parallel with        | size       |
| ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- | -------------------- | ---------- |
| WP5.1 | placement: split/stable-split.ts 1,673 (PLACEMENT_TIERS at :765), placement-trail.ts 231, content-anchor.ts 224, layout.ts 119 → `core::place`                                                                        | placement decisions + tier in dump | differ: placement decisions + tiers identical                                                                                                                                                                                                                                                                       | WP1.3          | WP5.2 prep           | L (2,247)  |
| WP5.2 | grouping/assignment: split/cluster-assign.ts 981, fossil-assign.ts 881, fossil-match.ts 653, fossil-map.ts 324, split-namer.ts 203 (folder naming, tiny LLM) → `core::place::assign`                                  | dump covers assignments            | differ: file assignments identical on warm replay                                                                                                                                                                                                                                                                   | WP5.1          | WP5.3 prep           | XL (3,042) |
| WP5.3 | order + emit: split/load-order.ts 670, cjs-emit.ts 1,678, emitter.ts 55, substitutions.ts 50, shared/cjs-factory.ts 67, bun-helpers.ts 111, unique-name.ts 27 → `core::emit` (slices rendered text by spans, `02` §3) | emission order in dump             | differ: emission order identical; then feeds the 5a gate                                                                                                                                                                                                                                                            | WP5.1, WP3.1   | WP5.4 prep           | XL (2,658) |
| WP5.4 | scaffold/relink/vendor/carry: split/runnable-scaffold.ts 317, bun-relink.ts 277, vendor-body-inherit.ts 144, using-desugar.ts 96, post-split-reconcile.ts 502, bundle-carry.ts 344 → `core::finish`                   | none                               | **phase 5a gate: byte-identical emitted trees vs TS on warm replay** (`03-migration-plan.md` 5a); boot gate passes                                                                                                                                                                                                  | WP5.3, Track B | —                    | L (1,680)  |
| WP5.5 | provenance sidecar (`02` §4b) — new, no TS counterpart → `core::sidecar`                                                                                                                                              | none                               | sidecar joins spans to trails on one pair; `explain` verb prints both sides of a diff hunk                                                                                                                                                                                                                          | WP5.3          | WP5.4                | M (est)    |
| WP5.6 | formatter swap: oxc codegen + normalization pass replaces plugins/babel/ (156 LOC retired); beautify becomes native                                                                                                   | none                               | **phase 5b: THE single noisy event** — prior bases regenerated (the default for `npm run eval -- score`), self-hop = 0, boot gates x4, concat-equivalence, mints ≈ 0, KPIs vs the freshly rebased reference with `novel`/`realLn` unmoved, judged against `experiments/034-eval-harness/noise-bands.json` (rule 11) | WP5.4          | nothing — runs alone | M (est)    |

### Track B — the shell (6,355 LOC; parallel with phases 2–4, merges before the 5a gate)

| id    | contents                                                                                                                                                                                                                                                  | TS-side prereq                         | exit gate                                                                                                                                                           | depends on | parallel with | size       |
| ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- | ------------- | ---------- |
| WPB.1 | detection: detection/ 424 (detect.ts:35, signals/) → `cli::detect`                                                                                                                                                                                        | none                                   | detection verdicts equal TS on the fixture set                                                                                                                      | WP1.1      | WP2.x–4.x     | M (424)    |
| WPB.2 | unpack: unpack/ 1,424 (adapters/bun.ts 827 native; webcrack via subprocess shim absorbing plugins/webcrack.ts 127; vendor-namer.ts, manifest-order.ts) → `cli::unpack`                                                                                    | unpack manifest + vendor table in dump | differ: unpack manifest + vendor-name carry identical (fresh vendor names gate at warm replay)                                                                      | WPB.1      | WP2.x–4.x     | L (1,551)  |
| WPB.3 | library detection: library-detection/ 547 (index.ts:14 registry, comment-regions, banner-patterns, adapters/) → `cli::libdetect`                                                                                                                          | none                                   | detector selections + regions equal TS on the fixture set                                                                                                           | WPB.2      | WP2.x–4.x     | M (547)    |
| WPB.4 | pipeline driver + CLI: commands/unified.ts 1,629, settings.ts 162, commands/env-reads.ts 60, env-reads/ 341, pipeline/ 113, ui/progress.ts 330, output-validation.ts 401, failed-output.ts 101, stage-fingerprint.ts 27, unminify.ts 187 → `humanify-cli` | contract doc (WP0.1)                   | binary honors the pipeline contract: flags, exit codes, `ERROR:` stdout blocks (what the harness's `-run-status.json` extraction parses), and `--stats-json` shapes | WP1.1      | WP2.x–4.x     | XL (3,351) |
| WPB.5 | profiling spans: profiling/ 482 → serde structs, same JSON shapes (`02` §3)                                                                                                                                                                               | none                                   | `--profile` trace opens in a trace viewer on one pair                                                                                                               | WP1.1      | everything    | M (482)    |

### Phase 6 — cutover and deletion

No new port surface. The checklist is section 5's M5.

## 3. Sequencing: the spine, the cycle-breakers, the two-agent rule

**The dependency spine** (from the import census; each arrow is a hard
ordering for gates, not necessarily for writing code):

```
hash (WP1.3) → graph (WP1.4) → fingerprint index/cascade (WP2.1)
  → prior-version evidence (WP2.4) → transfer (WP3.2) → LLM waves (WP4.3)
  → naming passes (WP4.4/4.5) → placement (WP5.1/5.2) → emit (WP5.3)
  → post-split reconcile + carry (WP5.4)
```

**Three TS dir-level cycles dissolve by porting the small shared module
early**, so the Rust graph is acyclic from day one:

| TS cycle                | shared module (ports early, becomes a leaf) | lands in |
| ----------------------- | ------------------------------------------- | -------- |
| rename ↔ prior-version | src/rename/lifecycle.ts (101)               | WP2.4    |
| rename ↔ split         | src/split/prior-carry.ts (61)               | WP3.2    |
| rename ↔ commands      | src/commands/default-args.ts (52)           | WP1.1    |

**Concurrency for two agents.** Safe pairings, by construction (disjoint crate
modules, no shared gate ordering):

- WP1.3 (hash) with WP1.4/WP1.5 (graph/modules), once WP1.2 lands.
- All of Track B with anything in phases 2–4 — the shell never touches
  `core::matching`/`core::naming`.
- WP3.1 (validated rename) builds during phase 2; its gate waits for phase 2
  outputs but its code depends only on WP1.2 scoping.
- WP4.1/WP4.2 (client, prompts) with phase 3 packages.
- WP4.4 with WP4.5 (disjoint passes; gate together at warm replay).
- WP5.1/5.2/5.3 build in parallel; the 5a gate serializes them.

Hard rules: one work package has one owner at a time (the ledger's
in-progress status is the lock); `core::model` types freeze at phase-1 exit
and change only additively; WP5.6 runs alone — nothing else lands while the
one noisy event is being judged.

## 4. The porting ledger: PORTING.md

Create `PORTING.md` at the repo root when phase 0 starts. **It is the single
place progress is visible** — commit messages, chat, and other docs must not
claim porting status independently (rule 9: claims age in older documents;
this file is the one that does not).

Header: the oracle label + oracle commit currently in force (per doc 07's
lifecycle), and the progress line — **TOTAL 44,393 LOC / parity-green N LOC
(x%)**, with the REMAINING count as the final line of the file.

One row per `src/` non-test file (143 rows, seeded from the section-1 census),
sorted by phase then LOC descending:

| column      | content                                                                                                      |
| ----------- | ------------------------------------------------------------------------------------------------------------ |
| TS file     | path under src/                                                                                              |
| LOC         | from the census                                                                                              |
| WP          | work package id from section 2                                                                               |
| Rust module | target `crate::module` path                                                                                  |
| status      | `not-started` / `in-progress` / `parity-green` / `replaced` / `designed-out` / `subprocess` / `harness-side` |
| gate run    | REQUIRED for every status change: the command run + oracle label + date (or a results path)                  |

Rules:

- **No status change without a gate-run citation.** "It compiles" is not a
  status; the differ section (or phase gate) naming that file's artifacts is.
- A file whose behavior spans two phases (statement-twin.ts: pairing gates in
  phase 2, apply in phase 3) goes `parity-green` only when its LAST gate
  passes; until then `in-progress`, with both gates cited on completion.
- A `parity-green` row citing a superseded oracle label is visibly stale —
  re-run the phase gate against the new dumps and update the citation
  (section 6).
- **TS deletion happens only at phase 6, regardless of per-file status.** A
  parity-green file's TS original stays in place: TS remains the oracle
  producer and the production pipeline until cutover.

**Doc upkeep rides on the same rule.** The `docs/rust-port/` set is subject
to measurement-pitfalls rule 9 like everything else: when a probe or bench
overturns a number a doc states (04's projections are the expected case —
`11-risk-register.md` R12), the owning doc is edited IN PLACE with a dated
note the day the measurement lands, not at phase 6. M5's "STATUS blocks on
this doc set" item is the final audit that this happened, not the mechanism.
PORTING.md's gate-citation rule is the model: a claim without a dated source
is not a status.

## 4b. Picking up work cold (agent onboarding)

A fresh session resuming port work — the two-lane rule in section 3 assumes
several will — starts here, in order:

1. **PORTING.md header**: the oracle label + commit in force, and the
   progress line. Everything else is judged against that label.
2. **Claim a package**: pick an unclaimed WP whose depends-on rows are
   parity-green; set its rows `in-progress` with the date and session in the
   gate-run column BEFORE starting (the claim protocol — two agents on one
   WP is the collision the ledger exists to prevent). Lanes that may proceed
   concurrently are exactly section 3's; anything else waits.
3. **Read for the package**: the WP row names its exit gate; the gate names
   its doc — differ gates are `07-differential-validation.md`, stage wiring
   `05-rust-toolchain.md` §8, test layers `06-testing-strategy.md`. Read the
   TS file(s) in the contents column and their colocated `*.test.ts` last —
   the tests are the behavior spec.
4. **Run the gate red first**, then port until it is green, then update the
   row with the citation. The commands per phase are section 2's exit-gate
   column; nothing outside those commands counts as evidence.

## 5. Go/no-go milestones

| id  | milestone                                         | entry criteria                                              | exit criteria                                                                                                                                                                                                                                                                                                                                                                                    |
| --- | ------------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| M0  | oracle frozen                                     | WP0.1–0.2 merged, `npm run check` green                     | neutrality NEUTRAL for the dump flag (baseline leg +0 cache writes); dumps + warm cache stored under an oracle label                                                                                                                                                                                                                                                                             |
| M1  | **phase 2 exact-match gate — THE decision point** | phase 1 differ sections clean (partitions, twins, edges)    | identical match sets pair-for-pair, tier-for-tier, and identical resolutionStats on all four oracle pairs — **exact, no tolerance**. Phases 0–2 are 11,302 of 44,393 LOC (25.5% of the surface, computed from section 1) — the cheap fraction that proves or falsifies the approach (`README.md`, "The decision this asks for"). Tolerance needed = stop and understand (`03-migration-plan.md`) |
| M2  | warm-replay name equivalence                      | phase 3 ledger gate passed; prompt byte-parity (WP4.3) held | every name assignment equals the TS warm run; **zero cache writes on the Rust leg** — the rule-10 proof obligation                                                                                                                                                                                                                                                                               |
| M3  | byte-identical trees (5a)                         | M2; Track B merged (end-to-end binary exists)               | emitted trees byte-identical vs TS on warm replay, all four pairs; boot gates pass                                                                                                                                                                                                                                                                                                               |
| M4  | formatter swap (5b) — **the single noisy event**  | M3; nothing else in flight                                  | identical to WP5.6's exit gate (§2 phase 5), judged against the re-measured noise bands                                                                                                                                                                                                                                                                                                          |
| M5  | phase 6 definition of done                        | M4                                                          | checklist below, every box                                                                                                                                                                                                                                                                                                                                                                       |

**M5 checklist:**

- [ ] Every gate green at the cutover commit: parity differ clean, warm-replay
      equivalence, 5a byte identity (historical, pre-5b), 5b gate suite.
- [ ] One 20-hop walk segment run on the binary and compared against the TS
      reference era: hop times, KPI bands, boot gates (`03-migration-plan.md`
      phase 6) — KPI comparison judged against
      `experiments/034-eval-harness/noise-bands.json`, not raw deltas.
- [ ] `experiments/034-eval-harness/run.sh`, the walk driver, and
      `npm run eval` point at the binary (one variable each).
- [ ] `scripts/check.ts` STAGES swapped: `cargo fmt --check`, `cargo clippy`,
      `cargo test`, `cargo build` enter as stages; `typecheck`/`lint`/`unit`
      retarget the surviving TS (scripts/, experiments/, test/) — one entry
      each in the STAGES array (scripts/check.ts:56-98), which remains the
      only list that says what the gate covers.
- [ ] Docs updated: `docs/pipeline-stages.md`, `docs/responsibility.md`
      (module ownership moves to crate paths), CLAUDE.md commands, STATUS
      blocks on this doc set (rule 9).
- [ ] `src/` deleted. `experiments/`, `scripts/`, `test/e2e` stay (harness-
      side). PORTING.md's final state committed as the record.

## 6. What continues on main during the port

Levers keep landing; the port must not freeze the product. The freeze applies
to the **oracle commit only** (`03-migration-plan.md`, sequencing realities).

When a merged lever changes decisions — the observable trigger is the eval
reference being re-scored and re-pointed, the existing practice for "current
main" labels — re-dump rather than argue with a stale oracle:

1. Pick the new oracle commit (normally the re-scored reference commit).
2. Re-run `--dump-artifacts` over the four pairs + fixtures at that commit;
   capture a warm cache at the same commit (WP0.4's procedure, doc 07's
   lifecycle).
3. New oracle label in PORTING.md's header; every `parity-green` row now
   cites a superseded label and is visibly stale.
4. Re-run the phase gates already passed, against the new dumps; update
   citations.

**Cost: a re-run, not a re-design.** Four cold pipeline runs at 9–17 min each
(measured, `04-performance-model.md`) plus differ re-runs that take seconds
(`03-migration-plan.md`, sequencing realities) — roughly an afternoon
(estimate). Do this per decision-changing merge burst, not per commit. Two
standing cautions carry over: the walk runs main's tree per hop, so no
edits to `src/` mid-run, and the neutrality baseline leg runs in a detached
worktree while the candidate leg runs from the live tree — same rule: no
edits to `src/` while a leg is running.

## 7. Calendar honesty

Magnitude bands for a single maintainer working with Claude agents. **All
estimates.** The dominant unknowns: agent-assisted Rust throughput on this
codebase, and time-per-parity-divergence once the differ starts reporting
real ones. Phases 0–2 exist to measure exactly that — **re-forecast at M1
with observed velocity**; the bands below are priors, not commitments.

| phase     | band                                                                                                     |
| --------- | -------------------------------------------------------------------------------------------------------- |
| **TOTAL** | **3–6 months** — consistent with `03-migration-plan.md`'s "months-scale"                                 |
| Phase 0   | days (serializing trails that already exist; neutrality proof ~25 min/pair, measured)                    |
| Phase 1   | 1–3 weeks (includes the oxc learning curve and the canonical-serialization design)                       |
| Phase 2   | 2–4 weeks; **M1 lands roughly one to two months in**                                                     |
| Phase 3   | 2–3 weeks                                                                                                |
| Phase 4   | 3–5 weeks (largest surface; processor.ts is the biggest single file)                                     |
| Phase 5a  | 3–5 weeks (Track B overlaps phases 2–4 in a second lane)                                                 |
| Phase 5b  | days to a week (one event; an eval sweep is ~81 min measured, so several gate iterations fit per day)    |
| Phase 6   | days (a 20-hop segment at today's 7.8 min median is ~2.6 h; at projected post-port speed, under an hour) |

What compresses it: the phase-1 bench and phase-2 gate teach the real
velocity (everything after M1 is re-plannable on data); two agents on the
disjoint lanes in section 3; and differ quality — the parity loop is
seconds per iteration when the differ localizes a divergence well, and
hours when it does not, so WP0.3 is leverage, not overhead.

## Open questions

- Doc 07's gate-runner naming: this doc refers to "differ sections"; align
  the section names with 07's gate ids once both are merged, in one pass.
- WP0.3's differ is the `humanify-parity` crate per doc 07 §4; the open
  question is only its check-stage wiring (which STAGES entry invokes it and
  on which fixture set) — decide at WP0.2 review.
- statement-twin.ts is one file with gates in two phases; if the split of
  pairing vs apply proves awkward in Rust module terms, re-cut WP2.3/WP3.2
  boundaries at phase-2 exit and record the re-cut in PORTING.md.
- Whether the fresh-grouping path (cluster-assign.ts, no-prior case) can gate
  on the oracle pairs at all — all four pairs have priors. It may need a
  fifth oracle input (a no-prior run of one version) dumped at WP0.4; decide
  before phase 5a starts.
- The walk driver script named at M5 lives outside this repo (the walk
  operates in the unpacked-claude-code checkout); confirm its variable name
  for the binary path when phase 6 opens.
