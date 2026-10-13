# Library matching: switches, settings and the index's life cycle

Status: DESIGN + CONFIG REVIEW, 2026-10-13. Nothing here is built. Companion to
[matching-architecture.md](./matching-architecture.md) (step 4 is where this
lands on main). Rule applied throughout: no backwards compatibility — a renamed
flag's old spelling is deleted, not aliased.

## Totals first

- **18 knobs touch library or vendor matching today**: 8 on main's pipeline
  and verbs, 2 in the eval harness, and 8 on the exp095 branches (2 pipeline
  flags, the diagnostics-as-switch coupling, 2 rule sets that live only in
  code, the lab's 5 environment variables counted as one, and the index
  builder's 14 flags counted as one).
- **Proposed: ONE switch and ONE path.** `--library-matching off|report|names`
  (default `off`) and `--library-index <dir|id>`. Every threshold moves into a
  named profile in a config file; the lab env vars and
  `--library-discovery-units` are deleted.
- **Index life cycle: a `humanify index` command family** — `fetch`, `build`,
  `info`, `verify`, `pack`/`unpack` — that builds offline from a package list,
  pins or a lockfile, reuses its shard store across builds (already true on
  the branch), names every index by a content hash (already true), and records
  every setting that decides compatibility. The run REFUSES an index whose
  versions do not match the binary.
- **Measured cost (index v2, the only real one):** 1,845 packages, 20,847
  versions, 430 MB raw / 77 MB tar.gz; **about one hour on 24 cores** (48 min
  for module/unit shards, then 12 min for the literal table with installs
  reused). Loading it in a run: 3.6 s (0.9 s without literals). Building is
  slow and the result is reusable, so it must be built once, ahead of time,
  and shared.
- **Two gaps found in the branch code:** the Bun slice is hard-wired to
  `--target node` while Claude Code is built for `--target bun` (the misses
  lane measured 52.1% → 71.2% of library bytes found when the index matches
  the app's target), and the run's loader checks only the units and literals
  versions, not the module key, recipe or bundler settings.

## 1. Flag inventory

### 1.1 Main (`crates/humanify-cli`)

| knob                                       | where                                                                                                 | what it does                                                                                                                 | default     | used by                                                          | proposal                                                                                                                                                                                                                                                                                                                                                                                       |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | ----------- | ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--skip-libraries` / `--no-skip-libraries` | `surface.rs` (two declarations); `settings.rs` `skip_libraries`; `unified.rs` library-detection phase | skip files the path/banner detector marks as library when asking the LLM for names                                           | skip (true) | every run; `tests/pipeline_stages.rs` uses `--no-skip-libraries` | **rename** to `--name-library-files` (plain on/off, default off) and delete both old spellings. A negated pair is the odd one out on this surface. Also fix the **stale header comment in `commander.rs`**: it still says both spellings set false, which was finding #19 and has been fixed in `surface.rs` by declaring two options. The dump's `skipLibraries` flag key follows the rename. |
| `--disable vendor-inherit`                 | `kill_switches.rs`                                                                                    | ablation: turn off whole-file reuse of an unchanged vendor library from the prior (exp046)                                   | on          | bisecting                                                        | **keep** (ablation switch, cross-version only)                                                                                                                                                                                                                                                                                                                                                 |
| `--disable manifest-prior-order`           | `kill_switches.rs`                                                                                    | ablation: turn off emitting the vendor manifest in prior order (exp047)                                                      | on          | bisecting                                                        | **keep**                                                                                                                                                                                                                                                                                                                                                                                       |
| `--bundler`, `--minifier`                  | `surface.rs`, toolchain                                                                               | force the toolchain, which also picks the library detector (`VendorRecord` vs `Default`) and, on the branch, the index slice | detected    | rare                                                             | **keep**                                                                                                                                                                                                                                                                                                                                                                                       |
| `humanify unpack --index <path>`           | `main.rs` `UnpackArgs`, written at `main.rs:587`                                                      | writes the extracted modules in BUNDLE order — nothing to do with a library index                                            | unset       | lab/debug                                                        | **rename to `--bundle-order <path>`**: `--index` will mean the library index everywhere else                                                                                                                                                                                                                                                                                                   |
| `humanify unpack --carries <path>`         | `main.rs`                                                                                             | writes the vendor content-carry report (findings #90/#95)                                                                    | unset       | debug                                                            | **keep**                                                                                                                                                                                                                                                                                                                                                                                       |
| `humanify libdetect`                       | `main.rs` verb                                                                                        | path/banner library detector gate, prints its verdict                                                                        | —           | gate tests                                                       | **keep**; later accepts `--library-index` to print the index verdict too                                                                                                                                                                                                                                                                                                                       |
| `humanify match`                           | `main.rs`, `match_verb.rs`                                                                            | prior-match dump; does no library detection                                                                                  | —           | debug                                                            | **keep**; gains `--library-matching`/`--library-index` in step 4                                                                                                                                                                                                                                                                                                                               |

### 1.2 Eval harness (`scripts/eval.ts`, `experiments/034-eval-harness/run.sh`)

| knob                                | what it does                                                                                             | proposal                                                                                                                        |
| ----------------------------------- | -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `--no-vendor`                       | skips SCORING the `vendor/` churn; does not change the pipeline                                          | **rename `--no-vendor-score`** so it cannot be read as "library matching off"                                                   |
| `--pipeline-arg <arg>` (repeatable) | the only way today to pass a library flag to every launch; recorded in `pipeline.json` as `pipelineArgs` | **keep**, but library matching gets first-class flags (§5) because the index id must be recorded and checked, not just the path |

### 1.3 exp095 branches (not on main)

| knob                                                          | where                                                                     | what it does                                                                                                                                                                                                                            | default | proposal                                                                                                                                                                               |
| ------------------------------------------------------------- | ------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--library-index <dir>`                                       | discovery-core / index-v2 `surface.rs`; `unified.rs` `load_library_index` | index for the report-only library lookup; read only if `--diagnostics` is also given; an unreadable index prints a message and the run continues without it                                                                             | unset   | **keep the name**; accept an index id too (looked up in the standard location, §4.5); used whenever `--library-matching` is not `off`; a missing or mismatched index **fails the run** |
| `--library-discovery-units`                                   | same                                                                      | lists every claimed, vetoed and queued unit in the diagnostics instead of totals                                                                                                                                                        | off     | **delete**; becomes `report.units = true` in the profile file                                                                                                                          |
| `--diagnostics` as the on-switch                              | `unified.rs` (`opts.diagnostics.is_some().then_some(DiscoveryRequest …)`) | discovery runs only when diagnostics are written                                                                                                                                                                                        | —       | **decouple**: `--library-matching report` turns it on; diagnostics only decide where the report goes                                                                                   |
| `LookupRules::default()`                                      | `libindex/lookup.rs`                                                      | 13 thresholds: min nodes 30, min content 3, cover 0.5, max packages 3, support 10 below 100 nodes, specificity 4× + 4096, agreement on, min literals 6, 20 bits, 90% cover, 10-bit margin; plus a targeted-round copy with min nodes 20 | code    | **move to a profile file** (§2)                                                                                                                                                        |
| `DiscoveryOptions::default()`                                 | `libdetect/discovery.rs`                                                  | closure floor 30, queue cap 256,000 bytes, 50 rounds, veto support                                                                                                                                                                      | code    | **move to the same profile**                                                                                                                                                           |
| `INDEX`, `AGREEMENT=0`, `SPEC=0`, `TARGETED=0`, `SUPPORT=<n>` | lab proto `discovery/proto/src/core_discover.rs`                          | lab overrides of the rules                                                                                                                                                                                                              | —       | **delete**; the lab passes `--library-profile <file>` instead                                                                                                                          |
| `humanify-index build` flags                                  | index-v2 `crates/humanify-index/src/main.rs`                              | `--packages`, `--pinned`, `--as-of` (required), `--registry`, `--store`, `--work`, `--out`, `--slices` (bun,esbuild), `--bun`, `--esbuild`, `--jobs` (2, max 4), `--policy` (`as-of` / `year-all+minor`), `--offline`, `--dry-run`      | —       | **becomes `humanify index build`** (§4); `--bun`/`--esbuild`/`--slices` fold into a recipe file; add `--lockfile` and a target per slice                                               |

## 2. One switch, one path, one profile file

```
--library-matching off | report | names     default: off
--library-index <dir | index-id>            required unless off
--library-profile <file | name>             default: the built-in "agreement" profile
```

- **`off`** — no index is read, no lookup runs. Byte-identical to today's
  main, by construction (no code path is entered).
- **`report`** — the lookup and discovery run and write their findings to the
  diagnostics and the stats file. Nothing is named, moved, frozen or skipped.
  Must be proven byte-identical to `off` on the output tree (warm neutrality).
- **`names`** — reserved for step 5: library claims may name or place code.
  Refused until an eval passes it ("not available in this build").

The **profile file** holds every threshold, named and versioned, e.g.
`crates/humanify-core/profiles/library-agreement.toml`:

```toml
name = "agreement"          # built-in; "structural" is the other one measured
version = 1
[lookup]
min_nodes = 30
min_content = 3
cover = 0.5
max_packages = 3
support = 10
strong = 100
specificity = [4, 4096]
agreement = true            # the index-v2 recommendation for anything that names code
min_literals = 6
literal_bits = 20.0
literal_cover = 0.9
literal_margin = 10.0
[lookup.targeted]
min_nodes = 20
support = 0
[discovery]
closure_floor = 30
queue_cap_bytes = 256000
max_rounds = 50
veto_support = "without-module"
[report]
units = false
```

The built-in profiles are compiled in; a file path overrides for lab work. The
run's stats record the profile's name, version and sha256.

**Missing or stale index — fail loud.** If `--library-matching` is not `off`
and the index is missing, unreadable, or built with different fingerprint,
recipe or format versions, the run exits 1 with a message naming both
versions and the command that rebuilds it. Today's branch behaviour (print a
message and run without the index) is fine for a report nobody reads and
wrong for an eval: a label would silently score "library matching on" with
no index. `off` never looks at the index, so a stale index can never break a
default run.

## 3. What decides whether an index fits a binary

The manifest (branch `libindex/format.rs` `Manifest`, format
`humanify-library-index/1`) already records format, as-of date, registry
snapshot, bundler slices with versions, packages, refusals, shard keys and
table hashes. Its id is the manifest's sha256. Missing today:

| setting                                                                        | today                                                                                                           | proposed                                                                          |
| ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| unit hash version                                                              | `fingerprints.units`, checked at load                                                                           | keep, checked                                                                     |
| literal version                                                                | `fingerprints.literals`, checked                                                                                | keep, checked                                                                     |
| module key version                                                             | `fingerprints.module` recorded, **not checked**                                                                 | check                                                                             |
| serializer preset versions (architecture §3.2)                                 | —                                                                                                               | record every preset the index used, check each                                    |
| known-globals set version                                                      | —                                                                                                               | record, check                                                                     |
| build recipe version                                                           | in each shard key (`recipe/4`) but not in the manifest                                                          | record, check                                                                     |
| per-slice bundler settings: target, format, minify, platform, entry conditions | **hard-coded** in `recipe.rs` (Bun: `--target node --format cjs --minify`; esbuild: `--platform=node --minify`) | a recipe per slice in the manifest, e.g. `bun@1.4.0 target=bun format=cjs minify` |
| families / stoplists data                                                      | sha256 in `data`                                                                                                | keep                                                                              |

**Matching an index to an app.** A run reads which bundler built the app
(already detected) and, where it can tell, the target. It looks up the slice
whose recipe matches. If the index has the bundler but not the target, the run
fails loud in `names` mode and warns in `report` mode, naming the missing
recipe. The misses lane's number is why: Claude Code is `--target bun`, the
index is `--target node`, and for packages with a `module` field and no
`exports` map Bun then bundles a different file (`dist-es` vs `dist-cjs`);
building with `--target bun` raised Claude Code's index-found library bytes
from 52.1% to 71.2% with 0 app code and 0 wrong family.

## 4. Index life cycle: `humanify index`

One subcommand family. Open decision whether it lives in the main binary or
stays the separate `humanify-index` binary with the same verbs (§6).

```
humanify index fetch   --packages list.txt [--pinned pins.txt] [--lockfile package-lock.json|bun.lock]
                       --as-of 2026-10-11 --mirror ~/.cache/humanify/npm-mirror
humanify index build   --packages … --as-of … --recipe recipes/claude-code.toml
                       --mirror … --store ~/.cache/humanify/index-store --out ~/.cache/humanify/indexes
                       [--policy as-of|year-all+minor] [--jobs N] [--dry-run]
humanify index info    <dir|id>       # id, as-of, recipes, versions, sizes, package count
humanify index verify  <dir|id>       # table hashes vs manifest; versions vs this binary
humanify index pack    <dir|id> -o ix.tar.gz
humanify index unpack  ix.tar.gz      # into the standard location, keyed by id
```

### 4.1 Offline build

- **`fetch` is the only network step.** It resolves the plan (packuments,
  the version policy, pins, a lockfile's exact versions) and downloads every
  tarball and packument into a mirror (an npm cache directory). Today's
  `--offline` covers packuments only; `npm install --before=…` still reaches
  the network.
- **`build` never touches the network.** It installs from the mirror
  (`npm install --offline --cache <mirror>`) and refuses any package not in
  it, naming it.
- **Inputs it accepts:** a package list (names → the policy picks versions),
  a pins file (exact `name@version`), and a lockfile (exact versions of
  everything an app actually ships — the most precise input; the misses lane
  found 8.4% of indexed-package misses were the wrong version).

### 4.2 Recipes

A recipe file names each slice's bundler, version and settings (target,
format, minify, platform, entry conditions). The recipe's hash goes into each
shard key (as `recipe/4` does now) and into the manifest. A run picks the
slice whose recipe matches the app (§3).

### 4.3 Incremental rebuild

Already built on the branch and kept: shards are keyed by package, version,
slice, bundler version, recipe version and fingerprint version, and are valid
forever for their key. Adding packages or versions builds only the new shards
and re-merges (the merge is cheap: no parsing). A version bump (e.g.
`units/3` for the normaliser) rebuilds only the affected shard kind. The
shard store is shared across indexes and dates.

### 4.4 Index id

The manifest's sha256 (already). Same shards in any order give the same bytes
(already tested). Every run, label and diagnostics block cites the id, never
the path.

### 4.5 Standard locations

No environment variable (the harness removed ambient env vars on purpose). A
flag with a fixed default: `--cache-root`, default `~/.cache/humanify/`,
holding `npm-mirror/`,
`index-store/` (shards), and `indexes/<id>/`. `--library-index <id>` resolves
there; a path also works.

### 4.6 Cost to record per build

`build` writes timings and sizes into a sidecar (not the manifest, which must
stay deterministic): wall time per phase, shards built vs reused, table sizes
raw and gzipped. Today's numbers, from the index-v2 README:

| index                                                                    | packages                       | versions                                                | raw                                                                | packed       | build                                              |
| ------------------------------------------------------------------------ | ------------------------------ | ------------------------------------------------------- | ------------------------------------------------------------------ | ------------ | -------------------------------------------------- |
| v2 (`b00fa735…`, as-of 2026-10-11, Bun 1.4.0 + esbuild 0.28.2, recipe/4) | 1,845 (2,960 owners with deps) | 20,847 of 22,356 planned (1,509 npm `ETARGET` failures) | 430 MB (literals 213, units 96 + 96, modules 12 + 13, manifest 10) | 77 MB tar.gz | 48 min modules/units on 24 cores + 12 min literals |
| cost index (as-of policy)                                                | 1,512 roots                    | 4,501                                                   | 232 MB                                                             | —            | 351 s                                              |

## 5. The eval harness side

- `npm run eval -- score <label> --library-matching report --library-index <id>`
  — first-class flags, validated up front like the others in `scripts/eval.ts`.
- `run.sh` resolves the id, runs `humanify index verify` before the first
  launch, and records in `pipeline.json` and every run manifest:
  `library: {matching: "off"|"report"|"names", indexId, profile: {name, version, sha256}}`.
  `off` is recorded explicitly, so an old label without the block reads as
  UNKNOWN, not as off.
- The dispatcher refuses to add cards to a label whose `library` block
  differs (same rule as base mode; `--force-mixed` overrides), and the
  leaderboard prints each label's library block and refuses mixed ones.
- The eval does not build indexes. A label names an index that already
  exists, and the run fails before any launch if it does not.

## 6. Open decisions for Andrew

1. **Default.** `off` until an eval shows `names` helps; should `report` be
   the default instead, once it is proven byte-neutral (costs ~1-4 s load per
   run plus lookup time)?
2. **Binary.** `humanify index …` inside the main binary (one tool, the
   builder links npm/bundler orchestration into it) or keep the separate
   `humanify-index` binary with the same verb names?
3. **Which recipes to ship first.** Claude Code needs Bun `--target bun`;
   rebuild v2 with that recipe (another hour) or add it as a second Bun slice?
4. **Lockfile input.** Is an app's lockfile ever available for the bundles
   humanify sees? If not, pins from declared readers (as v2 did) remain the
   precise input.
5. **`--skip-libraries` rename.** `--name-library-files` (default off) is
   proposed; it is a different question from library matching (whether the
   LLM names library files at all) and should stay separate from
   `--library-matching`.
6. **Where the profile file lives** (compiled-in TOML under `crates/` vs a
   repo-level `config/`), and whether lab runs may pass an arbitrary file.
