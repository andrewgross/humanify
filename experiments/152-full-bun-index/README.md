# exp152: the FULL Bun library index with call edges (format `/4`)

## STATUS (2026-10-26)

DONE on branch `exp152-full-bun-index` (off main `26fa17e11`). A BUILD, not a pipeline change: no Rust
code and no default changed (the scripts commit touches this directory and
`experiments/108-app-corpus/scripts/build_chunks.py`). The index is BUILT, MERGED and installed at
`/work/library-index/53a1d8746fc823b32aa72c0c5729fa02a454b30b59cdc3a089bda0d4d589f5aa/` — it is NOT the
default: the `current` symlink still points at the `/3` index `c4d2bd6c…`, and the main session decides
after measurement. Step 0 groundwork for `docs/design/library-names-plan.md` step 2 (the call-context
rule needs edges over the whole corpus, not a sample).

## Totals

| quantity                       | value                                                                                            |
| ------------------------------ | ------------------------------------------------------------------------------------------------ |
| index id (manifest sha256)     | `53a1d8746fc823b32aa72c0c5729fa02a454b30b59cdc3a089bda0d4d589f5aa`                               |
| format                         | `humanify-library-index/4` (edges stamp `edges/1`)                                               |
| as-of                          | 2026-10-25T23:59:59.999Z                                                                         |
| pin list                       | 66,523 versions = 63,910 carried + 2,613 delta                                                   |
| pin list sha256                | `7fc74f75d745cabfa89b528b8173eb9e17e5f5ed243de58b2e5b820997507990`                               |
| indexed (≥1 shard)             | 65,843 versions of 9,897 packages, 197,529 shard keys                                            |
| refused                        | 6,326 = 5,646 unpublished + 680 no-shard (all install ETARGET)                                   |
| size on disk                   | 28.64 GiB (vs 6.62 GiB for the `/3` index — table below)                                         |
| wall time                      | 6.9 h build start → merged (7.0 h incl. registry: 5.6 min registry, 6.08 h shards, 50 min merge) |
| discover (Claude Code 2.1.216) | 7 s, 250 packages, 22,674 claims (vs `/3`: 8.2 s, 249, 22,423)                                   |

Everything below is aggregate numbers and build mechanics only — no bundle or app source is committed.

## The package set (pins + as-of delta, and what could NOT be recovered)

The installed `/3` index's manifest records every version's role as `pinned` — its original roots list
is gone (the pre-exp152 data dirs were deleted, see `experiments/151-index-edges/README.md` §10), so
exp142's delta recipe degenerates to the exp101 tail-pin rule: per package, year-last-12 + majors down
to 4, and only versions published after the old as-of. The delta at 2026-10-25 added **2,613 versions**.

That is safe here, verified rather than assumed: `scripts/root_roles.py` replays a hypothetical roots
role (year-all + minor) over the fresh registry at the new as-of and it would add **0 further
versions** beyond the pins-only set (`rootLike: 3,150` candidates, `extraVersionsIfRoots: 0`). The
carried 63,910 are exactly the `/3` index's built set, all of which built again.

Pin list: `LC_ALL=C sort -u` over carried + delta (byte-reproducible), one `pkg@ver` per line;
`sha256 = 7fc74f75d745cabfa89b528b8173eb9e17e5f5ed243de58b2e5b820997507990`.

## How it was built

Two Bun slices only — `bun:target=bun,format=cjs,minify,production` and
`bun:target=node,format=cjs,minify,production` — plus literals, `--sponge-regions origin`, exactly as
the exp151 `/4` recipe. Tools, all recorded:

| tool           | version / digest                                                                                                                                     |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| Bun            | 1.4.0, binary sha256 `33d56b070be6a9e3da0ab013038b43d1645d0534ca811ecdba4472599117eb4b` (`/work/tools/bun-1.4.0`)                                    |
| npm / node     | 11.16.0 / 24.18.0 (installs: `--ignore-scripts --before=<publish time>`, retry `--legacy-peer-deps`)                                                 |
| humanify-index | built from this branch @ `57f319a48` (Rust identical to main `26fa17e11`), sha256 `b9772047c9575a01d89486236aa905b4cb52ea7c5630ff8c7c0c6ef199b577da` |

Steps and wall time (a `build.json` sidecar with these numbers sits next to the installed index —
build times must NOT go in the manifest, the id is its hash):

