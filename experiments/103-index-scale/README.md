# exp103: index scale — a library index 10–50× larger has to stay cheap to load and to build

## STATUS (2026-10-14)

- **State.** DONE. Branch `exp103-index-scale` @ `4a356b16` (main `5a04e816` merged in), not merged, not
  pushed. Two changes, each byte-identical in output:
  1. **Index format `humanify-library-index/3`**: binary lookup tables a run reads on demand (no parse on load).
  2. **A streaming merge** in `humanify-index` (shards read one at a time, strings interned), plus
     `build --merge-only`.
- **Why.** Most library misses are "not indexed" (72% of held-out library bytes). The fix is a much larger
  index, and the `/2` format cost every run about 1.9× the index's size in RAM and ~4 s per GB to load. The
  merge cost ~60× the index's size in RAM (112 GB for exp101's 1.8 GB index).

### Totals first

| index (format `/2` size)                                   | run load, `/2` → `/3` (wall, peak RSS; tiny bundle, agreement rule) | `/3` size | merge peak RSS, old → streaming        |
| ---------------------------------------------------------- | ------------------------------------------------------------------- | --------- | -------------------------------------- |
| step 4 (577 MB, 2,989 owners)                              | 2.3–2.7 s, 1,246–1,296 MB → **0.04 s, 56 MB**                       | 538 MB    | (not measured) → 3.5 GB, 122 s         |
| exp101 c2-l12m4 (1.82 GB, 5,263 owners)                    | 7.2–8.1 s, 3,528 MB → **0.07 s, 72 MB**                             | 1.73 GB   | **112 GB → 10.2 GB**, 272 s            |
| exp101 call2k-l12m4 (3.04 GB)                              | 12.5–13.2 s, 5,773 MB → **0.10 s, 106 MB**                          | 2.89 GB   | not run (its merge used main's binary) |
| synthetic step 4 ×10, one slice (1.32 GB of lookup tables) | — → 0.05 s, 62 MB                                                   | 1.33 GB   | —                                      |

The whole exp096 score on idle cores, 70 bundles, 4 at a time (`npm run eval -- library`, main `5a04e816` vs
this branch; per-run walls summed, peak RSS per run):

| index, rule                 | wall (sum of runs)                         | peak RSS mean / min / max                           |
| --------------------------- | ------------------------------------------ | --------------------------------------------------- |
| step 4, structural          | 282 s → **138 s**                          | 1,314 / 1,246 / 2,413 MB → **421 / 52 / 2,044 MB**  |
| step 4, agreement           | 334 s → **198 s**                          | 1,313 / 1,246 / 2,445 MB → **437 / 52 / 2,048 MB**  |
| call2k (3.0 GB), structural | 970 s → **137 s** (pool wall 246 s → 46 s) | 5,780 / 5,761 / 5,891 MB → **444 / 104 / 2,058 MB** |

What is left of a large bundle's RSS (≈ 2 GB for movieprint) is the bundle's own processing, not the index.

### Byte-identity proofs

| check                                                                                  | result                                                                                                                                      |
| -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| exp096 reports, step 4, main `5a04e816` (`/2`) vs `4a356b16` (`/3`), structural        | **70/70 identical** except the index id: 87,711 claims, 654 vetoes, 350,327 unit rows                                                       |
| same, agreement                                                                        | **70/70 identical**: 49,517 claims, 290 vetoes, 350,327 unit rows                                                                           |
| same, exp101 call2k-l12m4 (3.0 GB), structural                                         | **70/70 identical**: 128,071 claims, 926 vetoes                                                                                             |
| earlier: `e9b8c0f0` vs `d99fc1e8` / `c2e0e84e`, both rules, step 4                     | 70/70 identical, every scorer cell identical (leaderboard)                                                                                  |
| streaming merge (`--merge-only`) vs main's merge + the `/2→/3` converter, step 4 store | **every table file byte-identical**; manifest identical but for the refused reason of versions with no shard (`no-shard` vs `build-failed`) |
| same, exp101 c2-l12m4 store (124,300 shards)                                           | **every table file byte-identical**; manifest the same way                                                                                  |
| `npm run check`                                                                        | ALL 13 STAGES PASSED (each commit; last at `4a356b16`)                                                                                      |
| warm neutrality `e9b8c0f0` vs `c2e0e84e`, 2.1.215→2.1.216                              | **NEUTRAL**: 0 files, 0 lines, cache +0 / +0, exit 0 / 0                                                                                    |
| warm neutrality `5a04e816` (main) vs `4a356b16` (this branch), 2.1.215→2.1.216         | **NEUTRAL**: 0 files, 0 lines, cache +0 / +0, exit 0 / 0                                                                                    |

### Recommendation

Merge. Then rebuild (or convert) the indexes the campaign uses: the loader reads `/3` only. Every `/2`
index converts in under a minute with `experiments/103-index-scale/convert` (call2k: 53 s, 13 GB RSS); the
streaming merge writes `/3` directly. With both changes a 10× index is routine, and a 50× index is
practical to load. Building a 50× index still needs a leaner merge (next lever, below).

## 1. Profile (step 4, `/2`)

- **Bytes by table**: `literals.jsonl` 229 MB (40%), `units.<slice>.jsonl` 3 × ~100 MB (52%), `modules.*`
  35 MB, `manifest.json` 13 MB.
- **Load by table** (a run on a 183-byte bundle, which loads the index and does nothing else): whole index 2.3 s /
  1,296 MB. Without the literal table: 0.7 s / 481 MB. Without the unit tables: 1.7 s / 998 MB. So the literal
  table is ~1.6 s and ~0.8 GB of every run, and one unit slice ~0.6 s and ~0.3–0.5 GB. That is ~1.25 GB of the
  1.25–2.4 GB peak of every exp096 run (the smallest runs' floor was the index).
- **What the rules read.** The structural rule, specificity and containment read only the chosen slice's
  unit rows, and only `(hash, owner, moduleBytes)` of each. The agreement rule (and exp102's literal
  guard) also read each literal file's owner and literal set, and per literal the number of packages holding
  it. Nothing in a run reads `modules.*`, `versions.json`, a unit row's `file`/`nodes`/`content`/`versions`,
  or a literal row's `file`/`versions`. The exp096 scorer reads `units.<slice>.jsonl` and `versions.json`.
- **Rows**: 647,596 unit rows (510,372 distinct hashes) per slice, 130,966 literal files holding 12.1 M
  literal occurrences of 623,804 distinct literals (13.5 MB of text). The `/2` loader turned them into
  `HashMap<String, Vec<UnitHit>>`, `HashMap<String, u32>`, and per-literal and per-file `Vec<u32>`s.

## 2. What changed

**Format `/3` (docs/design/library-index.md §4).** The merge writes, beside the row tables:

- `packages.json`: the owners, sorted. An owner id is its position.
- `units.<slice>.bin`: sorted distinct unit hashes (16 hex = `u64`), row offsets, rows `(owner u32,
moduleBytes u64)` in row order.
- `literals.bin`: literal keys (FNV-1a, each hit confirmed against the stored string), per literal its
  package count, string offset and postings offset; per file its owner and literal-id offset; then the
  strings, the postings and each file's sorted literal ids.

`literals.jsonl` is gone. Its sets live only in `literals.bin`; the owner, path and versions are in
`literals.files.jsonl`, row for row. A run reads the header and a **fence** (every 64th key) of each table.
A lookup reads one 512-byte key block and, on a hit, its rows, through a cache of the 4 KB pages this run
has touched. The cache is bounded by the bundle's vocabulary, never by the table. Core forbids `unsafe`, so
this is positional reads, not mmap. The OS page cache is shared by every concurrent run.

- **The first version (plain `pread` per lookup, `d99fc1e8`) made the agreement rule system-call bound.**
  task-master-ai went 18 s → 122 s. Adding the page cache (`c2e0e84e`) made every bundle faster than `/2`
  (table below).
- **Owner ids are now name order, not first-appearance order.** Only one tie-break reads an id: equal
  literal scores between two packages. Such a tie fails the margin rule (`literal_margin` > 0), so it cannot
  change a claim at any margin above zero. The 70/70 identity is the empirical proof at the default.
- **The `ReferenceStore::candidates` trait** returns `Cow<[Piece]>`. The prior store still borrows; the index
  store returns the rows it read. This is the only default-path code change, covered by both neutrality runs.

**Streaming merge.** `Merger::add(shard)` takes shards in (label, key) order and refuses one out of order: a
unit group keeps the first shard's nodes and content, so order is part of the bytes. `build` reads the store
shard by shard into it. Strings (owner, version, file, module key, literal) are interned to `u32`, and unit
hashes are kept as `u64`. The old merge held every shard, unmerged, as `String`s, and cloned the vector.
`--merge-only` merges what the store holds and never retries failed builds. A full re-merge of step 4 had
6,036 failed shards, which `build` would have re-attempted.

**Per-bundle timing, idle-ish box, 3 alternating reps, median** (`ab.sh`; `/2` = main, `/3` = this branch):

| bundle                   | rule                   | `/2` wall     | `/3` wall     | `/2` MB | `/3` MB     |
| ------------------------ | ---------------------- | ------------- | ------------- | ------- | ----------- |
| twilio-cli (183 B)       | both                   | 2.6–2.7 s     | 0.05 s        | 1,250   | 56          |
| mailspring               | structural / agreement | 3.35 / 3.19   | 0.67 / 0.64   | 1,258   | 204–214     |
| wrangler                 | structural / agreement | 6.11 / 6.34   | 2.91 / 3.03   | 1,458   | 972         |
| task-master-ai (esbuild) | structural / agreement | 9.18 / 27.95  | 6.38 / 25.24  | 1,320   | 958–1,012   |
| Claude Code 2.1.216      | structural / agreement | 14.28 / 23.46 | 8.63 / 20.38  | 1,419   | 1,129–1,241 |
| movieprint (35 MB)       | structural / agreement | 17.04 / 16.44 | 12.85 / 12.39 | 2,445   | 2,053       |

## 3. Projected cost at 10× and 50× (per run, and per build)

Measured scaling of the `/2` load across three real indexes: RSS ≈ **1.9 × index bytes**, load time ≈ **4.1 s
per GB** (577 MB: 1.25 GB / 2.3 s; 1.82 GB: 3.5 GB / 7.2 s; 3.04 GB: 5.8 GB / 12.5 s). Under `/3`, RSS grows by
~17 MB per GB of index (56 → 72 → 106 MB) and load stays under 0.1 s. Package counts scale index bytes
unevenly: c2 has 1.76× step 4's owners and 3.2× its bytes. So the projection is per index byte, with "10×" =
10 × step 4's bytes.

|                                   | 1× (577 MB)                   | 10× (~5.8 GB)                                       | 50× (~29 GB)                             |
| --------------------------------- | ----------------------------- | --------------------------------------------------- | ---------------------------------------- |
| `/2` run: load wall, RSS          | 2.3 s, 1.3 GB                 | ~24 s, ~11 GB                                       | ~120 s, ~55 GB (4 jobs: 220 GB)          |
| `/3` run: load wall, RSS          | 0.04 s, 56 MB                 | <0.2 s, ~150 MB                                     | <0.5 s, ~0.5 GB (packages.json + fences) |
| `/3` per-lookup cost              | 1–3 page reads, cached        | same (+1 key block per 64×: log-depth fence search) | same                                     |
| `/3` on disk                      | 538 MB (177 MB lookup tables) | ~5.4 GB                                             | ~27 GB                                   |
| old merge RSS (≈ 60 × index)      | ~35 GB                        | ~350 GB (> the box's 247 GB)                        | infeasible                               |
| streaming merge RSS (≈ 6 × index) | 3.5 GB                        | ~33 GB                                              | ~170 GB                                  |

The synthetic ×10 (step 4's bun-node slice and literal table ×10; half of each copy's literals
copy-specific) loads in 0.05 s at 62 MB. Its 1.32 GB of lookup tables cost no more per run than 1×.

**What still grows linearly** is the merge's working set, about 6 bytes of RAM per byte of index. The heavy
part is the literal groups (every distinct (owner, file, literal set) with its version list), followed by
`literal_columns` at the end. The next lever for 50× is to merge one slice at a time and to spill sorted runs
of literal groups to disk (an external sort). The output order is already fixed, so this can be done without
moving a byte. Disk is ~0.93 × the `/2` size. Beyond `literals.jsonl`, the bulk is now the
`units.<slice>.jsonl` provenance (≈ 55%), which only the scorer reads. Gzipping them (and teaching
`mmlib.Index` to read `.gz`) would halve the index on disk if that ever matters.

## 4. Reproduce

```bash
D=/work/exp103-data
# /2 -> /3 (lab converter, outside the workspace; the crates read and write /3 only)
cd experiments/103-index-scale/convert && CARGO_TARGET_DIR=$D/convert-target cargo build --release --offline
flock /work/index-build.lock $D/convert-target/release/exp103-convert <index /2 dir> <new /3 dir>
# streaming re-merge of an existing store, no builds retried
humanify-index build <the original build's flags> --offline --merge-only --out <dir>
# synthetic k-times index for cost only (claims meaningless)
$D/convert-target/release/exp103-scale <index /2 dir> <its /3 dir> bun-node-cjs-min 10 <out>
# score / identity
npm run eval -- library <label> --index <dir> --rule structural|agreement --commit <sha>
python3 $D/identity.py <label a> <label b>     # every report equal after dropping indexId
python3 $D/cmpix.py <index a> <index b>        # table files by sha256, manifest modulo refused reasons
```

Converted indexes: `/work/exp103-data/ix/{step4-v3, c2-l12m4-v3, call2k-l12m4-v3, synth-x10}`;
re-merged: `ix/{step4-remerge, c2-remerge}`. Logs: `/work/exp103-data/logs/`. The data is not committed.

## 5. Known limits

- `experiments/096-library-matching-score/merge_index.py` (a lab union of two indexes) reads
  `literals.jsonl` and does not work on `/3`. It is not part of any living instrument.
- A `/3` index loaded by a run from a file that changes under the run fails loudly (a panic naming the
  read). The tables' sizes are checked against their headers at open. Indexes are immutable by design
  (§5 of the design doc).
- The page cache in the run is a `RefCell`: the lookup runs on one thread. A parallel lookup would need a
  lock, which the concurrency census would flag.
