# exp142: index coverage of versions released after the index's as-of date

## STATUS (2026-10-23)

DONE on branch `exp142-index-coverage` (off main `673c72cc0`). Harness and index-builder scripts only, **no crates
change, no default changed, nothing merged.** Judged ONCE on test / test2 / remin2 (summaries only).

- **The old numbers were a different quantity.** exp135's "pop-esbuild 270 KB after the as-of" and "pop 180 KB last
  12 months" were WRONG-claim bytes (its no-print class). Counted as MISSED library bytes (recall), versions
  published after the as-of are **0 KB on held-out, apps, bundlers, esbuild-app and wrangler**, and 4.7 / 5.2 MB on
  pop-bun / pop-esbuild, 5.5 / 7.0 MB on remin, 8.9 / 11.2 MB on remin3 (6.5-11.7% of each set's misses). But most
  of those units are **already named through a neighbouring version** at the same rate as units whose version is
  indexed (pop-bun 83% vs 82%), so the reachable part is small.
- **The largest miss class everywhere is "the version IS indexed"** (50-77% of missed bytes): not coverage.
  Held-out's second class is packages not indexed at all (29.6 of 72.2 MB missed), pop's is versions inside the
  12-month window the pins' 12-release cap left out (~10 MB per pop set, two packages are 8 MB of it).
- **Contamination: none either way, but the current index is STALE for six corpora.** The index's as-of is
  2026-10-11. pop, remin, remin3 (exp108 locks) and every judge (test 2026-10-14, test2 2026-10-15, remin2 exp108
  locks 2026-10-14) resolved their dependencies AFTER it: they are scored against an index older than the bundles.
  An index as of "now" knowing versions newer than a bundle is the real user's situation and leaks no truth as long
  as version choice is corpus-blind (it is: `delta_plan.py` reads only the index's own package list and a registry
  snapshot). **Honest framing: one index as of (or after) the newest bundle's dependency resolution — 2026-10-15
  for this campaign's corpora.** A per-bundle as-of is not expressible (one `asOf` per index; the lookup has no
  publish-time filter) and the current index already post-dates most bundles (wrangler by 21 months).
- **Simulation (before building): below the 0.3 bar.** Expected pop F0.5 +0.12 / +0.14 (upper bound +0.43 / +0.46),
  held-out 0. Built anyway because the delta cost 1.4 h (cap 24 h) and the sim cannot resolve units whose function
  prints the family already holds.
- **Built: delta index `/work/exp142-data/full/ix/k2-all-r5-origin-prod-asof20261015`** (7.1 GB, id `c4d2bd6c…`,
  as-of 2026-10-15): k2-all-r5-origin-prod + 1,633 policy-picked versions published 2026-10-12..15 of 937 packages
  already in it, exactly exp135's recipes; every old version kept (0 lost); 1,245 built, 388 failed
  `npm install --before` (monorepo siblings published seconds after their dependent: the documented as-of limit). 61 min shards on
  30 cores + 21 min merge (35.6 GB peak).
- **Results (scorer 14, main defaults, both scopes): F0.5 up on 11 of 14 dev sets and all 9 judge sets; precision up
  or equal on every judge set and 11 of 14 dev sets, DOWN on 3 dev sets by <= 0.006 points**: pop-bun 99.518 ->
  99.512 (+24.8 KB wrong), pop-esbuild 99.315 -> 99.311 (+23.2 KB), remin3-terser 99.855 -> 99.849 (+2.7 KB). All of
  it is CARRIERS in new releases (a new major that ships its dependencies inlined claims another shipper's copy:
  ipx 4.0.0 claiming vitepress's vendored sitemap chunk, 22 KB in each pop set; got 16.1.0 claiming
  @sindresorhus/is / uint8array-extras; a new claude-agent-sdk claiming zod). App claimed 0 everywhere.
  pop +0.17 / +0.18, held-out +0.03, esbuild-app +0.29, remin +0.07 / +0.15, remin3 +0.13 / +0.35;
  test +0.07..+0.23, test2 +0.07..+0.24 (test2-rollup P +0.10), remin2 +0.05 / +0.11.