| step                                         | wall              | notes                                                                                                                                                                                                      |
| -------------------------------------------- | ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| registry fetch (9,898 packuments)            | 335 s             | cached under `/work/exp152-data/registry/`                                                                                                                                                                 |
| shard phase (install + bundle + fingerprint) | 21,883 s (6.08 h) | 1,109 short-lived chunk processes (60 pins each), 7 concurrent, `taskset 34-63`, `--jobs 4`; wall includes one early driver restart, 1,896 s queued behind another lane, and the pinyin solo retries below |
| merge (`--merge-only`)                       | 3,008 s (50 min)  | peak RSS **117.1 GB** (exp142 peaked 35.6 GB on half the data)                                                                                                                                             |
| total (build start → merged)                 | 24,891 s (6.9 h)  | completed 2026-10-26T05:41:58Z                                                                                                                                                                             |

Child-process time (sums over parallel lanes, so they exceed wall): npm installs 81,883 s over 68,943
calls; Bun bundles 17,700 s over 302,527 calls — installs dominated the shard phase roughly 4.6:1.
Per-step wall INSIDE a chunk (install vs bundle vs fingerprint) is not separable from these logs at
7×4 parallelism; the split above is the honest granularity.

**Machine discipline.** Chunks ran holding `flock /work/heavy.lock` (one of the three machine slots),
with `build_chunks.py --wave-lock`: the lock is held while chunks run and released as soon as ANOTHER
process is seen waiting on it (tail of `/proc/locks`), draining running chunks first, then queueing
behind the other lane. The production run queued **1,896 s behind another lane's long job** (02:53→
03:24:54Z, `logs/chunks-…/waves.log`), plus a 232 s yield in the pre-restart run's first wave. The
final merge ran under both `/work/index-build.lock` and `/work/heavy.lock`. Progress went to
`/work/exp152-data/progress.txt` every chunk (versions done/total, rate, ETA).

## Failures by reason

The merged manifest's `refused` list (6,326) and the per-chunk FAILED logs agree exactly:

| reason      | count | what it means                                                                                                                  |
| ----------- | ----: | ------------------------------------------------------------------------------------------------------------------------------ |
| unpublished | 5,646 | registry-state record carried from the `/3` index; the same 5,646 set, byte-identical — candidates that can never be installed |
| no-shard    |   680 | pinned, but `npm install` failed with **ETARGET** under the as-of pin, so neither Bun slice ever ran                           |

The log side: 680 unique `FAILED pkg@ver` lines, every one an install-time
`npm error code ETARGET`, and the set is EXACTLY the manifest's no-shard set (0 either way). All 680
are delta versions (none of the 63,910 carried pins failed). The mechanism is the documented as-of
limit (exp142 saw 388/1,633 the same way): these packages pin exact dependency versions published
minutes AFTER their own publish time — e.g. `@ai-sdk/react@4.0.151` depends on `ai@7.0.147`, which
did not exist yet at react's own `--before` date, so `npm` cannot resolve it. Bleeding-edge families
dominate: `@ai-sdk/react` (12), `expo` (10), `@ai-sdk/azure`, `@ai-sdk/togetherai`, `@ai-sdk/vue`,
`@eslint-react/core`, `@eslint-react/eslint-plugin`, `eslint-plugin-react-rsc` (9 each). 1,933 of the
2,613 delta versions built; 65,843 total.

The `/3` index's own 462 no-shard versions were never re-attempted (they are not in any pin list) and
stay refused there — different recipes fail on different packages; the two no-shard sets are disjoint.

## Size vs the `/3` index

Per file, new `/4` vs the `/3` index at `/work/library-index/current/` (bytes):

| file                                 |           new `/4` |      old `/3` |
| ------------------------------------ | -----------------: | ------------: |
| edges.bun-bun-cjs-min.bin            |      3,501,856,368 |             0 |
| edges.bun-bun-cjs-min.jsonl          |      9,649,753,590 |             0 |
| edges.bun-node-cjs-min.bin           |      3,726,827,952 |             0 |
| edges.bun-node-cjs-min.jsonl         |     10,198,238,405 |             0 |
| units.bun-bun-cjs-min.bin            |        141,962,656 |   138,889,836 |
| units.bun-node-cjs-min.bin           |        134,830,168 |   131,795,996 |
| units.\*.jsonl (both slices)         |      2,209,127,093 | 2,154,533,035 |
| modules.\* (bun slices)              |        112,279,271 |   111,228,562 |
| literals.bin + files.jsonl           |      1,046,544,537 | 1,194,984,341 |
| esbuild/webpack/rollup units+modules |                  0 | 3,319,994,773 |
| manifest.json / versions.json        |         34,955,743 |    56,899,693 |
| **TOTAL**                            | **30,756,683,861** | 7,108,877,572 |

