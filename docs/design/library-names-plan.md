# Plan: library names from a call-aware, run-time-built index (Bun first)

STATUS: PLAN, 2026-10-25. Agreed with Andrew. Nothing here is built yet.

## Totals first

- **Goal.** `--library-matching names` puts code the index recognises under its real package path
  (`vendor/<package>/<file>`) and keeps the model off it. It also restores the package's own identifier names
  where the match carries them.
- **The lever.** Library matching stopped at loose unit hashes and never adopted the cross-version matcher's main
  evidence, call context (callee hashes, call shapes, two-hop shapes). Every guard tuned in exp097-exp149 works
  around that weak signal. The index gains an edge table, and the library profile gains a call-context rule that
  reuses the cascade's pieces.
- **The index is built for the run, and cached.** A run detects the bundler and its version, finds or builds the
  index it needs, and does this in parallel with the stages that don't need it. It also works stand-alone
  (`humanify-index build`, `--library-index <dir|id>`).
- **Profiling covers it.** The index build appears in the run's profile as its own stages, so a full run shows how
  long the index took and why.
- **Scope.** Bun only, tested on Bun-built apps with source maps and on Claude Code. Other bundlers follow the same
  path later; the evidence is meant to be bundler-agnostic, because the index is built with the target's own
  bundler, version and flags, so it reproduces the target's inlining and rewrites.

## Decisions (Andrew, 2026-10-25)

| question              | decision                                                                                         |
| --------------------- | ------------------------------------------------------------------------------------------------ |
| what `names` does     | real `vendor/<package>/<file>` placement and the model kept off it (identifier restore is later) |
| install scripts       | always skipped (`--ignore-scripts`, already the builder's rule)                                  |
| Bun version unknown   | guess it (a compiled executable records it; a JS bundle is guessed from Bun's runtime helpers)   |
| index not ready       | the run waits; the wait is a profiled span                                                       |
| where the cache lives | a folder relative to the working directory by default, overridable, found automatically          |
| refresh               | each index records how it was built (times, tool versions, as-of) so it can be refreshed         |
| bundlers              | Bun first                                                                                        |
| profiling             | how long the index took and why, as part of the total run time                                   |

## Steps

Each step lands on its own `expNNN-*` branch with a gate table and a results table. Steps 1-3 change no default.

### 0. Test bed (Bun)

DONE (exp150, `experiments/150-bun-testbed/README.md`): `--corpus bun-dev` (45 apps, 90 bundles), `--corpus bun-held`
(15 apps, 30 bundles, private) and `--corpus cc` (the seed-95 sample redrawn exactly and re-labelled, blind pass
124/124), with main's baseline on each.

The 2026-10-25 cleanup deleted the scoring corpora and Claude Code's ground truth (exp095's 620-statement labelled
sample of 2.1.216). Rebuild only what Bun needs:

- **Bun apps with truth.**
  - Build about 60 open-source apps with Bun 1.4.0 (`--target node` and `--target bun`, minified) and
    `--sourcemap=external`. The source map is the truth.
  - Use the committed recipe: `experiments/108-app-corpus/scripts/select_apps.py` and the pop corpus's build
    recipe.
  - Split it into dev and held-out sets. The held-out set is scored once per candidate, summaries only.
- **Claude Code truth.**
  - Redraw the same sample (`experiments/095-library-fingerprints/negatives/scripts/sample.py`, seed 95).
  - Re-label it, with a blind second pass on 20%, as the original lane did.
  - Labels quote Claude Code, so they live outside the repo (`/work/cc-library-truth/`).
  - Only rates and package names are committed.
- **Baseline.** Score today's matcher (the installed index) on both. Every later step is judged against this.

### 1. Edges in the index (format `/4`)

- For every unit row, the builder also records the prints of the units it calls inside its own package (INSIDE
  edges), from the same reference graph the pipeline builds (`graph/edges.rs`).
- Edges into the package from outside are never recorded. That is the graph scope rule:
  `docs/matching-architecture.md`.
- The index store answers `ReferenceStore::edges` for INSIDE and OUT-OF, which today returns `NotRecorded`
  (`matching/store.rs`).
- New lookup table `edges.<slice>.bin`; a new fingerprint stamp, so shards rebuild.
- Size is measured before committing to it.
- Test first: a fixture library whose two functions have identical bodies but different callees must be told
  apart.

### 2. Call-context rule in the library profile

- **What it does.** A unit match is supported when its in-library calls line up: the bundle's unit calls units
  that the index says the matched unit calls, in the same package family.
- **Reuse.** It reuses the cascade's callee-hash and call-shape code (`matching/cascade.rs`, `CalleeHashes`,
  `TwoHopShapes`), relaxed where the library problem allows:
  - containment instead of equality, because tree-shaking drops callers but never a kept function's callees;
  - many programs per store.