- **Recommendation: the ship bar (P >= base on EVERY set) is NOT met on dev (3 sets, max -0.006 pts); the judges
  pass cleanly.** Andrew's call. My recommendation: adopt the as-of-2026-10-15 index as the measurement index
  (the as-of must not be older than the corpora it scores; the judges agree), and treat the dev losses as the open
  carrier class (exp133/134/139), not as a coverage cost. If the strict bar holds, keep k2-all-r5-origin-prod.
  Either way, every FUTURE index build should set its as-of to the newest corpus resolution date, and a delta (this
  recipe) is the cheap way to move it: ~1.5 h per few days of releases, against ~18 h for a version-density rebuild.

## 1. Census: missed library bytes by why the index lacks the truth's version

`scripts/misses.py` emits every library unit scorer 14 judges (both scopes) with its truth row (shipper package T,
version V, origin) and its verdict under main's defaults on k2-all-r5-origin-prod (labels `*exp138-helper*`,
`remin3-exp141-main`; reproduced exactly by `*exp142-base*` at `673c72cc0`). `scripts/classify.py` puts each MISSED
byte (unclaimed, or claimed as the wrong family / app) in one class from the index's `versions.json` and a fresh
registry snapshot (`scripts/fetch_registry.py`, every indexed package + every unindexed truth package, fetched
2026-10-23): held (T@V indexed), after-asof (T indexed, V published after 2026-10-11), not-chosen (T indexed, V
published before, the version policy did not pick it), package not indexed. Bytes in KB; held-out by aggregate
only; test / test2 / remin2 not read.

| set            | library KB | missed KB | held (version indexed) | after as-of | not chosen: last 12 months | not chosen: latest patch of minor | not chosen: older | package not indexed | no version / unknown time | after as-of share of missed |
| -------------- | ---------: | --------: | ---------------------: | ----------: | -------------------------: | --------------------------------: | ----------------: | ------------------: | ------------------------: | --------------------------: |
| heldout        |    229,606 |    72,192 |                 36,366 |          13 |                        882 |                             2,778 |             2,513 |              29,640 |                         0 |                        0.0% |
| esbuild-app    |     18,161 |     4,587 |                  3,423 |           0 |                        264 |                                 0 |                 0 |                 331 |                       569 |                        0.0% |
| wrangler       |      1,811 |     1,211 |                    909 |           0 |                          0 |                               102 |                41 |                 159 |                         0 |                        0.0% |
| apps-esbuild   |    149,430 |    33,398 |                 20,043 |           0 |                        576 |                             6,316 |               224 |               6,219 |                        20 |                        0.0% |
| apps-rollup    |     92,553 |    16,777 |                  8,692 |           0 |                        763 |                             4,386 |                94 |               2,842 |                         0 |                        0.0% |
| apps-webpack   |     72,737 |    19,276 |                  9,936 |           0 |                         18 |                             4,356 |               106 |               1,506 |                     3,354 |                        0.0% |
| dev-rollup     |      2,093 |       470 |                    470 |           0 |                          0 |                                 0 |                 0 |                   0 |                         0 |                        0.0% |
| dev-webpack    |      1,786 |       663 |                    663 |           0 |                          0 |                                 0 |                 0 |                   0 |                         0 |                        0.0% |
| pop-bun        |    318,254 |    72,528 |                 44,745 |       4,690 |                     10,062 |                             2,717 |             1,023 |               9,277 |                        15 |                        6.5% |
| pop-esbuild    |    318,602 |    67,835 |                 40,917 |       5,173 |                      9,661 |                             2,007 |               290 |               8,710 |                     1,078 |                        7.6% |
| remin-esbuild  |     97,254 |    46,916 |                 32,636 |       5,504 |                        367 |                             4,965 |             1,762 |               1,280 |                       402 |                       11.7% |
| remin-terser   |     91,167 |    72,628 |                 55,790 |       7,020 |                        376 |                             5,756 |             2,057 |               1,257 |                       373 |                        9.7% |
| remin3-esbuild |    202,078 |   102,096 |                 70,468 |       8,856 |                        884 |                             5,640 |             1,798 |              13,385 |                     1,066 |                        8.7% |
| remin3-terser  |    191,574 |   158,274 |                122,321 |      11,214 |                        972 |                             7,224 |             2,451 |              13,101 |                       990 |                        7.1% |

