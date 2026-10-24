# Library matching: one setting, the index and its life cycle

Status: DESIGN + CONFIG REVIEW, 2026-10-13, updated the same day with Andrew's
six answers (§6). **§2 is built** (config sweep step 6, branch
`cleanup/config-sweep-5-6`; config-consolidation.md §5 step 6 records what
landed — including that the missing-slice WARNING runs against today's
one-slice-per-bundler index until exp097's recipe slices reach main). §3's
recipes, §4's `humanify index` verb and §5's eval flags are not built. Companion to
[matching-architecture.md](./matching-architecture.md) (step 4 is where this
lands on main) and [config-consolidation.md](./config-consolidation.md) (the
one config file this setting lives in). Rule applied throughout: no backwards
compatibility — a replaced flag's old spelling is deleted, not aliased.

## Totals first

- **18 knobs touch library or vendor matching today**: 8 on main's pipeline
  and verbs, 2 in the eval harness, and 8 on the exp095 branches (2 pipeline
  flags, the diagnostics-as-switch coupling, 2 rule sets that live only in
  code, the lab's 5 environment variables counted as one, and the index
  builder's 14 flags counted as one).
- **ONE decision, "what happens to code identified as library"**, replaces
  both `--skip-libraries`/`--no-skip-libraries` and the branch's index
  switches: `--library-matching off|report|names` (default **off**) plus
  `--library-index <dir|id>`. What happens to library code that has no index
  name (leave it as-is, today's default; or let the model name it, today's
  `--no-skip-libraries`) is a value in the config file, not a flag.
- **No separate library profile file.** Library thresholds are the
  `[library]` section of humanify's ONE config file (`--config <file>`,
  built-in defaults compiled in). `--profile` already means the perf trace.
- **Indexes are organised around the packing tool.** An index is a set of
  recipe slices (bundler + target + format + minify), because the recipe
  changes what gets bundled. Building or fetching an index names its recipes;
  a run picks the slice from the toolchain it detected once. A detected
  recipe with no slice is a loud WARNING plus a diagnostics entry.
- **`humanify index …` lives in the main binary**: `fetch`, `build`, `info`,
  `verify`, `pack`, `unpack`. Offline build, content-addressed id, reusable
  shard store (already true on the branch).
- **Measured cost (index v2, the only real one):** 1,845 packages, 20,847
  versions, 430 MB raw / 77 MB tar.gz, **about one hour on 24 cores** (48 min
  module/unit shards + 12 min literal table). Load in a run: 3.6 s (0.9 s
  without literals).
- **Gaps found in the branch code:** the Bun recipe is hard-wired to
  `--target node` while Claude Code is built for `--target bun` (misses lane:
  52.1% → 71.2% of library bytes found with the matching target); the loader
  checks only the units and literals versions; `--offline` covers package
  metadata but `npm install` still reaches the network.

## 1. Flag inventory

### 1.1 Main (`crates/humanify-cli`)

| knob                                       | where                                                                                                                                    | what it does today                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | default     | used by                                                             | proposal                                                                                                                                                                                                                                                              |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--skip-libraries` / `--no-skip-libraries` | `surface.rs` (two declarations); `config.rs` (`Config::build`, resolved once into `config.library.skip_libraries` — config sweep step 4) | three effects, all keyed on the path/banner detector (`libdetect.rs`): (1) **whole library files** are dropped from per-file naming (`unified.rs:655`, `filter_libraries`); (2) **library functions in mixed files are frozen** before any transfer (`rename/transfer/stage.rs` `library_freeze` → `libdetect/function_carry.rs`, only when the graph found no wrapper IIFE); (3) those frozen functions get the deterministic `<library>_<name>` prefix pass, no LLM (`naming/driver/library.rs`) | skip (true) | every run; `--no-skip-libraries` only in `tests/pipeline_stages.rs` | **delete both spellings**. Becomes `[library] unnamed = "leave"` (today's default) / `"model"` (today's `--no-skip-libraries`), §2. (The stale `commander.rs` header went with the port in config sweep step 3; passing both spellings is now refused as a conflict.) |
| `--disable vendor-inherit`                 | `kill_switches.rs`                                                                                                                       | ablation: no whole-file reuse of an unchanged vendor library (exp046)                                                                                                                                                                                                                                                                                                                                                                                                                              | on          | bisecting                                                           | **keep** (ablation, cross-version only)                                                                                                                                                                                                                               |
| `--disable manifest-prior-order`           | `kill_switches.rs`                                                                                                                       | ablation: no prior-order vendor manifest (exp047)                                                                                                                                                                                                                                                                                                                                                                                                                                                  | on          | bisecting                                                           | **keep**                                                                                                                                                                                                                                                              |
| `--bundler`, `--minifier`                  | `surface.rs` → toolchain                                                                                                                 | force detection's verdict; this also picks the library detector (`VendorRecord` vs `Default`) and, with an index, the recipe slice                                                                                                                                                                                                                                                                                                                                                                 | detected    | rare                                                                | **keep**                                                                                                                                                                                                                                                              |
| `humanify unpack --index <path>`           | `main.rs` `UnpackArgs`, written at `main.rs:587`                                                                                         | writes extracted modules in BUNDLE order — unrelated to a library index                                                                                                                                                                                                                                                                                                                                                                                                                            | unset       | lab/debug                                                           | **rename `--bundle-order <path>`**                                                                                                                                                                                                                                    |
| `humanify unpack --carries <path>`         | `main.rs`                                                                                                                                | vendor content-carry report (#90/#95)                                                                                                                                                                                                                                                                                                                                                                                                                                                              | unset       | debug                                                               | **keep**                                                                                                                                                                                                                                                              |
| `humanify libdetect`                       | `main.rs` verb                                                                                                                           | prints the path/banner detector's verdict                                                                                                                                                                                                                                                                                                                                                                                                                                                          | —           | no caller found in tests, scripts or harness                        | **keep only if** it gains the index verdict (`--library-index`) in step 4; otherwise delete                                                                                                                                                                           |
| `humanify match`                           | `main.rs`, `match_verb.rs`                                                                                                               | prior-match dump, no library detection                                                                                                                                                                                                                                                                                                                                                                                                                                                             | —           | `experiments/lib/match-truth`                                       | **keep**; reads the same `[library]` config in step 4                                                                                                                                                                                                                 |

### 1.2 Eval harness

| knob                                        | what it does                                                                             | proposal                                                                                              |
| ------------------------------------------- | ---------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `--no-vendor` (`run.sh`, `scripts/eval.ts`) | skips SCORING `vendor/` churn; pipeline unchanged                                        | **rename `--no-vendor-score`**                                                                        |
| `--pipeline-arg <arg>`                      | the only way to pass a library flag today; recorded as `pipelineArgs` in `pipeline.json` | **keep**, but library matching gets first-class flags (§5): the index id must be checked and recorded |

### 1.3 exp095 branches (not on main)

| knob                                                      | where                                                                                                                                                                                                                                                               | what it does                                                                                                                                                                                                                              | proposal                                                                                                             |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `--library-index <dir>`                                   | `surface.rs`; `unified.rs` `load_library_index`                                                                                                                                                                                                                     | report-only lookup; read only with `--diagnostics`; an unreadable index prints a message and the run continues                                                                                                                            | **keep the name**; also accept an index id; required unless `--library-matching off`; missing or mismatched → exit 1 |
| `--library-discovery-units`                               | same                                                                                                                                                                                                                                                                | per-unit listing in the diagnostics                                                                                                                                                                                                       | **delete**; `[library] report_units = true`                                                                          |
| `--diagnostics` as the on-switch                          | `unified.rs` (`opts.diagnostics.is_some().then_some(DiscoveryRequest …)`)                                                                                                                                                                                           | discovery runs only when diagnostics are written                                                                                                                                                                                          | **delete the coupling**; `--library-matching report` turns it on                                                     |
| `LookupRules::default()`                                  | `libindex/lookup.rs`                                                                                                                                                                                                                                                | 13 thresholds (min nodes 30, min content 3, cover 0.5, max packages 3, support 10 below 100 nodes, specificity 4× + 4096, agreement on, 6 literals, 20 bits, 90% cover, 10-bit margin) + a targeted-round copy (min nodes 20, no support) | **move to `[library.lookup]`**                                                                                       |
| `DiscoveryOptions::default()`                             | `libdetect/discovery.rs`                                                                                                                                                                                                                                            | closure floor 30, queue cap 256,000 B, 50 rounds, veto support                                                                                                                                                                            | **move to `[library.discovery]`**                                                                                    |
| `INDEX`, `AGREEMENT=0`, `SPEC=0`, `TARGETED=0`, `SUPPORT` | lab proto `discovery/proto/src/core_discover.rs`                                                                                                                                                                                                                    | lab overrides                                                                                                                                                                                                                             | **delete**; lab passes `--config <file>`                                                                             |
| `humanify-index build` (14 flags)                         | index-v2 `crates/humanify-index/src/main.rs`: `--packages`, `--pinned`, `--as-of`, `--registry`, `--store`, `--work`, `--out`, `--slices` (bun,esbuild), `--bun`, `--esbuild`, `--jobs` (2, max 4), `--policy` (`as-of`/`year-all+minor`), `--offline`, `--dry-run` | the offline builder                                                                                                                                                                                                                       | **becomes `humanify index build`** (§4); `--slices`/`--bun`/`--esbuild` become `--recipe`                            |

## 2. The one library setting

```
CLI:     --library-matching off | report | names     default off
         --library-index <dir | index-id>             required unless off
Config:  [library] section of the one config file (--config <file>)
```

| `--library-matching` | detection                                    | index                                       | what library code gets                                                                                        |
| -------------------- | -------------------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `off` (default)      | path/banner (as today)                       | not read                                    | per `unnamed`                                                                                                 |
| `report`             | path/banner + index lookup + graph discovery | read, slice chosen from the detected recipe | per `unnamed`; index findings go to diagnostics/stats only. Must be byte-identical to `off` (warm neutrality) |
| `names`              | same as report                               | read                                        | index claims may name code (step 5); refused until an eval passes it                                          |

`[library] unnamed` — what happens to code identified as library that has no
name from the index (in `off` and `report`, that is all of it):

- `"leave"` (default) — exactly today's `--skip-libraries`: library files
  are not model-named, library functions in mixed files are frozen and get
  the deterministic `<library>_<name>` prefix.
- `"model"` — exactly today's `--no-skip-libraries`: no file filter, no
  freeze, no prefix pass; the model names everything. The move must prove
  byte-identity with today's `--no-skip-libraries` on the e2e fixtures.

The config section, with the compiled-in defaults. Since exp109 (2026-10-14)
the `[library.lookup]` and `[library.discovery]` defaults are the overnight
campaign's MEASURED recommendation (experiments/106-matcher-hygiene, judged
end to end on the campaign index in experiments/109-default-config), read
from `LookupRules::recommended()` / `DiscoveryOptions::recommended()`.
`LookupRules::default()` / `DiscoveryOptions::default()` stay the engine's
baseline (the agreement rule, every guard off), which the unit tests compare
each rule against. Matching itself stays `off` by default, so these keys
change only `report` (and, once allowed, `names`) runs; `humanify.example.toml`
is generated from the registry and holds every value below. Which index to
point `library.index` at: docs/design/library-index.md §10.

```toml
[library]
unnamed = "leave"              # "leave" | "model"
report_units = false

[library.lookup]
min_nodes = 30
min_content = 3
min_function_content = 0
cover = 0.5
max_packages = 3
support = 10
strong = 100
specificity = true             # factor 4 + slack 4096 B
specificity_factor = 4
specificity_slack = 4096
agreement = false              # exp109: the STRUCTURAL rule (dev held-out F0.5 85.0 vs agreement 75.2, exp106)
min_literals = 6               # the literal rule below is read only when agreement = true
literal_score = 20.0
literal_cover = 0.9
literal_margin = 10.0
literal_margin_by_family = true    # exp106 (agreement rule only)
max_packages_by_family = false     # exp125: count a print's owners by FAMILY against max_packages
terser_slices = "off"          # exp127: also read the terser-built slices (webpack, rollup): "union", "fallback",
terser_share = 0.2             #   or "auto" (union for a bundle the terser slices alone hold >= terser_share of)

[library.lookup.targeted]
min_nodes = 20
support = 0

[library.discovery]
closure_floor = 30
queue_cap_bytes = 256000
max_rounds = 50
veto_support = "without-module" # or "all-rules" / "shown" (exp144: all-rules support only for an unambiguous seed of a family the bundle shows without module fill)
evidence_conflict = true       # exp102 precision guards on seeds / propagated claims
literal_conflict = true        #   (experiments/102-precision), ON since exp109
contested_cover = 0.05
specific_rivals = true         # exp106: the two conflict guards read rivals under specificity
banner_names = "known"         # exp106 matcher hygiene (experiments/106-matcher-hygiene)
derived_families = true        # exp106: one family = npm-metadata links (families-derived.json)
payload_min_bytes = 4096       # exp106: a propagated string payload needs its own evidence (exp113: own bytes; a base64 payload naming the package is its own)
helper_families = 0            # exp106: off (N=4 costs right claims, N=12 ~neutral)
helper_prints = "seed"         # ON since 2026-10-22 (exp138: P >= the previous default on all 21 sets); exp129: a unit whose whole print the
                               #   index's helper table (tslib, @babel/runtime ... own code) holds is no other package's seed: "seed" (dropped),
                               #   "credit" (the helper package's) or "off"; needs an index with helpers.jsonl
context_presence = 1.5         # exp114, ON since exp117: a package seen only in index modules
context_presence_slack = 16384 #   bigger than the bundle's groups is no seed (experiments/114-dev-precision)
shadow_ties = true             # exp114, ON since 2026-10-15 (Andrew): fork claims gone on webpack/rollup; costs 1,035 B on test2-esbuild
presence_fence = true          # exp114, ON since exp117: what context_presence drops is named by no one
declared_max_bytes = 65536     # exp112, ON since exp117: a declared seed on a statement over N bytes
                               #   is no seed (experiments/112-test-precision, 117-default-flip)
copy_presence = 0.1            # exp121, ON since 2026-10-16 ("v2", clean on 19 sets on k2-all): a package whose every index seed is an inlined copy of a
copy_presence_factor = 4       #   smaller-context origin (share >= copy_presence, origin's module <= 1/factor)
copy_presence_origin = true    #   is no seed; _origin gives the copy seeds to that origin; _own = share of
copy_presence_own = 0.1        #   seed bytes non-copy seeds need to vouch (experiments/121-sponge-rule)
factory_bodies = true          # ON since 2026-10-21; exp128: cut each bundled CommonJS module into its body statements
                               #   (judged and propagated as lazy-module bodies; experiments/128-cjs-scope)
factory_tie_fence = true       # ON since 2026-10-21; exp132: a cut factory whose every index seed is a cross-family
                               #   tie (no untied/declared/banner seed) is claimed by no one (experiments/132-factory-precision)
factory_whole_seed = true      # ON since 2026-10-21; exp132: a package names a cut factory only with one whole-statement
                               #   index seed (or a declared/banner seed) there; function-print seeds alone are dropped
factory_banner_index = true    # ON since 2026-10-21; exp132: a banner seed on a cut factory's statement another family's
                               #   index prints cover more of is refused
```

Since exp117 (2026-10-15) `context_presence = 1.5`, `context_presence_slack = 16384`,
`presence_fence = true` and `declared_max_bytes = 65536` are on too: together they lowered
neither precision nor F0.5 on any of 19 sets (dev, apps, bundlers, pop, test, test2) on the
k2-all index (experiments/117-default-flip). To reproduce a pre-exp117 run, set them back to
`0.0`, `0`, `false`, `0`.

To reproduce a pre-exp109 run, set the old values back explicitly (and the exp117 keys as above):
`agreement = true`, `literal_margin_by_family = false`, `evidence_conflict =
false`, `literal_conflict = false`, `contested_cover = 0.0`, `specific_rivals
= false`, `banner_names = "any"`, `derived_families = false`,
`payload_min_bytes = 0`. (The exp096 harness already pins the rule per label
with `--rule`; every other key reads the compiled-in default.)

**Missing or stale index — fail loud, before any work.** With
`--library-matching` not `off`, a missing, unreadable or version-mismatched
index exits 1, naming both versions and the rebuild command; so does a mode
other than `off` with no index, and an index given with mode `off` (it
would be silently ignored). `off` never opens the index, so a stale index
cannot break a default run. All of these are checked at the one parse
point, before unpacking starts; the full refusal table (conflicts between a
flag and the config file, unknown keys, wrong types, out-of-range
thresholds, retired keys) is config-consolidation.md §3.1, and it applies
to every `[library]` key.

## 3. Recipes: an index is a set of packing-tool slices

The packing tool decides what gets bundled: Bun `--target bun` resolves a
package's `module` field (`dist-es`) where `--target node` resolves `main`
(`dist-cjs`), and escapes non-ASCII strings differently. So:

- **A recipe** = bundler + bundler version + target + format + minify (+
  platform and entry conditions where the bundler has them). Today's two,
  hard-wired in `recipe.rs`: Bun `--target node --format cjs --minify`;
  esbuild `--platform=node --minify`. **First shipped index (Andrew,
  2026-10-13): three slices** — Bun `--target bun` (Claude Code's), Bun
  `--target node`, and esbuild — each `--format cjs --minify`. The Bun
  `target bun` slice is new (another ~hour of Bun builds on 24 cores; the
  shard store reuses every install).
- **An index is a set of recipe slices.** `humanify index build` and
  `fetch` must name their recipes (`--recipe bun:target=bun,format=cjs,minify`,
  repeatable, or a recipe file); there is no default recipe. Each slice's
  recipe and its hash are in the manifest and in every shard key.
- **The run picks the slice from the toolchain resolved once by detection**
  (the detection-built-pipeline rule: resolve plugin pieces once, pass them
  down; the minifier verdict is untrusted; unsure → Bun). The toolchain gains
  one more piece, the recipe, with its reason like every other piece
  (`toolchain_record`). The store gets that one slice; nothing below
  re-detects.
- **No matching slice:** in `report` a `WARNING: library index <id> has no
slice for recipe <r> (has: …)` line on stderr, and a `libraryDiscovery.
recipeMissing` entry in diagnostics and stats; detection falls back to
  path/banner. In `names`: exit 1. Never a silent miss. The check needs
  detection's recipe, so it runs right after detection and still before
  unpacking. **Report stays a warning in the binary** (a report-only run
  has lost only its report, and the user still gets their output), **but
  the eval refuses it**: `run.sh` runs `humanify index verify <id> --recipe
  <detected>` per pair before the first launch and stops the label on a
  missing slice, because a label's library numbers from a run without a
  slice are meaningless.
- **When detection cannot tell the target** (a Bun bundle that does not say
  `bun` vs `node`), the toolchain records `target: unknown`; the run uses the
  slice the index marks for that bundler as default, and says so in the
  record.

**What else decides whether an index fits a binary** (manifest =
branch `libindex/format.rs` `Manifest`, format `humanify-library-index/1`):

| setting                                        | today                                  | proposed                                |
| ---------------------------------------------- | -------------------------------------- | --------------------------------------- |
| unit hash version                              | `fingerprints.units`, checked at load  | keep, checked                           |
| literal version                                | `fingerprints.literals`, checked       | keep, checked                           |
| module key version                             | recorded, **not checked**              | check                                   |
| serializer preset versions (architecture §3.2) | —                                      | record each, check                      |
| known-globals set version                      | —                                      | record, check                           |
| recipe per slice                               | only the recipe version, in shard keys | full recipe + hash in manifest, checked |

## 4. Index life cycle: `humanify index` (main binary)

```
humanify index fetch   --recipe … --packages list.txt [--pins pins.txt] [--lockfile …]
                       --as-of 2026-10-11
humanify index build   --recipe … --packages … [--pins …] [--lockfile …] --as-of …
                       [--policy as-of|year-all+minor] [--jobs N] [--dry-run]
humanify index info    <dir|id>       # id, as-of, recipes, versions, sizes
humanify index verify  <dir|id>       # table hashes vs manifest; versions vs this binary
humanify index pack    <dir|id> -o ix.tar.gz
humanify index unpack  ix.tar.gz      # into the standard location, keyed by id
```

### 4.1 Which versions to build: pins, not lockfiles

Package lists, pins and lockfiles are **inputs to BUILDING the index** —
they choose which package versions get built. They are never an input to
matching. For the bundles humanify de-minifies we normally have **no
lockfile**; the pins come from the bundle itself:

- **declared versions the bundle carries** — the package readers
  (`modules/identity.rs`, `identity::readers` on main; the branch adds `scan_app_declarations` for every top-level statement), the
  vendor record, `/*! name vX */` banners;
- a **version policy** for packages named without a version (`as-of`:
  latest, previous patch, two previous majors; `year-all+minor`);
- a **lockfile, optional**, when one exists (e.g. indexing your own app) —
  just another pin source.

A natural verb follows: `humanify index pins <bundle>` prints the pins the
bundle declares, for `build --pins`. (Index v2 did this by hand with
`declared_all.rs`.)

### 4.2 Offline build

`fetch` is the only network step: it resolves the plan and downloads every
packument and tarball into an npm mirror. `build` installs only from the
mirror (`npm install --offline --cache <mirror>`) and refuses, by name, any
package not in it. Today's `--offline` covers packuments only.

### 4.3 Incremental rebuild (exists on the branch, kept)

Shards are keyed by package, version, recipe (hash), bundler version and
fingerprint version, and are valid forever for their key. Adding packages,
versions or a recipe builds only the new shards and re-merges (cheap, no
parsing). A version bump (e.g. `units/3`) rebuilds only that shard kind.

### 4.4 Index id and location

The id is the manifest's sha256 (exists); the same shards in any order give
the same bytes (tested). Runs, labels and diagnostics cite the id. Location:
the config file's `[paths] cache_root` (default `~/.cache/humanify/`) holds
`npm-mirror/`, `index-store/` and `indexes/<id>/`; no environment variable.

### 4.5 Cost, recorded per build

`build` writes wall time per phase, shards built vs reused and table sizes to
a sidecar (the manifest stays deterministic). Today's numbers (index-v2
README):

| index                                                                                        | packages                       | versions                                       | raw                                                                | packed       | build                                              |
| -------------------------------------------------------------------------------------------- | ------------------------------ | ---------------------------------------------- | ------------------------------------------------------------------ | ------------ | -------------------------------------------------- |
| v2 (`b00fa735…`, as-of 2026-10-11, Bun 1.4.0 + esbuild 0.28.2, recipe/4, both `node` target) | 1,845 (2,960 owners with deps) | 20,847 of 22,356 planned (1,509 npm `ETARGET`) | 430 MB (literals 213, units 96 + 96, modules 12 + 13, manifest 10) | 77 MB tar.gz | 48 min modules/units on 24 cores + 12 min literals |
| cost index (as-of policy)                                                                    | 1,512 roots                    | 4,501                                          | 232 MB                                                             | —            | 351 s                                              |

## 5. The eval harness side

- `npm run eval -- score <label> --library-matching report --library-index <id>`,
  validated up front with the other flags in `scripts/eval.ts`.
- `run.sh` runs `humanify index verify <id>` before the first launch and
  records `library: {matching, indexId, recipes, configSha256}` in
  `pipeline.json` and every run manifest. `off` is recorded explicitly; a
  label without the block reads as UNKNOWN.
- The dispatcher refuses to add cards to a label whose `library` block
  differs (`--force-mixed` overrides); the leaderboard prints the block and
  refuses mixed labels. The eval never builds an index.

## 6. Decisions (Andrew, 2026-10-13)

1. **Default `--library-matching off`** for now.
2. **`humanify index …` lives inside the main binary.**
3. **Indexes are structured around the packing tool**: an index is a set of
   recipe slices; building/fetching names the recipe; the run picks the slice
   from the toolchain detected once; a missing slice is a loud WARNING plus a
   diagnostics entry in report mode (§3).
4. **Lockfiles are an optional pin source for building the index**, not an
   input to matching; for de-minified bundles the pins come from the bundle
   (§4.1).
5. **One decision, not two flags**: `--skip-libraries`/`--no-skip-libraries`
   are deleted; the treatment of unnamed library code is `[library] unnamed`
   (§2). No `--name-library-files`.
6. **No separate profile file**: one global config file (`--config`,
   defaults compiled in), library thresholds as its `[library]` section;
   see config-consolidation.md for the file itself and the `--profile` name
   clash.

7. **Config file is TOML; only operational flags stay** (input, output,
   model endpoint, `--library-matching`, `--library-index`, `--config`);
   conflicts and bad values are refused loudly before any work.
8. **First index slices:** Bun target bun, Bun target node, esbuild (§3).

Still open: whether `--library-matching report` becomes the default once it
is proven byte-neutral.
