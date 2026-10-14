# The library fingerprint index — an offline, as-of, reproducible builder

STATUS: v1 BUILT 2026-10-11 on branch `exp095-index-builder` (exp095,
finding #98). Nothing in a humanify run reads the index yet: this is the
builder and the format. The pipeline side (looking modules up, naming from
it) is exp095 step 2 and later, and is described here only so the format
serves it.

**Why.** exp095 showed that an index of npm library fingerprints can name
vendored modules and find library code in `src/`
(`experiments/095-library-fingerprints/SYNTHESIS.md`; the scale lane's
README at `/work/exp095-library-fingerprints/scale/README.md`). The lab
built its index with throwaway scripts (`pick.mjs`, `ixbuild.mjs`,
`label.mjs`, `srclits.mjs`, a private `fphash` binary). An index that ships
has to be rebuilt as npm moves, so the build must be its own reproducible
program. And it must be able to build an index **as of** a date, so a
bundle from 2024 is never matched against library versions released after
it (Andrew, 2026-10-11).

## Totals first

- **Where it lives.** A new crate, `crates/humanify-index`, with its own
  binary `humanify-index`. The fingerprints themselves are computed by
  `humanify-core` — the same functions the pipeline will call to look a
  module up. The builder never hashes anything on its own.
- **What a build does.** Read a package list and a date → pick the versions
  that existed on that date → install each one (dependencies as they were
  when that version was published) → bundle it with each bundler → record
  every bundled module's fingerprint and every source file's literals in a
  per-version **shard** → merge the shards that belong to the chosen date
  into one **index**, storing each fingerprint once with the version ranges
  it appears in.
- **Reproducible and incremental.** A shard is keyed by what went into it
  (package, version, bundler and its version, fingerprint versions). A
  later build reuses every shard whose key still matches and builds only
  what is new. The index files are written in a fixed order: the same
  shards and the same date give the same bytes.
- **Recorded, for reproducibility.** An index is identified by the sha256
  of its manifest, and is never edited in place. A run records the id it
  used, so re-running with that index gives the same result. It is not a
  churn guard: an unchanged module keeps its prior name by the exact-hash
  carry before the index is ever consulted (§8).

- **Which index to use.** §10: `k2-l12m4` (Bun/esbuild) with `step4-wr`
  for webpack/rollup bundles, until `k2-all` (all five slices) is built.

## 1. Separation: a builder outside the run, sharing the pipeline's code

| Piece                                                                                                  | Lives in                                                                             | Why there                                                                                                                                                                                                                |
| ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Module fingerprint (whole-module key of a bundled CommonJS module)                                     | `humanify_core::modules::vendor_content::bundle_content_keys` → `vendor_content_key` | The pipeline already keys vendor modules this way (carry by content). The index must hold the key the pipeline will compute for the same module, so it is the same function, not a copy.                                 |
| Literal fingerprint (the strings, property names, numbers and regexes a minifier cannot rename)        | `humanify_core::libindex::literals`                                                  | The pipeline will extract the literals of a bundled module and look them up in an index built from npm SOURCE. Both sides call one extractor with a mode (`Source` / `Bundled`).                                         |
| Index format: manifest, rows, version ranges, the semver order ranges are read in                      | `humanify_core::libindex::format`                                                    | The pipeline reads it; the builder writes it. One set of types.                                                                                                                                                          |
| Bundler slice (which part of the index a bundle is looked up in)                                       | `humanify_core::libindex::IndexSlice`                                                | A plugin piece (below).                                                                                                                                                                                                  |
| Registry reading, as-of selection, npm install, bundler runs, sourcemap attribution, shards, the merge | `crates/humanify-index`                                                              | Network, subprocesses (npm, bun, esbuild) and a work tree of installs have no place in the pipeline binary, which must stay offline and deterministic. The builder's release cadence is the index's, not the pipeline's. |

Rejected: a `humanify index build` verb in the pipeline binary. It would
put npm and network code in the binary the eval scores
(`rust:build -p humanify-cli`), and every builder change would change the
scored binary's hash. A separate crate keeps "how the index is made" out of
"how a bundle is processed", while the shared fingerprint functions stay in
one place.

## 2. The build, stage by stage

```
humanify-index build --packages <list> --as-of <date> --registry <dir> \
    --store <dir> --work <dir> --out <dir> [--slices bun,esbuild] \
    [--bun <bin>] [--esbuild <bin>] [--jobs N] [--offline] [--dry-run]
```

1. **Registry snapshot.** For each package (and each bundler, see §3) the
   builder needs its packument's `time` map and its `versions` list. They
   are fetched once (`npm view <pkg> time versions --json`) into
   `--registry <dir>` as `<pkg>.json` with the fetch time, and read from
   there afterwards. `--offline` refuses to fetch. The snapshot directory
   is an INPUT: the manifest records every packument's sha256 and the
   earliest and latest fetch time.
2. **Plan (as-of selection, §3).** Per package: the versions to index,
   each with its publish time, and every version refused with the reason.
   `--dry-run` prints the plan and, per version and slice, whether its
   shard already exists or would be built — then stops.
3. **Install.** Per selected `pkg@version`, in a fresh work directory:
   `npm install --ignore-scripts --before=<that version's publish time>`
   (retried once with `--legacy-peer-deps`). `--before` makes npm resolve
   every dependency range as it would have resolved on the day this version
   was published, so no dependency newer than the version itself (and so
   none newer than the as-of date) enters the build. No package code is run.
   Input sourcemap comments are stripped so a bundler does not chain them.
4. **Bundle**, per slice: the slice's recipe (§6) builds a `require()` entry
   and an `import()` entry, minified, with a sourcemap. An unresolvable
   specifier is retried as external (up to 4 rounds); top-level await is
   retried as ESM.
5. **Fingerprint.** On each bundle: `bundle_content_keys` (the pipeline's
   module classification + content key) → per CommonJS module its key and
   body span. The sourcemap attributes the span to a source file (the file
   owning the majority of the mapping segments inside it, and at least 90%
   of them — a mixed span is dropped, as the lab's "pure library spans");
   the file's nearest `package.json` gives the owner package and version.
   Literals: every `.js/.mjs/.cjs/.json` file (≤ 4 MB, no `.d.ts`) of every
   installed package, owned by its nearest `package.json`, through
   `libindex::literals` in `Source` mode. Pre-bundled files ("sponges":
   `bundled`, `bundle.`, `.min.`, `.global.`, `.umd.`, `browser` in the
   file name, or a name starting `cdn.`) are dropped at extraction; the
   scale lane measured that they cause most wrong literal claims at size.
6. **Shard.** One file per `pkg@version` per kind:
   `store/shards/<slice>/<pkg>@<version>.json` (module rows) and
   `store/shards/literals/<pkg>@<version>.json` (literal rows). Rows are
   labelled with their OWNER `package@version` — a dependency's modules are
   recorded under the dependency, not the package that pulled it in.
7. **Merge (compaction).** The shards of the selected versions are merged
   into the index directory (§4). Shards of versions not selected for this
   date are ignored, so one store serves indexes for many dates.

## 3. As-of builds

**Rule.** `--as-of D` (an RFC 3339 time, or a date meaning the END of that
UTC day) admits a version only when its registry publish time
(`time[version]`) is ≤ D.

**Which versions, per package.** The lab's policy (`scale/scripts/pick.mjs`),
applied to the admitted, non-prerelease versions only:

- `latest` — the highest admitted version. NOT `dist-tags.latest`: that tag
  is today's, and it may point past D.
- `patch` — the highest earlier patch of that major.minor.
- `major-1`, `major-2` — the highest version of each of the two previous
  majors (for 0.x the minor plays the major's role).

An explicit `name@version` in the package list (a version a reader found in
a real bundle — SYNTHESIS §4's builder note) is indexed as `pinned`, and
refused if it was published after D.

**Dependencies** are installed as of their dependent's publish time (step 3),
which is ≤ D. Their versions are therefore ≤ D as well.

**Popularity as of D (designed, not built in v1).** v1 takes an explicit
package list. A ranked list as of D would sum each candidate's downloads
over the year before D from
`api.npmjs.org/downloads/range/<D-365d>:<D>/<pkg>` (bulk for unscoped
packages, 128 per request; data starts 2015-01-10; ranges of at most 18
months) and record the list and the API response hashes in the manifest.
Known limit: the candidate pool has to come from somewhere — a list taken
today misses packages that were popular at D and have since faded.

**Bundlers as of D.** The builder runs the bundler binaries it is given.
For each, it reads the bundler's own packument (`bun`, `esbuild`) and
records in the manifest the version used, its publish time, and the
version that was newest at D; if the one used is newer than D the slice is
marked `bundlerAfterAsOf: true` and the build warns. Installing the as-of
bundler automatically is deferred.

**Known limits of as-of.**

- A version unpublished since D is not in today's `versions` list and
  cannot be installed: it is missing from an as-of index (the plan reports
  `unpublished`). npm forbids re-publishing a version number, so a version
  that IS installable is the same code it was at D.
- `npm install --before` needs the full packuments of every dependency
  (slower), and a package whose dependency had no matching version on its
  own publish day fails to install; the plan records the failure.
- Bundler behaviour: an index built with a newer bundler than the bundle's
  is recorded (`bundlerAfterAsOf`), not corrected. Within the Bun family
  the normaliser (exp095 step 3) is what makes versions agree.
- The registry `time` map is the publisher's clock. Mirrors can differ;
  the builder reads `registry.npmjs.org` through npm's own configuration.

## 4. On-disk format (`humanify-library-index/3`)

An index is a directory of two kinds of table: ROW tables (UTF-8 JSON or
JSON-lines, one row per line, rows sorted, keys in a fixed order, a
trailing newline) — provenance, read by instruments (the exp096 scorer
reads `units.<slice>.jsonl` and `versions.json`), never by a run — and
LOOKUP tables (binary, `humanify_core::libindex::table`), the only tables a
run reads.

```
<index>/
  manifest.json          what was built from what (below); its sha256 is the INDEX ID
  versions.json          {"<owner>": ["1.0.0", "1.1.0", ...]} — each owner's indexed versions, semver order
  modules.<slice>.jsonl  {"key", "owner", "files", "versions"}   one row per (key, owner)
  units.<slice>.jsonl    {"hash", "owner", "file", "nodes", "content", "moduleBytes", "versions"}
  literals.files.jsonl   {"owner", "file", "versions"}   literal file i (its literal SET is file i of literals.bin)
  packages.json          every owner the lookup tables hold, sorted: an owner id is its position
  units.<slice>.bin      lookup: unit hash (16 hex = u64) -> rows (owner id, smallest module bytes)
  literals.bin           lookup: literal -> package count, files; file -> owner id, literal ids
  stoplists.json         copied data (§7); its sha256 in the manifest
  families.json          copied data (§7); its sha256 in the manifest
```

**Lookup tables (since `/3`, exp103).** Sorted binary columns, opened by
reading a header and a FENCE (every 64th key) and nothing else; a lookup
reads one 512-byte block of keys and, on a hit, the rows it points at, by
positional reads (the OS page cache is the cache, shared by every run on
the machine). A literal key is FNV-1a of its string and every key hit is
confirmed against the stored string. Nothing is parsed into a map on load:
`/2` parsed every unit and literal row on load (2.3 s and ~1.3 GB per run
on the 577 MB step-4 index); `/3` opens the same index in milliseconds
with a resident set of ~1/64 of the key columns, so the run's cost no
longer grows with the index's size. The literal sets are stored ONLY in
`literals.bin` (`literals.jsonl`, 40% of a `/2` index, is gone); a unit
row's file, nodes, content and versions stay in the row table. The merge
writes both kinds from the same rows; the loader reads only `/3`.

**Version ranges.** `versions` is a list of `[from, to]` pairs over the
owner's indexed versions in `versions.json` order: `["1.0.0","1.2.0"]`
means every indexed version from 1.0.0 to 1.2.0 inclusive. A fingerprint
seen in 1.0.0, 1.1.0, 1.2.0 and 2.1.0, but not 2.0.0, is two ranges. Ranges
are over INDEXED versions, not all npm versions — a range says nothing about
a version the index never built; `versions.json` is there so a reader can
expand one. The lab measured why this matters: neighbouring patches share
~82% of whole-module hashes, and even at 4 sparse versions per package
merging versions keeps only ~63% of rows.

**Collisions are kept.** A key held by two owners is two rows. Dropping
multi-owner keys, the specificity rule and the agreement rule are LOOKUP
rules (§8), not build rules, so the index keeps the evidence they need.

**Manifest** (`manifest.json`):

```json
{
  "format": "humanify-library-index/3",
  "asOf": "2024-06-30T23:59:59.999Z",
  "fingerprints": {"module": "vendor-content-key/4", "literals": "literals/1"},
  "registry": {"earliestFetch": "...", "latestFetch": "...", "packuments": {"ms": "<sha256>", ...}},
  "slices": [{"slice": "bun", "bundler": "bun", "version": "1.4.0", "published": "...",
              "newestAtAsOf": "1.1.17", "bundlerAfterAsOf": true}],
  "packages": [{"name": "ms", "version": "2.1.3", "role": "latest", "published": "..."}],
  "refused": [{"name": "ms", "version": "3.0.0-canary.1", "reason": "prerelease"}],
  "shards": {"bun/ms@2.1.3": "<shard key>", "literals/ms@2.1.3": "<shard key>"},
  "tables": {"modules.bun.jsonl": "<sha256>", "literals.bin": "<sha256>", ...},
  "data": {"stoplists.json": "<sha256>", "families.json": "<sha256>"}
}
```

No build-machine time, path or host appears in any index file: the only
times are publish times and the snapshot's fetch times, both inputs.

## 5. Incremental updates and determinism

- **Shard key** = sha256 of `{shard format, kind, package, version, slice,
bundler@version, recipe version, fingerprint version}`. Every input that
  changes a shard's content is in its key; nothing else is. A package
  version never changes on npm, so a shard is valid forever for its key.
- **Update** = run `build` again with a new date or a longer list. Shards
  whose file exists with the same key are reused; new versions, a new
  bundler version or a new fingerprint version (e.g. the normaliser
  landing as `vendor-content-key/5`) build new shards. The merge is
  recomputed from shards (it is cheap: no parsing), so ranges are always
  right after an update — a new version inserted between two indexed ones
  can split a range, which is why ranges are never patched in place.
- **Determinism.** Rows are sorted; maps are ordered; version lists are in
  semver order; nothing reads the clock or the directory order. Tested:
  merging the same shards in another order gives the same bytes.
- **What is not deterministic, and is therefore recorded instead:** the
  registry snapshot (fetch time; packument hashes), the bundler binaries
  (name, version). npm's dependency resolution is pinned by `--before`.

## 6. Bundler slices as plugin pieces

A structural fingerprint depends on the bundler (Bun vs esbuild agree on
~60% of module hashes; SYNTHESIS §1), so the index has one module table per
**slice**. Literals come from source and are shared by every slice.

- **The slice is a value, `libindex::IndexSlice` (`Bun`, `Esbuild`)** — the
  only code that maps a slice to a bundler. On the pipeline side it becomes
  a toolchain piece (P18, "which index slice to query"), chosen once from
  the bundler verdict exactly like P17's `ModuleIdentitySource`; nothing
  downstream compares bundler names. A bundler with no slice (an unknown
  one) gets literal lookups only. exp105 (2026-10-14) added the webpack and
  rollup recipe bundlers (`webpack:target=node,format=cjs,minify`: webpack
  5 production + terser, one chunk; `rollup:target=node,format=cjs,minify`:
  rollup 4 + node-resolve + commonjs + json + terser); their bundles cut
  into units by their own module grammar on both sides.
- **The builder's recipe per slice** (`humanify-index`'s `recipe`): how to
  run that bundler (Bun: `bun build --target node --format cjs --minify
--sourcemap=external`; esbuild: `--bundle --platform=node --format=cjs
--minify --sourcemap=external`; webpack and rollup: the Node scripts in
  `crates/humanify-index/recipes/`, run as `node <script> <entry> <out>
  <format> [external…]` from `--webpack` / `--rollup`, because their
  recipes are configuration code). The recipe's version is part of the
  shard key.
- **Bun family vs esbuild:** one Bun slice covers Bun 1.2–1.4 once the
  normaliser lands (SYNTHESIS: 100% / 96.5% agreement after
  normalisation); until then, the bundler version is recorded and the
  measured gap applies. The normaliser is a per-slice piece too (Bun rules
  vs esbuild rules), versioned inside the module fingerprint version.

## 7. Evidence kinds

| Kind                                                  | In the index?                                               | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ----------------------------------------------------- | ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Whole-module key (CommonJS factory)                   | **yes** — `vendor_content_key`, slice tables                | The pipeline's own vendor content key (strings blurred, numbers exact, references to the rest of the bundle as slots). Not yet the exp095 normaliser (step 3); when it lands it is a new fingerprint version and shards rebuild.                                                                                                                                                                                                                                                       |
| Whole-module key, lazy ES modules (`__esm`)           | no — deferred                                               | Needs the boundary piece (marker + hoisted functions) the pipeline does not have yet.                                                                                                                                                                                                                                                                                                                                                                                                  |
| Literals from npm source, sponge files dropped        | **yes** — `literals.bin` (+ `literals.files.jsonl`)         | `S:` strings (template chunks, folded `"a"+"b"`), `P:` property names and export names, `N:` numbers by value, `R:` regexes; values over 120 UTF-16 units kept as a sha1 prefix; `0`, `1` dropped; `node:` spelling of builtins unified; `"undefined"` and `"u"` read as one (minifiers rewrite `typeof x=="undefined"` to `typeof x>"u"`).                                                                                                                                            |
| Sub-tree units (statement + every function inside it) | **yes** — `units.<slice>.bin` (+ `.jsonl`), stamp `units/3` | Built (exp095 → exp097 step 1): `libindex::units` prints the whole statement unit and every function inside it with the `LIBRARY_UNITS` canonical hash, a node count and a content-token count; the builder writes no row under `UNIT_ROW_MIN_NODES` (20). The lookup's own floors (`min_content` 3) and the specificity rule (an index module ≤ 4× the query + 4 KB) sit in `libindex::lookup`. `units/3` re-fingerprinted lazy-body units (`units/2` had reused stale esbuild rows). |
| Shingles / MinHash                                    | deferred                                                    | Raw shingles are the size problem (246 MB at top-2,000); MinHash was not measured.                                                                                                                                                                                                                                                                                                                                                                                                     |

**Stop-lists and package families are data files** shipped with the index
(`crates/humanify-index/data/stoplists.json`, and `families.json`, which since
library discovery lives in `crates/humanify-core/src/libindex/` because the
lookup reads it too; both copied into
every index and hashed in its manifest): the helper packages (tslib,
`@babel/runtime*`, `@babel/helpers`, `@swc/helpers`, regenerator-runtime,
`@oxc-project/runtime`) are dropped from the index at merge time; the
families (the SYNTHESIS §4 five plus the scale lane's data-driven rule:
same npm scope, and named scoped forks) are read at lookup time. The
Stainless and es-shims template rules are file patterns kept in the
stop-list file for the lookup.

## 8. How the pipeline will consume it (not built)

- **Flag:** `--library-index <dir>`. The run reads `manifest.json`,
  refuses an unknown `format`, and refuses an index whose fingerprint
  versions differ from its own (`hash::forms::VENDOR_CONTENT.stamp()`,
  `literals/<LIBRARY_LITERALS_VERSION>`, `LIBRARY_UNITS.stamp()`) — the same all-or-nothing rule as the vendor
  record's `hashVersion`, because a half-matching index would silently
  half-name.
- **Lookup:** the toolchain's slice picks `modules.<slice>.jsonl`; the
  vendor stage computes `bundle_content_keys` for its modules (it already
  does for the content carry) and `literals(Bundled)` for each. A name needs
  the scale lane's design rule: structural AND literal agreement, or either
  AND a declared reader; multi-owner keys refused; the specificity rule;
  stop-lists; families decide what counts as the same package.
- **Where it sits in vendor naming.** The order is: the exact-hash carry
  from the prior FIRST (an unchanged module keeps its prior name whatever
  the index says), then the declared-identity pair, the content pairing,
  the declared name (P17), then the index alongside the model, and
  `lib_<hash>` last. So an index can only name modules whose content
  changed or that are new — and those show in a cross-version diff anyway.
  An index update cannot rename an unchanged module, so the two sides of a
  diff do NOT need to use the same index.
- **Pinning is for reproducibility.** An index is immutable and
  content-addressed (its id is the manifest's sha256; an update is a new
  directory with a new id). The run writes the id and `asOf` into its
  stats, its diagnostics and the vendor record, and every index-named
  module records `nameSource: "library-index"`, so re-running with the
  same index gives the same result and a reader can tell which index
  produced a name.
- **As-of keeps future versions out of old bundles.** Processing a bundle
  from date X with an index as of a later date can name a module with a
  version that did not exist at X. Use an index as of the bundle's own
  release date (it can be read from a declared VERSION or banner, or be
  given).

## 9. v1 scope

**Built:** the crate and binary; the registry snapshot reader/fetcher; the
as-of version policy with refusal reasons; `--dry-run` plans; npm install
with `--before`; the Bun and esbuild recipes; sourcemap attribution;
module and literal shards with content keys; the merge into ranges; the
manifest with the index id; stop-list/family data files; the literal
extractor in core; `bundle_content_keys` in core.

**Deferred:** popularity as of D; installing the as-of bundler
automatically; lazy-ESM module keys; sub-tree units (since built: §7,
`units/3`); shingles/MinHash; the
normaliser (exp095 step 3); every lookup rule and the pipeline flag (§8);
parallel builds beyond `--jobs 4`; compression of the tables.

**Tests** (beside their modules): `crates/humanify-index/src/asof_test.rs`
(from a recorded `ms` packument, no network), `shard_test.rs`,
`merge_test.rs`, `sourcemap_test.rs`, `recipe_test.rs`, `build_test.rs`
(the plan, offline, no subprocess);
`crates/humanify-core/src/libindex/literals_test.rs`, `format_test.rs`;
`modules/vendor_content/vendor_content_test.rs`'s three bundle-key tests.

**Smoke (2026-10-11, 8 small packages, `--as-of 2022-06-30`):** 29
versions, 87 shards (literals + Bun + esbuild), 107 / 93 module rows and
201 literal rows; merging versions into ranges keeps 63–66% of the rows
(the lab measured ~63%). A rebuild from the same store built 0 shards and
wrote byte-identical files. `--as-of 2020-12-01` refuses `ms@2.1.3`
(published 2020-12-08) and indexes 2.1.2 / 2.1.1 / 1.0.0 / 0.7.3; the plan
also caught a real unpublished version (`debug@4.4.2`). ESM-only packages
(escape-string-regexp 5, uuid 8 under esbuild) give no module rows: lazy-ESM
keys are deferred.

## 10. The recommended default index (exp109, 2026-10-14)

An index is data, not code: it is never committed, and nothing in a run
picks one unless `--library-index` names it. The recommendation below is
what `library.matching = "report"` should be pointed at today, and what the
eval's library score (`npm run eval -- library`) is judged on.

**Recommended now: `k2-l12m4`** (exp108; `/work/exp108-data/idx/ix/k2-l12m4`,
/3, 4.65 GB, 0.67 s and ~2 GB per lookup run) — step 4's roots and pins plus
every package bundled by at least two of 2,155 independent training apps,
at the `year-last12+major4` density, in three slices: Bun target bun, Bun
target node, esbuild. With the shipped config (§ `library-matching-config.md`)
it beats exp101's `c2-l12m4` on the dev held-out apps (F0.5 90.2 vs 85.0,
precision 99.1 vs 99.3) and on the frozen test split's Bun and esbuild sets
(74.1 vs 66.6, 79.4 vs 68.4, precision up on both).

**Webpack and rollup bundles: `step4-wr`** (`/work/exp103-data/ix/step4-wr-v3`,
exp105) until k2's own webpack/rollup slices exist. An index refuses a
bundle whose bundler it has no slice for, so a run on a webpack or rollup
bundle needs this index today.

**Next build: `k2-all`** — k2's package set across all five slices. A
`c2-all` build (exp101's smaller c2 set, five slices) was started by exp109
and stopped at ~43% when k2's numbers arrived; its store
(`/work/exp109-data/store`: ~12.6k new webpack/rollup shards, ~15.7k
five-recipe literals shards) and seeded work dir are the starting point for
k2-all, which reuses every shard whose key matches.

**How to build one** (the builder is `crates/humanify-index`; every step
runs offline against a registry snapshot except npm installs):

1. **Package set.** A roots list and a `--pinned` list (exp101's
   `scripts/tail_pins.py` picks the `year-last12+major4` versions of new
   roots from the registry snapshot as of the index date).
2. **Reuse what exists.** Point `--store` at a store holding earlier shards
   (hard-link copies: `cp -al <old store>/shards/<slice> <new store>/shards/`;
   the builder writes every shard with write-then-rename, so a link is
   never written through). A shard is reused only when its key matches:
   package, version, recipe, bundler VERSION, fingerprint versions — so pass
   the same bundler binaries (exp101/105: Bun 1.4.0, esbuild 0.28.2,
   webpack 5.111.1, rollup 4.64.5). The `literals` shard's key holds the
   whole recipe SET, so adding a slice rebuilds every literals shard; it
   reads the slices' builds from `--work`, so seed the work dir from earlier
   builds (`experiments/109-default-config/scripts/seed-work.sh`: installs
   hard-linked, finished builds copied).
3. **Build in short-lived processes.** A `humanify-index build` process
   grows ~40 MB per version it builds; run many small ones
   (`experiments/108-app-corpus/scripts/build_chunks.py`, ~100 pins per
   chunk), then one `build ... --merge-only` (the streaming merge, §4).
   Under the campaign, every build holds `flock /work/index-build.lock` and
   at most 30 cores.
4. **Check it.** `--dry-run` prints the plan (shards to build vs reuse);
   the merged index's id is its manifest's sha256.

`experiments/109-default-config/scripts/build-c2-all.sh` is the c2-all
build as one command (point it at k2's roots and pins for k2-all). Since
exp111 (mimalloc) the builder's memory is flat, so step 3's short-lived
chunks are optional.
