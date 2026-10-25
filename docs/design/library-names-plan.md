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

## Not in this plan

- Identifier restore from package source.
- Other bundlers.
- Terser.
- The cheap index leads in `library-index.md` §11 (versions with no rows, prereleases); they can ride along with the
  step 1 rebuild.