(`data/census.json`; top packages per class for the non-held-out sets in `data/census-pkgs.json`.)

- Every after-as-of byte is a version published 2026-10-12..14, and in pop / remin / remin3 only: those corpora
  install exp108's lock files, resolved 2026-10-14 (`experiments/108-app-corpus/scripts/closures.py`: "ranges
  resolved today"). Top pop items: lucide-react 1.57.0, undici 8.12.1, mediabunny 1.61.4, @sentry/conventions 0.27.0,
  openai 7.33.0 (each 0.2-0.8 MB).
- Packages not in the index at all are 0.6-1.3% of pop/remin misses but **41% of held-out's** (29.6 MB): a
  package-set lever, measured by exp133 (dl25k) as a precision loss through carriers.
- pop's "not chosen, last 12 months" (~10 MB per set) is the pins' cap (`year-last12+major4`: at most the 12 newest
  releases of the window): googleapis 171.4.0 (5.8 MB) and oxfmt 0.32.0 (2.3 MB) are 80% of it. Most of those units'
  FUNCTION prints are held (section 2), so a density rebuild (~18 h, exp135) is not obviously worth it either.

### Named rate by class: the after-as-of units are mostly named already

Share of each class's library bytes (named and missed) that main names (`data/named-rate-by-class.txt`):

| set            | held |  after as-of | not chosen | not indexed |
| -------------- | ---: | -----------: | ---------: | ----------: |
| heldout        | 0.81 | 0.57 (30 KB) |       0.43 |        0.02 |
| pop-bun        | 0.82 |         0.83 |       0.62 |        0.00 |
| pop-esbuild    | 0.83 |         0.81 |       0.65 |        0.01 |
| remin-esbuild  | 0.58 |         0.35 |       0.21 |        0.00 |
| remin-terser   | 0.23 |         0.14 |       0.06 |        0.00 |
| remin3-esbuild | 0.56 |         0.38 |       0.27 |        0.00 |
| remin3-terser  | 0.19 |         0.17 |       0.12 |        0.00 |

On pop a unit of a post-as-of version is named as often as one whose version is indexed: an adjacent version
carries the same prints. Only remin/remin3 show a gap.

## 1b. Contamination: when was each corpus resolved, against which as-of?

A bundle cannot have been resolved before the newest publish time of any (package, version) its library units
carry. Per dev set (`classify.py`, from the truth rows; held-out aggregate only):

| set            | bundles | newest dependency publish: min / median / max | bundles resolved after the as-of |
| -------------- | ------: | --------------------------------------------- | -------------------------------: |
| heldout        |      59 | 2023-06-10 / 2026-10-07 / 2026-10-12 00:50    |                                1 |
| esbuild-app    |       1 | 2026-10-11 / 2026-10-11 / 2026-10-11 16:18    |                                0 |
| wrangler       |       1 | 2025-01-16 / 2025-01-16 / 2025-01-16 09:24    |                                0 |
| apps-esbuild   |      39 | 2025-06-16 / 2026-10-08 / 2026-10-11 19:34    |                                0 |
| apps-rollup    |      23 | 2025-06-16 / 2026-10-08 / 2026-10-11 19:34    |                                0 |
| apps-webpack   |      15 | 2026-08-26 / 2026-10-08 / 2026-10-11 19:34    |                                0 |
| dev-rollup     |      14 | 2026-04-07 / 2026-09-24 / 2026-10-10 21:12    |                                0 |
| dev-webpack    |      13 | 2025-09-11 / 2026-09-18 / 2026-10-10 21:12    |                                0 |
| pop-bun        |      45 | 2023-03-16 / 2026-10-13 / 2026-10-14 09:33    |                               31 |
| pop-esbuild    |      46 | 2023-03-16 / 2026-10-13 / 2026-10-14 09:33    |                               32 |
| remin-esbuild  |      36 | 2023-08-23 / 2026-10-07 / 2026-10-14 06:12    |                               12 |
| remin-terser   |      36 | 2023-08-23 / 2026-10-07 / 2026-10-14 06:12    |                               12 |
| remin3-esbuild |      72 | 2024-06-17 / 2026-10-09 / 2026-10-14 09:29    |                               30 |
| remin3-terser  |      72 | 2024-06-17 / 2026-10-09 / 2026-10-14 09:29    |                               30 |

