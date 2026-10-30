# exp161 — the fresh 2-version hop with the FULL names index (plan §7 closing lane)

STATUS: LANDED (2026-10-30). Branch `exp161-names-hop` off main
`324ebe607` (the branch's only code change is the merge memory fix below;
the index build and the hop are recorded runs). This lane BUILDS the full
names-stamped library index (the standing full index `53a1d874…` has call
edges but NO names tables, so restoration is inert on it), then records
the fresh 2.1.215→2.1.216 hop in names mode against it and in default mode
for contrast, and reads the line-diff breakdown off the two trees.

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
  6.39 h wall (~10,400 versions/h). Two solo `--jobs 1` pinyin attempts were
  also killed by memory BEFORE the policy landed; under the policy those
  versions are refused (below), never retried.
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
- Result: index id `cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28`,
  43 GB (vs the standing full index `53a1d874…`'s 29 GB — the +14 GB is
  the names tables: names bins 3.10 GB + names jsonl 12.25 GB + names.json
  0.02 GB), 65,839 pinned versions across 9,896 packages, 197,517 module
  shards, all 24 table shas verified against the manifest, 6,330 refused
  rows (the plan's own `unpublished` refusals + the 4 pinyin memory-hog
  entries). Names coverage: **100% of the 13,403,551 unit rows** carry a
  names row (9,284,587 lists, 243,569,885 entries, 1,271,115 distinct
  strings). Installed hard-linked at
  `/work/library-index/cb64c1f5…` with its `build.json` sidecar; `current`
  untouched.
- Pinyin is REFUSED, not retried (Andrew, 2026-10-29): the three failed
  chunks and the two solo attempts are the whole story — `--refuse
name@version:reason` (this branch) records one refusal entry per version
  in the merged manifest's refused rows, exactly like the unpublished
  refusals, and no chain ever retries it again.
- The parallel merger (exp162, `--merge-jobs N`) was wired in as the
  PREFERRED path behind its own proof gate: the chain self-runs the
  60-package pilot names replay with the parallel binary and only proceeds
  on manifest-id equality. The proof PASSED (id `94e1839c…` matched).
  The merge itself took four attempts (details in the build.json sidecar's
  `steps`): the buffered merger OOM-killed at 156.61 GB; the redesign's
  serial run livelocked on low MemFree during a giant shard's parse (the
  crawl signature: utime frozen, stime at 100% of a core — later
  reproduced with exp162's cache thermostat OFF, so the drops were
  exonerated); the parallel run WITH the buffered finish climbed to
  150.66 GB in the finish phase (it holds ~30 GB of simultaneously encoded
  tables: edges jsonl 19.8 GB, edges bin 7.2 GB, units jsonl 2.2 GB,
  literals 0.96 GB) and was killed at the agreed 140 GB trigger line after
  7,743 s; and exp162's streaming finish (`finish()` streams each output
  table to the staging dir as its rows finish, manifest shas unchanged, so
  the index id is unchanged) COMPLETED the same merge at **98.65 GB peak /
  6,007 s / exit 0** — 52 GB under the kill line, on a 30-worker parallel
  merge, byte-identity proven in both directions (pilot at jobs 30 AND
  jobs 1 on the streaming binary; exp152-store serial leg `diff -r`
  IDENTICAL, id `6bf769bb…`).

## The merge fix (the branch's only code change)

`crates/humanify-index/src/merge.rs` — the interned-compact-streamed merge;
`crates/humanify-core/src/libindex/table.rs` — `NameRowCompact`,
`compact_name_rows` (the test adapter), `names_lookup_files`, and
`NameTable::encode` rewritten over compact rows; `lookup.rs` and the tests
routed through the adapter; `build.rs` passes `staging = out`.

## The hop

One pair only — `npm run eval -- score exp161-names-hop --pairs 215->216`
(the dispatcher prints PARTIAL for a single-pair label; the full 4-pair set
is not re-run). The pair is 2.1.215→2.1.216, SCRATCH base (the default:
the base 2.1.215 is rebuilt by the current pipeline with no prior, so both
sides of every comparison are pure current-pipeline output). Names mode:
`--pipeline-set library.matching=names --pipeline-set
library.names_unlocked=true --pipeline-set library.index=<the new index dir>
--pipeline-set library.lookup.agreement=false`. The same pair is recorded
again with names OFF under `exp161-names-hop-default` for the contrast.
Cold run, no LLM cache (rule 10); alone under `flock /work/heavy.lock`;
endpoint the :8000 default.

