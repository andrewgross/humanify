# exp131: a slimmer index build cache (sourcemaps without `sourcesContent`)

## STATUS (2026-10-17)

DONE on branch `exp131-cache-slim` (off main `a72cce9f`). Builder-only change (`crates/humanify-index`): no
pipeline code, so no neutrality run is needed. The bare gate passed all 13 stages before each commit.

- **TOTAL freed: 815.3 GB** (df on `/work`: 800.66 GB free -> 1,615.97 GB free; used 6,736.5 -> 5,921.2 GB),
  33 minutes on 16 cores. Build-cache maps 1,140.7 GB -> 325.3 GB (-71.5%) over 817,537 maps (one per inode
  across the four work dirs), 0 errors.
- **The index is byte-identical.** 1,000 sampled versions (6,000 shards) re-fingerprinted from their cached builds
  before and after the strip: all 22 index tables AND `manifest.json` (so the index id) byte-identical, in every
  pairing of three legs (main's builder before, this branch's builder after, main's builder after). The shards
  are byte-identical except 22-31 per pair, which differ ONLY in a failed build's error text (exp111's known
  nondeterminism: a failed build is retried by every leg, and esbuild's first error line varies; e.g.
  `zrender@5.6.1` reports `Expected ";"` in one leg and `No loader is configured for ".md"` in the other).
- **New builds write no `sourcesContent`** (esbuild `--sources-content=false`, rollup `sourcemapExcludeSources:
true`, webpack `devtool: "nosources-source-map"`, and every finished build's map slimmed before it is stamped,
  because Bun has no flag). 60 versions built FRESH by main's builder + recipes and by this branch's: every
  `out.js` and build stamp byte-identical, every map identical once `sourcesContent` is removed, the index
  tables and manifest byte-identical. Maps 338 MB -> 112 MB (-67%).
- **req/dyn dedup: NOT APPLICABLE.** 0 of 676 sampled req/dyn pairs are byte-identical: each entry carries its
  own app wrapper (`labAppMain_req` vs `labAppMain_dyn`, `require` vs `import()`), so the outputs always differ.
  No alias was implemented.
- REMAINING: other build caches on `/work` still hold full maps and were NOT stripped (outside this lane's
  scope): `/work/exp095-index-v2-data/{final,selA,selABC,smoke}/work`, `/work/exp105-data/work`,
  `/work/exp108-data/idx/work`, `/work/exp111-data/work`, `/work/exp118-data/idx/work`. Any of their files
  hard-linked into the four stripped dirs were slimmed through the shared inode. The deployed recipe scripts
  (`/work/exp105-data/tools/*-build.*`) are copies of main's: copy the new ones there (or point `--webpack` /
  `--rollup` at copies of this branch's) — even without that, the post-build slim leaves no `sourcesContent`.

## 1. Why it is safe

The builder reads a map through `SourceMap::parse` (`crates/humanify-index/src/sourcemap.rs`): `sourceRoot`,
`sources` and `mappings`, nothing else. `sourcesContent` (every source file, embedded) was ~72% of each map's
bytes and ~half of a version's build outputs. Nothing in the harness reads an INDEX work dir's maps for their
content (the truth layers read the frozen app corpora's maps).

The recipe version was NOT bumped. It is in every shard key, and a bump would have made every cached build
stale; the rule it protects ("a rule that changes what the builder reads changes the key") is not touched by a
byte the builder never reads. §3 is the proof.

## 2. The tool: `humanify-index slim-cache`

```
taskset -c 48-63 humanify-index slim-cache --work <dir> [--work <dir>...] --journal <dir>
```

- Walks every `<install>/build/**/*.map` of each work dir (never `node_modules`, never `entries/`), one path per
  (device, inode) across all dirs, so a map hard-linked into four work dirs is rewritten ONCE.
- Cuts the top-level `sourcesContent` member out of the TEXT (every other byte kept, nothing re-serialised),
  then parses old and new with the builder's own `SourceMap::parse` and refuses to write unless `sources` and
  every decoded segment are equal.
- Rewrites IN PLACE through the inode (open for write, write, truncate, mtime restored), so every hard link
  sees the slim file and the links stay shared. Symlinks are never followed or replaced.
- Journals each rewrite first (`<journal>/<dev>-<ino>`: path + new text, committed by rename, fsynced), so a
  run killed mid-write is finished by the next run before it walks; a not-yet-committed `.tmp` entry means the
  map was never touched.
- A map that is not UTF-8 or not a parseable map is left byte-identical and listed (the builder reads it exactly
  as before). Idempotent: a second pass finds nothing to cut.
- Parallel on the core's `par::map_ordered`, bounded by the CPU affinity (`taskset`).

Tests (`slim_test.rs`, `recipe_test.rs`): cut position first/middle/last/only with quotes, escapes, brackets and
commas inside the cut strings; nested `sourcesContent` and a `names` entry spelled `sourcesContent` untouched;
malformed text refused; a hard-linked map rewritten once with inode, link count and mtime kept and a symlink
still a symlink; an unreadable map untouched; journal recovery; the walk's scope (build outputs only, one path
per inode, symlinked dirs not followed); the esbuild flag and both scripts' options; a finished build (a stand-in
bundler that embeds sources) is stamped without `sourcesContent`.

## 3. The proof