Judges, from their own scripts and READMEs (their units were not read): test resolved 2026-10-14 (exp100 `build.py`,
installs "ranges resolved" at build), test2 2026-10-15 (exp116 README: "Versions resolve as of today
(2026-10-15)"), remin2 2026-10-14 (exp108's locks, as remin / remin3 / pop).

| corpus                                | resolved                                                         | vs as-of 2026-10-11 (current)           | vs as-of 2026-10-15 (delta)           |
| ------------------------------------- | ---------------------------------------------------------------- | --------------------------------------- | ------------------------------------- |
| held-out, apps, bundlers, esbuild-app | <= 2026-10-12 00:50 (one held-out bundle 51 min after the as-of) | ~ same day                              | index 3-4 days newer than the bundles |
| wrangler                              | 2025-01-16                                                       | index 21 months newer                   | 21 months newer                       |
| pop, remin, remin3, remin2, test      | 2026-10-14                                                       | **index 3 days OLDER than the bundles** | index 1 day newer                     |
| test2                                 | 2026-10-15                                                       | **index 4 days OLDER**                  | same day                              |

Verdict. "The index must not know versions published after a bundle was built" is a time-travel rule; it would
need a per-bundle as-of, which the format cannot express (one `asOf` per index, no publish-time filter at lookup)
and which the current index already breaks for wrangler and every bundle resolved before 2026-10-11. Knowing newer
versions cannot leak the answer: they are not in the bundle, and what they add is distractors (carriers, section 4)
— the real user's situation, since a user indexes npm as of the day they analyse. What WOULD contaminate is picking
versions or packages FROM the corpus truth; the delta is drawn corpus-blind from the index's own package list. (The
package SET itself was drawn from exp108's 2,155 training apps, the pool pop / remin / remin3 / remin2 also come
from: true of both indexes alike.) The honest framing is therefore **one index as of the newest bundle's resolution
date in the evaluated corpora** (2026-10-15 here), which no corpus contradicts; the current as-of under-serves the
six corpora resolved after it, the judges among them.

## 2. Simulation before the build

`scripts/presence.py`: for each missed unit, does the truth package's FAMILY hold its whole print, one of its
function prints (>= 30 nodes), or none ("unheld") in the slice the bundle reads (`data/presence.json`)? A delta can
only add prints, so the reachable bytes are the unheld EXCESS over the held-version control (units whose version
IS indexed still end unheld 4-20% of the time: drop rules, print shape), times the named rate given a held print.
Recorded before the run (`data/prediction.txt`; F0.5 at P = 99.5):

| set            | after-as-of missed KB | unheld KB | upper bound R / F0.5 | expected KB | expected R / F0.5 | measured R / F0.5 |
| -------------- | --------------------: | --------: | -------------------: | ----------: | ----------------: | ----------------: |
| heldout        |                    13 |         2 |        +0.01 / +0.00 |           0 |     +0.00 / +0.00 |     +0.08 / +0.03 |
| pop-bun        |                 4,690 |     2,724 |        +1.47 / +0.43 |       1,247 |     +0.39 / +0.12 |     +0.60 / +0.17 |
| pop-esbuild    |                 5,173 |     2,891 |        +1.62 / +0.46 |       1,516 |     +0.48 / +0.14 |     +0.62 / +0.18 |
| remin-esbuild  |                 5,504 |     2,438 |        +5.66 / +2.78 |         694 |     +0.71 / +0.37 |     +0.13 / +0.07 |
| remin-terser   |                 7,020 |     4,309 |        +7.70 / +9.96 |         416 |     +0.46 / +0.68 |     +0.09 / +0.15 |
| remin3-esbuild |                 8,856 |     4,793 |        +4.38 / +2.32 |       1,414 |     +0.70 / +0.39 |     +0.23 / +0.13 |
| remin3-terser  |                11,214 |     7,340 |        +5.85 / +8.91 |         533 |     +0.28 / +0.48 |     +0.20 / +0.35 |