- **Experiment order.**
  1. Census first: how many of today's ties, isolation drops and below-floor units call-context resolves right or
     wrong.
  2. Simulate.
  3. Build it as a default-off key.
- **Then retire guards.** Each guard it makes redundant is removed with its own measurement: the isolation veto,
  the position-window support, the tie fences and copy presence.
- **Call-tree hash.** A whole call-tree hash (a function plus everything it calls, computed once bottom-up) is
  measured in the same census. Minifier inlining is the risk.

### 3. The builder as a library, a cache, and run-time builds

> **STATUS: BUILT (exp154, 2026-10-26), default off.** The cache layout, the
> store finder (`search_store`), the in-process run-time build
> (`build_runtime`, called from the CLI's `runtime_index` on a scope thread
> joined under one deadline — span `library.index.wait`), the `build.json`
> sidecar (`library-index-build/1`) and the profile spans are on branch
> `exp154-run-time-index`. Keys: `index_store_dir` (`--library-index-store`,
> default `.humanify/library-index`), `index_build` ("off"|"auto", no flag),
> `index_build_max_packages` (300, refused at the parse point when exceeded).
> An explicit `--library-index` still wins and the machinery stays off; a
> default run never reads the store and never spawns a build process. **Two
> pieces of this section are deliberately NOT built**: the shared base list
> (the package list is the bundle's own evidence + its dependency closure
> only — no bootstrap), and downloading Bun (the pinned Bun must already be
> in `<store>/tools/`; the JS-bundle Bun-version guess is out of scope — the
> detected version is recorded `"unknown"` and the newest cached Bun is
> picked, with the choice in the report's `indexSource` trail).

- **Cache layout.**
  - Default: `.humanify/library-index/` under the working directory. A run finds it there by itself;
    `--library-index-store <dir>` overrides it.
  - `store/` holds shards. One per `pkg@version` per recipe and Bun version, as today. This is what makes reuse
    across runs and app versions automatic.
  - `<index id>/` holds merged indexes.
  - Each index gets a `build.json` sidecar: wall times per step, tool versions, as-of, package counts, failures.
    It is a sidecar because the index id is the manifest's hash, and times in the manifest would give identical
    indexes different ids.
- **Package list for a run-time build.**
  - The bundle's own evidence: banners, inlined `package.json`, version constants and error prefixes.
  - Their dependency closures.
  - Plus a shared base list, built once per Bun version and reused.
  - What is built and why is recorded in `build.json`.
- **Concurrency.** The build runs while unpack and parse run. Matching waits on it before naming starts. Install
  and bundle child processes follow `docs/concurrency.md`: drained pipes, a deadline over every wait, sites on the
  concurrency allowlist.
- **Profiling.** The builder takes the run's `Profiler` (`crates/humanify-core/src/profiling.rs`). Each of the
  following is a span, with child-process wall time included:

  - cache check;
  - registry fetch;
  - plan;
  - install;
  - bundle;
  - fingerprint;
  - merge;
  - the run's wait.

  The profile summary reports how many versions were built, how many reused, and how many failed and why.

- **Bun version.** A compiled executable records it (`Bun v1.x.y` in the binary). A JS bundle is guessed from
  Bun's runtime helper shapes, and the guess is recorded in diagnostics. Unsure means the newest Bun the cache
  holds.

### 4. `names` mode

- **What it does.** Place matched code under `vendor/<package>/<file>`, keep the model off it, and keep it out of
  the cross-version diff's app churn.
- **Gates.**
  - Library precision on the Bun test bed and Claude Code at least at today's bar: P 99.8%, app claimed 0.
  - The 034 eval: `novel` / `realLn` unchanged, vendor noise down.
- Andrew decides the flip.

### 5. Placement stability (exp157)

- **The question.** `names` mode placed a library differently depending on whether the run built its tree
  from scratch or inherited a prior tree — the same module content landed at different `vendor/` paths,
  which the cross-version diff books as whole-file remove + add.
- **The diagnosis.** The planner's `unclaimed` verdict (no factory claim inside the region's span — the
  bundler scattered the package between versions, exp047's mechanism) fell back to hash names that are
  derived from the bundle's COMPOSITION: a per-package ordinal rank of the regions present, so adding or
  removing one region elsewhere in the package renumbered the survivors. Shape itself was proven innocent:
  a stub run of the same bundle with and without `--prior-version` placed the naming era identically
  (15 files / 39 statements, byte-identical); every divergence traced to composition or to the naming era's
  strict statement-set join (a fossil joins a region only on exact statement-set equality — content drift
  flips the join wholesale, so the region loses its index answer and falls back).
- **The fix** (both in `crates/humanify-core/src/place/library_layout.rs`):
  1. an `unclaimed` region with a CLEAN index answer (one owner) now moves on the INDEX's evidence instead
     of leaving; contested / partial / multi-owner / no-row regions still leave unchanged;
  2. synthetic file names come from the region's own content (`module-<first 8 of the module head
statement's rename-invariant hash>`), not from the bundle's composition; arrival order only breaks a
     tie (`-2`, `-3`), so adding a region elsewhere in the package can no longer rename the survivors.
- **What it bought.** On the quiet pair (2.1.85→86, names mode): `vendorNoise` 3,192 → 136 lines,
  `vendorChurn` 4,732 → 1,633, bodies-name-only 3,132 → 100, `movedPath` 10 → 0, identical bytes 1,784 →
  9,029; 86% of vendor bytes sit at index paths on both sides (was 52%). Across all four pairs (names
  mode, vs the exp156 baseline): `vendorNoise` 4,766 → 1,491, `vendorChurn` 8,600 → 5,184, `vendorReal`
  3,834 → 3,693, `novel` / `realLn` byte-equal per pair and total (4,188 / 416,377). Warm self-hop
  byte-identical with 0 cache writes.

  Per pair, names mode (exp156 baseline → exp157; vR = vendorReal, vN = vendorNoise, vC = vendorChurn,
  mints = mintedLeftovers):

  | pair    | vR          | vN          | vC          | bodies-name-only | mints     |
  | ------- | ----------- | ----------- | ----------- | ---------------- | --------- |
  | 85→86   | 1540 → 1497 | 3192 → 136  | 4732 → 1633 | 3132 → 100       | 668 → 655 |
  | 118→119 | 865 → 835   | 10 → 10     | 875 → 845   | 0 → 0            | 209 → 209 |
  | 197→198 | 865 → 862   | 1367 → 1148 | 2232 → 2010 | 4 → 0            | 211 → 242 |
  | 215→216 | 564 → 499   | 197 → 197   | 761 → 696   | 14 → 14          | 666 → 663 |

  In DEFAULT mode (vs the 2026-10-03 reference `ref-scratch-0f338ffa-r1`; conflates the merged exp150–155
  arcs since the fix is byte-inert there): vendorReal 1302 → 454, vendorNoise 3524 → 1199, mints 341 → 241,
  `novel` / `realLn` byte-equal (4,188 / 416,377).

- **The residual is not placement.** The remaining `vendorReal` (1,497 on the quiet pair) decomposes as:
  1,298 lines in 13 `trulyAdded` files — the naming era's strict statement-set join flipping between
  versions (a region emitted as a module on one side, fossil-bound on the other); 622 lines of rename
  ripple in `realChange` files — minted names re-rolled between the two runs and appear in string-literal
  and require-path positions (the scorer's signature preserves literals), plus app-side emitted-path
  changes that vendor files `require()`. Both are naming-era phenomena, not path choice.
- **Gates.**
  - `npm run check`: **ALL 13 STAGES PASSED**, exit 0.
  - Default-mode neutrality (branch parent `1840f83d6`, pair 85→86, warm standing cache,
    `--endpoint http://127.0.0.1:9/v1`): **NEUTRAL** — 0 differing files / 0 differing lines, cache
    writes +0/+0, exit 0/0. The fix is byte-inert with `names` off.
  - Names-mode neutrality (same pair and cache, the four `names` settings on, live endpoint): valid run —
    the baseline leg wrote **0** cache entries (it replayed the candidate's 1,027 answers exactly) and
    exits match (0/0) — and **NOT NEUTRAL by design**: 3,283 files / 23,613 lines, all of it the intended
    mechanism (vendor paths moved onto index evidence and content-derived names, plus the `require` lines
    and call sites that point at them; the `src/` half is require-literal + import-call updates only).
  - Determinism: every pair exits 0 with cache +0; warm self-hop byte-identical, 0 cache writes. The COLD
    self-hop on 2.1.216 reads 1,264 lines — outside the 2–184 reference range, same as the exp156 baseline
    (1,294): that range is measured with naming locked, and `names` mode re-rolls vendor and app names
    freely, so it is the regime, not this change.
  - Library scorer (exp096, structural rule, the edges/2 index, the four `names` settings): byte-identical
    to the exp156 baseline on all three corpora — cc P 1.0 / R 0.819 / F0.5 0.958, app claimed 0, wrong
    family 0; bun-dev P 99.8% / wrong 149.9 KB; bun-held P 100% / wrong 5.6 KB. Placed share on the quiet
    pair: 86% of vendor bytes at index paths on BOTH sides (1,220 index / 358 fallback / 11 declared /
    3 asset factories per tree).

### 6. The era's synthetic names and its join (exp158)

Two mechanical blockers on the way to identifier restoration (step 7): the era's synthetic names
renamed on composition changes, and the strict region-to-module join was suspected of flipping
wholesale on content drift. Branch `exp158-naming-stability`.

- **Fix 1 — the synthetic key is the module's whole statement set.** The era's synthetic
  `module-<hash8>.js` names keyed the module's HEAD statement only; short generic wrappers (the same
  blurred `var m = W(() => …)` shape) tie across different modules, and the tie fell to arrival
  order — so a version that discovered one more region renamed every survivor (the eval pair's
  `trulyAdded` bucket held the `lodash/module-df83ae16{,-2,-3}` and `lodash-es/module-df83ae16{,-2}`
  tie family and the `2374bc3b` collision shared by `@azure/core-tracing` and `@smithy/types`). The
  era's key (`module_content_key`, `naming/module_names.rs`) is now the module's per-statement
  rename-invariant hashes SORTED and hashed together — order-independent, content-derived — with
  arrival order only breaking an exact content tie (`module-<key8>-2.js`). The unpack side
  (`place_factories_from_index`) is untouched: its per-factory text keys were never the disease.
  Owner note: the task pointed at `place/vendor_names.rs`, which does not exist — the measured
  disease is the era's key in `naming/module_names.rs`; `modules/vendor_names.rs`'s
  `FileNameChooser` `lib_<hash8>-N` uniquifier is default-reachable but ties only byte-identical
  bodies (no measured churn on the pair) and was left alone.
- **Fix 2 — the drift-tolerant join relaxation is REFUTED by measurement.** The strict join (a
  region joins the module whose statement set is exactly its own) was suspected of flipping
  wholesale on content drift. The statement-level census (exp157's join census) put the two-sided
  overlap bar at 0.55 — the lowest symmetric bar with zero conflicting candidates. The OUTCOME
  census says otherwise: dumping the join's inputs from the binary on the 2.1.85→2.1.86 pair and
  replaying the join offline (the replay reproduces the strict join counts, 75/52, AND the pair's
  bucket counts, trulyRemoved 34 / trulyAdded 13, exactly), every bar from 0.5 to 0.95 in all three
  forms — two-sided, region-side only, module-side only — INCREASED the era's one-sided placed
  paths (strict 34/13 → 0.55 two-sided 99/59 → 0.55 region-side 156/146); the near-miss
  relationships themselves do not replicate across versions, because the claims flip packages
  wholesale (matching's territory). The relaxation shipped nowhere; the strict join stays, with
  exp158's PACKAGE GUARD (a module two packages claim never moves, whichever pass — a safety
  property; on the pair it refused 0 exact and 4–5 fuzzy candidates).
- **Reachability.** Both fixes are names-mode-only: `humanify-cli/src/config.rs` sets
  `module_answers`/`placement_groups` only when `matching == names`, `place_names_modules`
  early-returns without them, and `plan_library_modules` returns immediately on empty regions. No
  default flip; `library.names_unlocked` stays measurement-only.
- **Gates.** On the quiet pair's stub trees (the tree-compare buckets, the instrument the join
  disease was measured with): the final design reproduces exp157's placement exactly — trulyAdded
  13 / trulyRemoved 34 / movedPath 0 / nameOnly 1,588 / realChange 3, before AND after (the stub
  pair's `nameOnly` mass is the stub namer's raw-spelling drift, not placement); re-placing the
  era's synthetic files costs 35 `movedPath` on 2.1.85 and 15 on 2.1.86 and nothing else
  (identical 1,589, nameOnly 0, realChange 0). The rejected 0.55 relaxation, measured on the same
  instrument: trulyAdded 13 → 59, trulyRemoved 34 → 99.
  034 eval (names mode, one cold run, scratch base, vs the names-mode label `exp157-vendstab-names`;
  the default-mode reference `ref-scratch-0f338ffa-r1` is a different regime — names mode charges
  vendor content change the default regime leaves in `src/`, so every names-mode bar is exp157's):
  novel 4,188 / realLn 416,377 **byte-equal** (zero band); vendorReal 3,693 → 3,699 (~0±32);
  vendorLn 1,491 **=**; mints 1,769 → 1,754; treeLn 150,551 → 149,557 (−994, the quiet pair −729);
  cleanReal 85,671 → 84,924 (−747 — path churn leaving the real bucket with `fileAddRemove` −489;
  `realLn` byte-equal says no real change moved with it). The eval pair's buckets: identical 1,585 /
  nameOnly 19 / movedPath 0 / realChange 19 / trulyAdded 13 / trulyRemoved 2 — the counts equal
  exp157's, but the trulyAdded LIST changed: the old key's tie family (`lodash[/df83ae16×3]`,
  `lodash-es/b28cc807`) and the `2374bc3b` two-package collision are gone; what remains is two
  cross-package claim flips (`lodash-es/module-0581b523{.js}` → `lodash/…` and same for
  `5b5305a7` — matching's territory) and content one-sided on one version. Every pair exits 0 with
  cache +0; warm self-hop byte-identical, 0 cache writes; the COLD self-hop on 2.1.216 reads
  1,288 lines — outside the naming-locked reference range, the same names-mode regime as the
  exp156 (1,294) and exp157 (1,264) baselines.
  Library scorer (structural rule, the edges/2 index, `matching=names` + `names_unlocked`):
  byte-identical to the exp157 baselines on all three corpora — cc P 1.0 / R 0.819 / F0.5 0.958,
  app claimed 0; bun-dev per-bundle lines byte-identical (90 bundles, app 0); bun-held 30 bundles,
  verdict block identical (app claimed 0).
  Default-mode neutrality (vs main `6c8cde934`, pair 85→86, warm standing cache
  `/work/neutrality-cache`): **NEUTRAL** — 0 differing files / 0 differing lines, cache writes
  +0/+0 (the baseline leg's 0 is the load-bearing one), exit 0/0. Both fixes are byte-inert with
  `names` off.

## Not in this plan

- Identifier restore from package source.
- Other bundlers.
- Terser.
- The cheap index leads in `library-index.md` §11 (versions with no rows, prereleases); they can ride along with the
  step 1 rebuild.
