# Library matching: one setting, the index and its life cycle

Status: DESIGN + CONFIG REVIEW, 2026-10-13, updated the same day with Andrew's
six answers (§6). Nothing here is built. Companion to
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

| knob                                       | where                                                                                                                                                 | what it does today                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | default     | used by                                                             | proposal                                                                                                                                                                                                                                                                              |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--skip-libraries` / `--no-skip-libraries` | `surface.rs` (two declarations); `settings.rs:174` (`unwrap_or(true)`); read back from the parsed options in `unified.rs` (`v.bool("skipLibraries")`) | three effects, all keyed on the path/banner detector (`libdetect.rs`): (1) **whole library files** are dropped from per-file naming (`unified.rs:655`, `filter_libraries`); (2) **library functions in mixed files are frozen** before any transfer (`rename/transfer/stage.rs` `library_freeze` → `libdetect/function_carry.rs`, only when the graph found no wrapper IIFE); (3) those frozen functions get the deterministic `<library>_<name>` prefix pass, no LLM (`naming/driver/library.rs`) | skip (true) | every run; `--no-skip-libraries` only in `tests/pipeline_stages.rs` | **delete both spellings**. Becomes `[library] unnamed = "leave"` (today's default) / `"model"` (today's `--no-skip-libraries`), §2. Also fix the **stale `commander.rs` header**, which still says both spellings set false (finding #19, fixed in `surface.rs` by two declarations). |
| `--disable vendor-inherit`                 | `kill_switches.rs`                                                                                                                                    | ablation: no whole-file reuse of an unchanged vendor library (exp046)                                                                                                                                                                                                                                                                                                                                                                                                                              | on          | bisecting                                                           | **keep** (ablation, cross-version only)                                                                                                                                                                                                                                               |
| `--disable manifest-prior-order`           | `kill_switches.rs`                                                                                                                                    | ablation: no prior-order vendor manifest (exp047)                                                                                                                                                                                                                                                                                                                                                                                                                                                  | on          | bisecting                                                           | **keep**                                                                                                                                                                                                                                                                              |
| `--bundler`, `--minifier`                  | `surface.rs` → toolchain                                                                                                                              | force detection's verdict; this also picks the library detector (`VendorRecord` vs `Default`) and, with an index, the recipe slice                                                                                                                                                                                                                                                                                                                                                                 | detected    | rare                                                                | **keep**                                                                                                                                                                                                                                                                              |
| `humanify unpack --index <path>`           | `main.rs` `UnpackArgs`, written at `main.rs:587`                                                                                                      | writes extracted modules in BUNDLE order — unrelated to a library index                                                                                                                                                                                                                                                                                                                                                                                                                            | unset       | lab/debug                                                           | **rename `--bundle-order <path>`**                                                                                                                                                                                                                                                    |
| `humanify unpack --carries <path>`         | `main.rs`                                                                                                                                             | vendor content-carry report (#90/#95)                                                                                                                                                                                                                                                                                                                                                                                                                                                              | unset       | debug                                                               | **keep**                                                                                                                                                                                                                                                                              |
| `humanify libdetect`                       | `main.rs` verb                                                                                                                                        | prints the path/banner detector's verdict                                                                                                                                                                                                                                                                                                                                                                                                                                                          | —           | no caller found in tests, scripts or harness                        | **keep only if** it gains the index verdict (`--library-index`) in step 4; otherwise delete                                                                                                                                                                                           |
| `humanify match`                           | `main.rs`, `match_verb.rs`                                                                                                                            | prior-match dump, no library detection                                                                                                                                                                                                                                                                                                                                                                                                                                                             | —           | `experiments/lib/match-truth`                                       | **keep**; reads the same `[library]` config in step 4                                                                                                                                                                                                                                 |

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

The config section, with today's values as the compiled-in defaults:

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
specificity = [4, 4096]
agreement = true               # index-v2: agreement for anything that names code
min_literals = 6
literal_bits = 20.0
literal_cover = 0.9
literal_margin = 10.0

[library.lookup.targeted]
min_nodes = 20
support = 0

[library.discovery]
closure_floor = 30
queue_cap_bytes = 256000
max_rounds = 50
veto_support = "without-module"
```

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