Result: both legs exit 0. Trees under `/tmp/eval-work/<label>/`
(2.1.215-rebuilt, 2.1.216, both modes' cold and warm self-hops ran; the
names leg's COLD self-hop diverged 1,176 ln — outside the reference range
2–184, a cold re-roll artifact of the changed pipeline mode — while the
default leg read 18 ln, inside; the WARM self-hop was byte-identical with
0 cache writes on BOTH legs, which is the determinism half that matters).
The KPI table (pair 2.1.215→2.1.216; the deltas names-vs-default; the
bands are the default-config measured bands, given here for scale only
since this compares two different configs):

| KPI     | names ON      | names OFF     | delta    | band  |
| ------- | ------------- | ------------- | -------- | ----- |
| stmts   | 35903         | 35903         | =        | —     |
| clean   | 34558 (96.3%) | 34636 (96.5%) | −78      | ±17   |
| noise   | 359 (1.0%)    | 281 (0.8%)    | +78      | ±17   |
| novel   | 986 (2.7%)    | 986 (2.7%)    | **= 0**  | 0     |
| noiseLn | 5411          | 3304          | +2107    | ±1376 |
| realLn  | 122066        | 122066        | **= 0**  | 0     |
| reloc   | 351           | 337           | +14      | ±35   |
| newName | 771           | 753           | +18      | ±13   |
| mints   | 685           | 81            | **+604** | ±33   |