Below the brief's >= 0.3 F0.5 on held-out or pop. Built anyway (cost 1.4 h; the fn-held class is unresolvable
without a build). The measurement landed between expected and upper bound on pop, under expected on remin, and
above zero on held-out and esbuild-app, whose truth versions are all pre-as-of: a new release whose code matches an
older version the policy never picked (the not-chosen class) names it.

## 3. The delta build

`scripts/delta_plan.py` (corpus-blind): for every package the index lists (manifest `packages` roles), the versions
its own policy picks at the new as-of — `year-last12+major4` for pins (exp101 `tail_pins.py`, how pins-k2 was
drawn), plus the builder's `year-all+minor` for roots (`recent` / `minor` roles) — minus what the index holds,
keeping only versions published after the old as-of (an old version the policy did not pick stays out: this is the
as-of move only). At 2026-10-14 / 15 / 22: 1,266 / 1,633 / 3,554 versions. **As-of 2026-10-15** = test2's
resolution day, the newest of any corpus: 1,633 versions of 937 packages (`data/delta-2026-10-15.txt`).

`scripts/build-delta.sh` = exp135's `build-prod.sh` with `build_chunks.py --as-of 2026-10-15 --build-pins <delta>`
(new options: shards for the delta only; the merge reads every old version + the delta, all `--pinned`, so no policy
re-pick can drop one). Store = hard-link copy of exp135's (dry run: 376,434 shards reused, 9,798 = 1,633 x 6 to
build); work dir empty; registry = the fresh snapshot (+ `bun.json` for the slice record). humanify-index built at
`673c72cc0`.

- 61 min of shards (17 chunks of ~96 versions, 7 processes x 4 jobs on cores 34-63, 6-9 GB peak each), merge 20.6
  min at 35.6 GB. Index 7.1 GB, `asOf` 2026-10-15T23:59:59.999Z, 93,791 versions (old 92,452; 0 lost; +1,245 delta
  +94 dependency versions installed with them).
- **388 of 1,633 failed (`no-shard`, 24%)**: `npm install --before=<publish time>` cannot resolve a sibling
  published seconds later (`notarget ... with a date before`: @aws-sdk, @graphql-tools, @peculiar ...). The as-of
  limit docs/design/library-index.md §3 names; it bites recent monorepo releases hardest. Lead (builder): install
  with `--before` = publish time + a short grace (still <= the as-of).

## 4. Results (scorer 14, structural, main `673c72cc0` defaults, both scopes)

Labels: base = `*exp142-base*` (k2-all-r5-origin-prod, id `9d40265a`), new = `*exp142-asof*` (the delta index, id
`c4d2bd6c`); judge labels `test-` / `test2-` / `remin2-exp142-{base,asof}` (scored once, summaries only).
Bar: P >= base, F0.5 >= base - 0.002, app claimed <= base (`scripts/table142.py`, `data/table.json`).

