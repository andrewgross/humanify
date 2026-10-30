# exp161 — the fresh 2-version hop with the FULL names index (plan §7 closing lane)

STATUS: RUNNING (2026-10-29). Branch `exp161-names-hop` off main `324ebe607`.
No code change: this lane BUILDS the full names-stamped library index (the
standing full index `53a1d874…` has call edges but NO names tables, so
restoration is inert on it), then records the fresh 2.1.215→2.1.216 hop in
names mode against it and in default mode for contrast, and reads the
line-diff breakdown off the two trees.

## The names index build

Same pins as exp152 verbatim (`/work/exp152-data/pins.txt`, 66,523 lines,
sha256 `7fc74f75…`), same recipes (bun:target=bun and bun:target=node,
format=cjs, minify, production; `--sponge-regions origin`), Bun 1.4.0 at
`/work/tools/bun-1.4.0`, plus `--names`. exp152's store and work dir were
REUSED through a hard-linked copy (`cp -al`, /work/exp161-data/store): the
shard path does not carry the module shard key, so a names-stamped build
writes its `+unit-names/1` shards to the same `store/shards/<kind>/<pkg>@<ver>.json`
paths as the plain builds — a shared store would have collided, and the
hard-link copy keeps exp152's plain index intact while sharing every shard
the names pass does not rebuild.

The shard-key rule, verified BEFORE burning the hours: `+names/1` changes
the MODULE shard key only; the names pass is a per-version derivation that
re-reads the cached builds (it does NOT re-bundle). One chunk of 60 versions
(pilot) ran first and its shard phase matched the plain builds' timings;
then the full shard phase ran with the exp152 chunk driver under
`flock /work/index-build.lock` + `flock /work/heavy.lock` per chunk.

- Shard phase: 66,523 versions, 1,109 chunks, 1,106 clean, 3 chunks killed
  (-9) by memory — the pinyin chunks (chunks 1011–1013, each ~60–65 GB peak);
  6.39 h wall (~10,400 versions/h). Every version's names-shard is in the
  store except pinyin@2.11.2/3.1.0/4.0.0 on the slices that did not finish;
  retried solo at `--jobs 1` before the merge.
- Merge attempt 1 (the exp152 merger, names tables buffered): **OOM, killed
  at 156.61 GB peak RSS after 28,372 s (7.9 h)**, no index produced. This is
  structural, not environmental: the full corpus is 131,686 module shards of
  ~121 GB of raw names JSON, and the merger held the names three times over
  with heap Strings (the group keys as `Vec<RestoredName>`, the `name_rows`
  Vec, and `NameTable::encode`'s `by_row` clone) while also buffering every
  names jsonl table in RAM. exp155's plain merge reference peaked at
  116.7 GB / 3,411 s — a plain retry would have died the same death.
- Merge attempt 2 (this branch's redesign): group names interned as
  `(slot, interner id, kind ordinal)` triples (no `RestoredName` Strings at
  rest), the names jsonl STREAMED to a staging file in the output dir with an
  incremental sha256 into the manifest, and `NameTable::encode` fed compact
  rows (`NameRowCompact { row, lists: Vec<Vec<RawName>> }`). Ordering
  invariants unchanged: unit rows still sorted (hash, owner, file), row ids
  still hash-order, lists still sorted by `RestoredName`'s Ord, strings
  still sorted. Proven byte-identical on the pilot: a 60-package replay
  (`--packages empty --pinned pilot60.pins`, PinsOnly) reproduced the pilot
  manifest `94e1839c…` byte-for-byte with BOTH the pre-change and the
  post-change binaries (`diff -r` clean), covering both the buffered and the
  streamed paths.
- Result (TBD: id, times, size, coverage).

## The merge fix (the branch's only code change)

`crates/humanify-index/src/merge.rs` — the interned-compact-streamed merge;
`crates/humanify-core/src/libindex/table.rs` — `NameRowCompact`,
`compact_name_rows` (the test adapter), `names_lookup_files`, and
`NameTable::encode` rewritten over compact rows; `lookup.rs` and the tests
routed through the adapter; `build.rs` passes `staging = out`.

## The hop (TBD)

## The line-diff breakdown (TBD)

## Restoration's measured effect (TBD)

## D1/D2 shape (TBD)

## Gate (TBD)