Hold columns (novel, realLn) byte-equal between the modes — restoration
touches naming and layout, not statement mass. The reducible KPIs all move
the WRONG way on this pair: restoration mints 604 MORE leftovers and adds
~2,100 bundle-noise lines. The on-disk composition (the reviewer's diff):
treeLn 25,989 vs 25,345 (+644); nameOnly 4,144 vs 3,946 (+198); naming
990 vs 756; real 20,069 vs 20,005 (+64).

## The line-diff breakdown

The exp157 buckets on the hop's two trees (prior vs fresh), vendor and src
separately — identical / nameOnly / movedPath / realChange / trulyAdded /
trulyRemoved, with files and lines each (`bucket-trees.ts` reads the one
sided line counts from the side the file lives on; the per-file rows stay
in /work/exp161-data).

**vendor** (files / lines):

| bucket       | names ON | names OFF | exp160 small-index names | exp157 names-era |
| ------------ | -------- | --------- | ------------------------ | ---------------- |
| identical    | 1614     | 1595      | 1614                     | 1613             |
| nameOnly     | 2 / 14   | 0 / 0     | 2 / 14                   | 2 / 14           |
| movedPath    | 0 / 0    | 0 / 0     | 0 / 0                    | 0                |
| realChange   | 4 / 30   | 0 / 0     | 4 / 30                   | 5 / 43           |
| trulyAdded   | 33 / 439 | 23 / 97   | 33 / 439                 | 32 / 424         |
| trulyRemoved | 0 / 0    | 0 / 0     | 0 / 0                    | 0                |

The names-mode vendor buckets are byte-for-byte the SAME shape as the
exp160 small-names-index buckets — the FULL index reproduces the small
index's restoration outcome on this bundle exactly. Names mode adds 10
vendor files (restored package files the default tree leaves inside the
bundle's flat vendor root): `@azure/core-rest-pipeline/module-d0d5d6a1.js`
(110 ln), `zod-to-json-schema/module-e57098c5.js` (76),
`@smithy/types/module-86eb974e.js` (29). The 2 nameOnly vendor files are
lodash module files whose 14 lines are SHORT-NAME disagreements
(`Rnm`→`bfm`, `Anm`→`_fm` across versions — the disclosed-retry lever's
target: the index's answer gives one 2–3-char shape, the prior tree
another).

**src** (files / lines):

| bucket       | names ON    | names OFF   | exp160 small-index names | exp157 names-era |
| ------------ | ----------- | ----------- | ------------------------ | ---------------- |
| identical    | 4206        | 4276        | 4206                     | 4206             |
| nameOnly     | 24 / 108    | 22 / 108    | 22 / 116                 | 53 / 298         |
| movedPath    | 0 / 0       | 0 / 0       | 0 / 0                    | 0                |
| realChange   | 585 / 20199 | 552 / 19909 | 587 / 20227              | 556 / 20071      |
| trulyAdded   | 30 / 4546   | 30 / 4546   | 30 / 4546                | 30 / 4546        |
| trulyRemoved | 10 / 342    | 0 / 0       | 10 / 342                 | 10 / 342         |

The nameOnly SHRINK the plan hoped for happened at the small-index stage
(exp157 names-era: 53 files / 298 ln → 22 / 116) and the FULL index HOLDS
it (24 / 108) — it does not go further, and against names OFF it is FLAT
(22/108 vs 24/108): restoration does not reduce identifier-only churn in
src on this pair; its visible src effects are (a) 70 files that were
byte-identical in default mode now differ (restored names disagreeing
across the hop), and (b) 10 src files / 342 ln leaving src — the
discovery recognizes library-shaped src code and places it into vendor
package dirs (`credential-provider-registry/sts-provider.js` (110 ln)
into the `@aws-sdk` family, `http-error.js` (76), `is-empty/`
(29)…), which the default tree keeps in src.

## Restoration's measured effect

- **It works**: the discovery claims 250 packages (22,674 claims, slice
  `bun-bun-cjs-min`, indexId `cb64c1f5…`), applies **4,718 restoration
  answers**, and restores 45.2% of the identifiers in the covered
  packages (real names like `convertToBuffer`, `Crc32`, `getInstanceAsync`,
  `PROTECTED_KEYS` — the smoke run's evidence).
- **Cross-version stability on its own scope** (`covered-stability.ts`
  over the discover-derived covered list): of 28,282 declared names in the
  covered vendor files, 28,276 carried from prior to fresh — **99.98%**
  (6 lost). Restoration names are stable across the version hop.
- **The cost is measured and real**: mints 81 → 685 (+604, band ±33) —
  bindings the DEFAULT pipeline settles (content-carry + transfer) ship
  minted in names mode. The minted declarations concentrate INSIDE claimed
  packages (`@opentelemetry` 633, `@grpc` 277, `semver` 274, `@aws-sdk`
  203 minted `lib_<8hex>` declarations): claiming a package takes its
  units out of the default naming paths, and the index's restoration
  answers cover only part of what it claims. Follow-up lever: raise the
  restoration's answer coverage inside claimed packages (or claim less).
- **Verdict for the reviewer**: the diff's shape improves (vendor files
  under package paths, real library names readable, D1/D2 layout holds);
  the diff's SIZE gets slightly worse on this pair (+644 treeLn, +604
  mints). Restoration trades a little diff churn for readability — the
  plan's §7 question "does the full names index make the reviewer's diff
  better" reads YES on names, NO on noise.

## D1/D2 shape

- Names tree: vendor root holds 4 fallback js + `_bun-modules.json` only;
  221 package dirs; 1,653 js files; 235 `lib_*` fallback files total;
  339 synthetic `module-*` files; js files spread to depth 9.
- Default tree: 593 js at the vendor root (276 of them fallback); 124
  dirs; 1,618 js; 1,107 `lib_*` fallback files; 0 synthetic module files;
  depths 1–3.
- D1 (no package file at the vendor root) HOLDS in names mode. D2
  (package dirs) HOLDS. The restoration moves 872 former fallback files
  (1,107 → 235) into package paths — the same shape the prior names-mode
  runs produced (exp160 era: 235 lib\_\* / 239 dirs; this run: 235 / 221).

## Gate

ALL 13 STAGES PASSED on the final tree (`npm run check`, bare, gate-4-wrap.log
in /work/exp161-data/logs — typecheck, lint, rust:fmt, rust:clippy, knip,
knip:prod, census:clones (advisory), census:concurrency, unit, rust:unit,
rust:build, rust:format-golden, e2e: 18 fixture pairs).