The edges tables are 27.1 GB of the 28.6 GiB (7.2 GB lookup `.bin` + 19.9 GB `.jsonl` provenance for
instruments — the pipeline loads the `.bin`). The rest is SMALLER than `/3`: the three non-Bun slices
(esbuild/webpack/rollup units+modules, 3.3 GB) are deliberately absent — this is the BUN index, not a
superset.

## Discover check: Claude Code 2.1.216

`humanify discover --library-index <index> <claude-code-2.1.216 entrypoint>` (report-only verb, no LLM
path). Same input, both indexes:

| metric             | new `/4`                                               | `/3` current                                           |
| ------------------ | ------------------------------------------------------ | ------------------------------------------------------ |
| seconds            | 7                                                      | 8.2                                                    |
| slice chosen       | bun-bun-cjs-min (`// @bun` banner)                     | bun-bun-cjs-min                                        |
| packages           | **250**                                                | 249                                                    |
| claims             | **22,674**                                             | 22,423                                                 |
| libraryClass units | **4,555**                                              | 3,533                                                  |
| vetoed             | 489                                                    | 488                                                    |
| rounds             | 21                                                     | 21                                                     |
| seeds              | banner 109, declared 205, index 5,046, queue-index 145 | banner 109, declared 205, index 5,044, queue-index 141 |

The one new package is `@anthropic-ai/claude-agent-sdk` — a delta-era version the `/3` as-of could not
have — contributing 1,021 class units (the whole libraryClass jump) and 5 seed units. Reports saved at
`/work/exp152-data/discover/v4.json` (new) and `v3.json` (`/3`).

One claim we checked and did NOT make: the per-package `intoEdges` counts come out non-zero on the `/3`
index too (10,349 over 229 packages vs 13,552 over 230 on `/4`), so those counts are not proven to come
from the edges tables — nothing in the default lookup reads edges yet (that is the plan's step 2), and
the discover verb's INTO counts have a bundle-side source. Discover on a 28.6 GiB index is not slower
than on the 6.6 GiB one (the edges table is mmap'd and read sparsely).

## Incidents (all resolved, all by recorded PID)

- **pinyin OOM.** One chunk was OOM-killed at 55.6 GB RSS while `pinyin@2.11.2/3.1.0/4.0.0` held 48/41
  GB with two other large chunks live; the driver's total crossed 108 GB. The three pinyin versions
  were rebuilt solo at `--jobs 1` (peaks 64.1 and 64.8 GB — single huge bundles spike far past the
  mimalloc plateau) and completed; no other version was affected.
- **driver restart.** The first wave scheme (fixed-size waves releasing the heavy lock between each)
  idled ~25% of cores at boundaries (measured chunk p50 70 s / p90 99 s / max 243 s); rewritten
  mid-run to drain-only-when-a-waiter-appears and restarted once, ~80 min in. Shard work is durable
  (shards land in the shared store as chunks go), so nothing was rebuilt.
- **premature merge.** The driver's own merge started before the pinyin retries finished; it was
  killed and ONE final `--merge-only` ran afterwards, whose numbers are the ones above.

## Scripts

- `scripts/build-full.sh` — the whole build: shims + `flock /work/index-build.lock` + `build_chunks.py`
  with `--as-of 2026-10-25 --wave-lock /work/heavy.lock --progress`.
- `experiments/108-app-corpus/scripts/build_chunks.py` — gained `--wave-lock` (waiter-detection drain)
  and `--progress`.
- `scripts/make_shims.sh`, `scripts/timed.sh` — npm/Bun wrappers that time every child into
  `logs/timing.log`.
- `scripts/step_times.py` — aggregates timing/TSV/shard mtimes into the `build.json` sidecar.
- `scripts/root_roles.py` — the would-a-roots-role-add-anything check reported above.