| set                |        base P / R / F0.5 | as-of delta P / R / F0.5 |      d P / d R / d F0.5 | wrong+app KB base / new | bar  |
| ------------------ | -----------------------: | -----------------------: | ----------------------: | ----------------------: | ---- |
| cc                 | 100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 | +0.000 / +0.00 / +0.000 |               0.0 / 0.0 | PASS |
| esbuild-app        |  99.078 / 74.74 / 93.020 |  99.115 / 75.60 / 93.310 | +0.037 / +0.86 / +0.289 |           126.3 / 122.5 | PASS |
| heldout            |  99.636 / 68.56 / 91.354 |  99.641 / 68.64 / 91.385 | +0.005 / +0.08 / +0.031 |           575.7 / 568.4 | PASS |
| wrangler           |  99.069 / 33.14 / 70.870 |  99.069 / 33.14 / 70.870 | +0.000 / +0.00 / +0.000 |               5.6 / 5.6 | PASS |
| apps-esbuild       |  99.908 / 77.65 / 94.490 |  99.908 / 77.65 / 94.491 | +0.000 / +0.00 / +0.000 |           107.3 / 107.3 | PASS |
| apps-rollup        |  99.112 / 79.31 / 94.398 |  99.112 / 79.31 / 94.398 | +0.000 / +0.00 / +0.000 |           678.8 / 678.8 | PASS |
| apps-webpack       |  99.599 / 73.50 / 92.994 |  99.599 / 73.51 / 92.997 | +0.000 / +0.01 / +0.002 |           215.5 / 215.5 | PASS |
| dev-rollup         |  99.873 / 77.53 / 94.430 |  99.873 / 77.53 / 94.430 | +0.000 / +0.00 / +0.000 |               2.1 / 2.1 | PASS |
| dev-webpack        |  99.642 / 62.89 / 89.216 |  99.642 / 62.89 / 89.216 | +0.000 / +0.00 / +0.000 |               4.0 / 4.0 | PASS |
| pop-bun            |  99.518 / 77.21 / 94.082 |  99.512 / 77.81 / 94.255 | -0.006 / +0.60 / +0.174 |         1189.9 / 1214.7 | FAIL |
| pop-esbuild        |  99.315 / 78.71 / 94.373 |  99.311 / 79.33 / 94.549 | -0.004 / +0.62 / +0.176 |         1730.6 / 1753.9 | FAIL |
| remin-esbuild      |  99.719 / 51.76 / 84.129 |  99.720 / 51.89 / 84.197 | +0.001 / +0.13 / +0.068 |           141.8 / 141.8 | PASS |
| remin-terser       |  99.902 / 20.33 / 56.043 |  99.916 / 20.43 / 56.191 | +0.014 / +0.09 / +0.148 |             18.2 / 15.6 | PASS |
| remin3-esbuild     |  99.679 / 49.48 / 82.863 |  99.679 / 49.71 / 82.994 | +0.000 / +0.23 / +0.131 |           322.1 / 323.5 | PASS |
| remin3-terser      |  99.855 / 17.38 / 51.236 |  99.849 / 17.58 / 51.585 | -0.006 / +0.20 / +0.349 |             48.4 / 51.1 | FAIL |
| **test-bun**       |  99.557 / 70.16 / 91.860 |  99.561 / 70.38 / 91.936 | +0.004 / +0.22 / +0.077 |           918.6 / 914.2 | PASS |
| **test-esbuild**   |  99.272 / 73.37 / 92.726 |  99.275 / 73.57 / 92.793 | +0.003 / +0.20 / +0.067 |         1670.9 / 1668.9 | PASS |
| **test-rollup**    |  98.966 / 72.43 / 92.210 |  98.977 / 73.11 / 92.435 | +0.011 / +0.67 / +0.225 |           806.6 / 804.9 | PASS |
| **test-webpack**   |  99.483 / 72.18 / 92.485 |  99.483 / 72.80 / 92.690 | +0.000 / +0.63 / +0.205 |           547.6 / 552.4 | PASS |
| **test2-bun**      |  99.361 / 69.43 / 91.475 |  99.365 / 69.69 / 91.566 | +0.005 / +0.25 / +0.091 |         1074.3 / 1070.4 | PASS |
| **test2-esbuild**  |  99.649 / 71.04 / 92.222 |  99.652 / 71.25 / 92.292 | +0.002 / +0.20 / +0.070 |           661.9 / 659.4 | PASS |
| **test2-rollup**   |  96.448 / 75.73 / 91.443 |  96.549 / 76.30 / 91.683 | +0.101 / +0.58 / +0.239 |         1610.5 / 1575.0 | PASS |
| **test2-webpack**  |  99.289 / 70.08 / 91.648 |  99.294 / 70.53 / 91.807 | +0.005 / +0.46 / +0.159 |           835.8 / 834.9 | PASS |
| **remin2-esbuild** |  99.499 / 52.12 / 84.191 |  99.499 / 52.20 / 84.236 | +0.001 / +0.09 / +0.045 |           281.3 / 281.3 | PASS |
| **remin2-terser**  |  99.712 / 16.80 / 50.178 |  99.713 / 16.86 / 50.285 | +0.001 / +0.06 / +0.107 |             48.9 / 48.9 | PASS |