**Sample.** 1,000 versions drawn (seed 131) from k2-all's pins that have builds in `/work/exp130-data/work` (the
work dir the shipped `k2-all-r5-origin` was re-fingerprinted from). Their `build/` dirs were COPIED (new
inodes, so the originals were untouched until §4) into `/work/exp131-data/work`, the same depth as the
original, with their `node_modules` symlinks kept: 27 GB, 10,061 maps.

Three legs, each re-fingerprinting every shard (empty store: 6 shards x 1,000 versions, literals included) from
the cached builds, with exp130's full-build configuration (5 recipes, `--sponge-regions origin`, as-of
2026-10-11, `year-all+minor`), `build_chunks.py` 4 x 4 jobs on cores 48-63, under the build lock:

| leg   | builder         | maps                                                                                          | shard wall |
| ----- | --------------- | --------------------------------------------------------------------------------------------- | ---------- |
| A     | main `a72cce9f` | full                                                                                          | 1,948 s    |
| strip | this branch     | 18.95 GB -> 5.21 GB (-72.5%), 10,057 stripped, 4 not UTF-8 left alone (`imap@0.6.2` Bun maps) |            |
| B     | this branch     | slim                                                                                          | 1,515 s    |
| C     | main `a72cce9f` | slim                                                                                          | 2,508 s    |

(wall time is one slow chunk's and swings by leg: no speed claim is made.)

| pair   | index tables (22) | manifest.json | shards byte-identical | differ only in failed-build error text |
| ------ | ----------------- | ------------- | --------------------- | -------------------------------------- |
| A vs B | identical         | identical     | 5,978 / 6,000         | 22                                     |
| A vs C | identical         | identical     | 5,969 / 6,000         | 31                                     |
| B vs C | identical         | identical     | 5,970 / 6,000         | 30                                     |

The strip changed no non-map file (sha256 of every other build file identical before and after). The varying
notes-only count is the failed builds' retry, not the strip: B and C read the SAME stripped files and differ in 30.

**Fresh builds.** 60 smaller versions of the sample, installs reused, no cached build: main's builder + main's
recipe scripts vs this branch's builder + its scripts. 1,848 build files each; 0 non-map files differ; 591 old
maps carry `sourcesContent`, 0 new ones do, and every map is equal once it is removed; tables and manifest
identical; 357/360 shards byte-identical, 3 differing only in failed-build error text.

## 4. The full strip

`scripts/strip-full.sh` under `flock /work/index-build.lock` (refuses to start while any builder or bundler
process runs; the three exp109 orphan webpack processes ignored), cores 48-63, over
`/work/exp124-data/work`, `/work/exp113-data/work`, `/work/exp101-data/work`, `/work/exp130-data/work`
(`/work/exp130-data/full/work` and `/work/exp124-data/full/work` are symlinks to two of these).

|                                                  |                                                                             |
| ------------------------------------------------ | --------------------------------------------------------------------------- |
| df `/work` used, before -> after                 | 6,736,549,064,704 -> 5,921,237,291,008 bytes (-815.3 GB)                    |
| df `/work` free, before -> after                 | 800,659,775,488 -> 1,615,971,549,184 bytes                                  |
| wall                                             | 18:20:44 -> 18:53:39 UTC (33 min), cores 48-63                              |
| maps walked (one per inode, 4 dirs)              | 817,537                                                                     |
| stripped                                         | 817,491: 1,140,694,632,838 -> 325,332,246,262 bytes                         |
| already without `sourcesContent`                 | 8                                                                           |
| not UTF-8, left byte-identical                   | 38 (all `imap@*` Bun maps; the builder's `read_to_string` refuses them too) |
| errors / journal entries left / symlinks skipped | 0 / 0 / 0                                                                   |

Checks after the strip:

- symlinks: 72,324 install-level symlinks in each of `/work/exp124-data/work` and `/work/exp130-data/work`
  (their `node_modules` -> exp113's), 0 broken; exp113 and exp101 have none; the two `full/work` symlinks intact.
- 600 random installs (150 per dir): 5,027 build maps, all parse, none still carries `sourcesContent`, no mtime
  changed (restored after each rewrite); `node_modules` never walked: 40 of 401 published package maps there
  still carry their `sourcesContent`.
- per version after: `build/` ~11 MB (exp113, 300 random installs; `node_modules` ~11 MB beside it) and ~15 MB
  (exp124, 300 random). The 1,000-version sample's `build/` went 27 MB -> 13 MB per version (maps 18.9 -> 5.2 GB).

## Reproduce

```bash
S=experiments/131-cache-slim/scripts
D=/work/exp131-data   # bin/ holds main's and this branch's humanify-index; sample/pins.txt the 1,000 versions
# copy the sample's builds, then (under the lock) legs A, strip, B, C:
$D/sample/copy.sh $D/sample/pins.txt
flock /work/index-build.lock $D/sample/proof.sh
python3 -I $S/compare.py $D/legA $D/legB sample      # also legA legC, legB legC
flock /work/index-build.lock $D/fresh/fresh.sh && python3 -I $S/compare.py $D/fresh/old $D/fresh/new fresh
flock /work/index-build.lock $S/strip-full.sh <humanify-index> $D/full /work/exp124-data/work /work/exp113-data/work \
  /work/exp101-data/work /work/exp130-data/work
```