(judges in bold; app claimed 0 on every set of both labels.)

**Where the dev precision went** (per-unit diff of `misses.py` rows, base vs new, same units):

| set            | named gained KB | named lost KB | new wrong KB | what                                                                                                                                                                |
| -------------- | --------------: | ------------: | -----------: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| heldout        |             231 |            51 |            0 | 7.3 KB of wrong fixed                                                                                                                                               |
| esbuild-app    |             155 |             0 |            0 | 3.8 KB of wrong fixed                                                                                                                                               |
| pop-bun        |           1,924 |             5 |         24.8 | 22.3 vitepress 2.0.0-alpha.20 chunk (truth origin: sitemap) claimed `ipx` (new 4.0.0 / 3.1.2); 2.5 zod 4.6.7 claimed `@anthropic-ai/claude-agent-sdk` (new 0.3.29x) |
| pop-esbuild    |           1,994 |             5 |         23.2 | 22.5 the same vitepress chunk -> `ipx`; 0.5 katex -> @tiptap/extension-mathematics; 0.2 fast-equals -> fast-stringify                                               |
| remin3-terser  |             388 |             0 |          2.7 | @sindresorhus/is 8.1.0 claimed `got` (new 16.1.0)                                                                                                                   |
| remin3-esbuild |             481 |             9 |          1.4 | uint8array-extras -> `got`; react-remove-scroll -> @radix-ui/react-select                                                                                           |

Every new wrong claim names a package whose NEW release inlines another package's code (a carrier). zod 4.6.7 itself
is in the delta but did not win the unit; ipx 4.0.0 is a new major. The fix belongs to the carrier lane (exp133 /
134 / 139: no global carrier rule has passed a simulation yet), not to the as-of.

## 5. Gate

| check                                                | result                                                                                                      |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `npm run check` (bare, `/work/exp142-data/gate.log`) | exit 0, ALL 13 STAGES PASSED (on the tree committed with this README)                                       |
| crates/ changed                                      | no (harness + builder scripts only) -> no neutrality run                                                    |
| judge looks                                          | test / test2 / remin2: one run each of base + delta, summaries only, after every dev number above was final |

## 6. Files

- `scripts/misses.py` every judged library unit + truth row + verdict (output outside the repo: names packages)
- `scripts/classify.py` miss classes + per-bundle resolution floors -> `data/census.json`, `data/census-pkgs.json`
- `scripts/presence.py` family print presence -> `data/presence.json`; prediction `data/prediction.txt`
- `scripts/fetch_registry.py` fresh registry snapshot in the builder's format
- `scripts/delta_plan.py` the corpus-blind as-of delta -> `data/delta-2026-10-15.txt`
- `scripts/build-delta.sh` the build (store hard-linked from exp135; `build_chunks.py --as-of / --build-pins`)
- `scripts/score-all.sh`, `scripts/score-judges.sh`, `scripts/table142.py` -> `data/table.json`
- remin3 (exp141's dev corpus) is registered here too (ported from exp141's branch, identical files) so it could be
  scored; merging both branches is a no-op on those files.

Data (not committed): `/work/exp142-data/full/ix/k2-all-r5-origin-prod-asof20261015` (the index, 7.1 GB),
`/work/exp142-data/full/store` (hard links into exp135's store + the delta's shards), `/work/exp142-data/full/work`
(the delta's installs/builds), `/work/exp142-data/registry-fresh` (fresh packuments, 178 MB),
`/work/exp142-data/misses` (per-unit rows, ~2.7 GB, deletable).
