# exp118: which generic package population reaches the held-out residual? (census, then one build)

## STATUS (2026-10-15)

DONE on branch `exp118-coverage-census` (off main `0b2b136b`, main `8ba487df` merged in: shadow_ties on by default,
scorer 8). Data and scripts only; no pipeline code. NOT recommended for merge as an index default.

- **Census (section 1).** Of the generic policies asked about, **npm's last-month DOWNLOAD rank is the only one
  that reaches the not-indexed held-out residual.** The download top-50k holds 24.65 of its 24.86 points, and the
  top-25k holds 23.23. Dependents rank (<= 2.9 points even at top-100k), the transitive dependency closure (0.16),
  devDependencies (0.33), bundleDependencies (0), vendored-path names (0.06), scopes (1.41) and maintainers (4.61,
  from an unbounded population) do not reach it. **But one package is 12.90 of the 24.86 points**: a single ~3.9 MB
  wasm-bindgen payload unit in one app. Without that package, download top-25k holds 10.33 points.
- **Build.** `k2-all-dl25k` = k2-all plus the 13,785 non-`@types` packages of the download top-25k that k2-all does
  not index, at `year-last3+major1`, in all five slices (`/work/exp118-data/idx/ix/k2-all-dl25k`, /3, 12.7 GB,
  id `cae793d8`). It adds 12,433 owners and 30,279 versions. Shards took 13.2 h on 30 cores, and the merge took
  34 min at a 61.9 GB peak.
- **Result (scorer 8, structural, main's defaults; before = `*exp119-k2all-st*`).**
  - Held-out F0.5 rises from 90.42 to 94.05 (R 66.69 -> 80.16), but **precision FALLS** from 99.24 to 98.30. The
    recall comes almost entirely from that one payload package (12.9 points, now named in full). Every other
    previously not-indexed package together gains about 0.6 recall points. Without the largest package, held-out
    F0.5 is about 93.7 -> 93.0 (DOWN).
  - **Every Bun/esbuild test set falls**: test-bun -4.90, test-esbuild -3.94, test2-bun -1.70, test2-esbuild
    -1.94 F0.5. Precision drops 2.6 to 5.8 points.
  - task-master falls by 6.16 F0.5.
  - Webpack gains a little: test-webpack +0.27, test2-webpack +0.79. Pop gains +0.5 / +0.9. Wrangler gains +4.5.
  - App claimed is 0 everywhere.
- **Why.** Covering a package by NAME is not matching its bytes. The not-indexed packages became mostly "hash
  differs / absent" (3.99 points), "too small" (2.05), "version missing" (1.23) and "other package" (1.17). Most of
  the newly added popular packages are SPONGES: their own dist inlines their dependencies, so their rows claim that
  code by SEED. On held-out, 96% of the new wrong bytes are seeds, 94% are vendored copies whose truth names the
  origin, one claimer is 87% of them, and one app is 93%.
- **Recommendation.** Do not ship `k2-all-dl25k`, and do not grow the index by download rank yet. The index cannot
  grow until a sponge rule stops an inlining package from claiming its dependencies' code. That rule is index-time
  origin ownership (exp110, whose re-measure is now due: it was flat when shippers of copies were rarely indexed)
  or vendored paths (exp115). With such a rule, the download top-25k is the population to try again. The census
  numbers say it is the only generic population that holds the residual.

## 1. The census (read-only)

**Question.** Under the shipped default index `k2-all` (exp113), 24.86% of the held-out apps' library bytes
(7.55 MB of 30.36 MB, 70 packages) are in packages the index does not hold at all (miss class "a"). exp108 showed a
bigger app corpus from the same populations cannot reach them (22.5 points are in no app of 2,133). Which GENERIC
selection policy would?

**Method** (`scripts/`; aggregates only, held-out is dev):

- `misses.py` re-runs exp096's scorer on the `exp113-k2all` label (k2-all, shipped defaults, structural) and keeps
  each not-indexed missed byte's package. The per-package file is written OUTSIDE the repo
  (`/work/exp118-data/misses-*.json`) and used only for membership tests; nothing here prints or commits a held-out
  package name, and no list was picked from it.
- `populations.py` builds each candidate population from public npm metadata and the index alone:
  [packages.ecosyste.ms](https://packages.ecosyste.ms) npm rankings (fetched 2026-10-15 by `fetch_ranks.py`: by
  dependent-package count to rank 20k, by last-month downloads to rank 50k; deeper ranks by the count at that rank),
  and registry.npmjs.org metadata for every indexed package (`fetch_npm.py`: every indexed version's runtime
  dependencies; the newest indexed version's devDependencies, bundleDependencies and maintainers), plus the index's
  own file paths.
- `census.py` reports, per population: the share of the not-indexed bytes it holds, as points of ALL held-out
  library bytes (so it reads directly against recall), with and without the single largest missed package, and a
  cost estimate.

**One package is 12.90 of the 24.86 points** (a single ~3.9 MB generated payload unit, the wasm-bindgen glue exp108
§4 already describes). Every share below is therefore also given without it (`ex-top1`); a policy that only looks
good because of that one package is not a policy.

**Cost model** (stated; `census.py`): new versions = new packages x 5.48 (exp108's pins per new root at
`year-last12+major4`); size = 69 KB per version (k2-all: 6.4 GB / 92,237 owner-versions); build = 35 core-seconds per
version over 30 cores (exp108's Bun/esbuild 8.9 + exp113's webpack/rollup/literals 25.6).

| policy (packages NOT already indexed)                    |     pts of library | ex-top1 |    miss pkgs |   + packages | + versions (est) | index GB (est) | build h @30 (est) |
| -------------------------------------------------------- | -----------------: | ------: | -----------: | -----------: | ---------------: | -------------: | ----------------: |
| **(a) dependents rank** top 20k (>= 61 dependents)       |               2.04 |    2.04 |           17 |       14,022 |           76,855 |            5.3 |                25 |
| top 50k (>= 18; threshold)                               |               2.79 |    2.79 |           28 |    <= 50,000 |       <= 274,050 |           19.0 |                89 |
| top 100k (>= 8; threshold)                               |               2.89 |    2.89 |           32 |   <= 100,000 |       <= 548,100 |           38.0 |               178 |
| **(b) download rank** 0-16k (>= 1.61 M/month)            |               4.47 |    4.47 |           33 |        7,316 |           40,099 |            2.8 |                13 |
| 16k-25k (>= 503 k/month)                                 |          **18.76** |    5.86 |           14 |        7,740 |           42,423 |            2.9 |                14 |
| 25k-35k (>= 208 k)                                       |               0.61 |    0.61 |            3 |        9,407 |           51,560 |            3.6 |                17 |
| 35k-50k (>= 81 k)                                        |               0.80 |    0.80 |            8 |       14,523 |           79,601 |            5.5 |                26 |
| 16k-50k                                                  |              20.18 |    7.28 |           25 |       31,670 |          173,583 |           12.0 |                56 |
| **0-50k**                                                |          **24.65** |   11.75 |           58 |       38,986 |          213,682 |           14.8 |                69 |
| 50k-100k (>= 13 k; threshold)                            |               0.08 |    0.08 |            6 |    <= 50,000 |       <= 274,050 |           19.0 |                89 |
| **(c) closure** runtime deps of every indexed version    |               0.10 |    0.10 |            4 |        2,558 |           14,020 |            1.0 |               4.5 |
| ... transitive, to the fixed point (7 levels)            |               0.16 |    0.16 |           12 |        4,261 |           23,355 |            1.6 |               7.6 |
| devDependencies of each package's newest indexed version |               0.33 |    0.33 |           10 |        6,947 |           38,077 |            2.6 |                12 |
| bundleDependencies                                       |               0.00 |    0.00 |            0 |            0 |                0 |              0 |                 0 |
| **(d) same @scope** as an indexed package                |               1.41 |    1.41 |           12 |  >= 6,695 \* |                  |                |                   |
| same maintainer (any indexed package's)                  |               4.61 |    4.61 |           40 | >= 22,171 \* |                  |                |                   |
| ... maintainer of >= 3 / >= 10 / >= 30 indexed packages  | 3.72 / 1.70 / 1.42 |       = | 25 / 19 / 17 |              |                  |                |                   |
| **(e) vendored-dir names** in indexed packages' paths    |               0.06 |    0.06 |            1 |           39 |              214 |           0.01 |               0.1 |

\* Scope and maintainer populations have no registry listing; the counts are the members found inside the two
ranked lists only (a floor). Maintainers are read from `maintainers`, never `_npmUser`: 2,470 of the indexed newest
versions were published by the shared "GitHub Actions" provenance identity, which joined unrelated packages (an
early pass that read it put 57 of 70 miss packages "in" the policy). Thresholds at rank: downloads 16k 1.61 M,
25k 503 k, 35k 208 k, 50k 81.2 k, 100k 13.4 k a month; dependents 16k 84, 20k 61, 50k 18, 100k 8.

**What the census says.**

- **Download rank is the only policy that reaches the residual.** npm's last-month download top-50k holds 24.65 of
  the 24.86 points (58 of 70 packages); 11.75 points without the largest package. Inside it, the 0-25k band holds
  23.23 points (10.33 ex-top1) for 15,056 new packages; ranks 25k-50k add only 1.41 points for 23,930 more.
- **Why exp108's "outside npm's top-16k" did not mean "obscure".** exp095's top-16k (npm-high-impact, >= 1 M
  downloads a WEEK) is a downloads cut, but the index never held all of it: 7,316 of ecosyste.ms' top-16k are not
  indexed (4.47 points). The residual sits just past it: 16k-25k is 503 k to 1.6 M downloads a month.
- **Dependents count is the wrong axis.** 21.5 of the 24.86 points are in packages ecosyste.ms counts ZERO
  dependent packages for: application-level packages (SDKs, CLIs' internals, app frameworks) that apps install
  and other packages rarely declare. Even the top 100k by dependents reaches 2.89 points.
- **Closure, devDependencies, bundled deps and vendored paths are near zero** (<= 0.33): what an indexed package
  depends on is already indexed (k2's installs index the closure).
- **Same maintainer** reaches 4.61 points but the population is unbounded (every package of 7,338 maintainers);
  same scope 1.41.
- **Version density.** On the 0-25k population, the miss bytes at an exact indexed version: any version 23.24 /
  ex-top1 10.33; `year-last12+major4` 22.24 / 9.34; `year-last6+major2` 21.38 / 8.47; `year-last3+major1`
  21.23 / 8.32; `year-last1+major1` 19.27 / 6.37. (Structural matching also reaches neighbouring versions, so these
  are floors for what a density can match.)

`census-k2all.json` holds every number above (aggregates only).

## 2. The build: k2-all + dl25k

**Population.** The download top-25k (packages.ecosyste.ms, last-month downloads; rank 25k = 503 k a month),
minus every package k2-all indexes, minus `@types/*` (no runtime code), minus 95 packages k2's build already
tried, gives 13,785 new roots. Their pins are `year-last3+major1` (`tail_pins.py`): 28,196 versions. Section 1's
density table put this density at 21.23 of the 23.23 points (8.32 of 10.33 ex-top1), at a third of k2's density
cost.

**Build** (`scripts/build-dl25k.sh`, under `flock /work/index-build.lock`, cpus 34-63). This is an increment:

- the store is a hard-linked copy of exp113's, and every k2-all shard is reused (the dry run plans 379,323 reuses);
- 33,761 versions build: the 28,196 new ones plus exp113's 5,565 known-failing retries, kept so that the old package
  set is byte-for-byte k2-all's;
- the builder is exp113's binary (`humanify-index` is unchanged since `f568e5bc`);
- 195 chunks, every one exit 0, 47,560 s of shards (13.2 h) and a 2,066 s merge at a 61.9 GB peak;
- 11,354 of the new roots got at least one version indexed (the rest are install failures: ETARGET under
  `--before`, EBADPLATFORM);
- 12.7 GB, against 6.4 GB for k2-all.

The cost model under-estimated both size and time. Popular packages are big: 185 KB per new version against the
model's 69, and about 42 core-seconds per version against its 35.

**Disk.** A new version's work dir holds about 41 MB. 33.8k versions would have needed about 1.4 TB with 716 GB
free, so `scripts/prune_work.py` deletes each FINISHED chunk's work dirs. A chunk writes every shard to the store
before it exits, each version is in exactly one chunk, and the merge reads only the store.

**Lookup.** The bigger index costs nothing at lookup: the dev corpus took 161 s against 237 s, peak 2.0 GB both
times.

**Score.** Scorer 8, structural rule, main `8ba487df`'s defaults (no `--pipeline-set`). Before =
`*exp119-k2all-st*` (k2-all, the same defaults); after = `*exp118-dl25k*`. Values are P / R / F0.5 in %. test and
test2 are summaries only, and test2 was never diagnosed.

| set           | k2-all (before)       | k2-all-dl25k          | dF0.5 |    dP | wrong KB       |
| ------------- | --------------------- | --------------------- | ----: | ----: | -------------- |
| held-out 67   | 99.24 / 66.69 / 90.42 | 98.30 / 80.16 / 94.05 | +3.63 | -0.94 | 155 -> 420     |
| task-master   | 96.25 / 65.50 / 87.99 | 89.71 / 60.54 / 81.83 | -6.16 | -6.54 | 126 -> 342     |
| wrangler      | 95.99 / 18.53 / 52.28 | 99.37 / 20.92 / 56.79 | +4.51 | +3.38 | 5.7 -> 1.0     |
| Claude Code   | 100 / 81.90 / 95.77   | 100 / 82.46 / 95.92   | +0.15 |     0 | 0              |
| apps-esbuild  | 98.73 / 89.71 / 96.79 | 98.72 / 88.47 / 96.48 | -0.30 | -0.01 | 295 -> 294     |
| apps-webpack  | 98.61 / 72.75 / 92.06 | 98.68 / 74.52 / 92.67 | +0.60 | +0.07 | 765 -> 744     |
| apps-rollup   | 98.57 / 77.12 / 93.38 | 98.72 / 80.65 / 94.48 | +1.11 | +0.15 | 1,096 -> 1,025 |
| dev-webpack   | 99.64 / 62.89 / 89.22 | 99.64 / 62.63 / 89.11 | -0.11 |     0 | 4.1            |
| dev-rollup    | 99.87 / 77.53 / 94.43 | 99.87 / 77.35 / 94.37 | -0.06 |     0 | 2.1            |
| pop-bun       | 95.08 / 77.13 / 90.85 | 95.86 / 76.89 / 91.35 | +0.50 | +0.78 | 4,040 -> 3,361 |
| pop-esbuild   | 93.52 / 79.54 / 90.34 | 94.73 / 79.48 / 91.23 | +0.89 | +1.22 | 5,601 -> 4,489 |
| TEST bun      | 97.23 / 66.39 / 88.96 | 91.46 / 63.52 / 84.06 | -4.90 | -5.77 | 1,236 -> 3,873 |
| TEST esbuild  | 96.42 / 75.96 / 91.49 | 92.23 / 72.79 / 87.55 | -3.94 | -4.19 | 1,947 -> 4,229 |
| TEST webpack  | 99.19 / 70.51 / 91.73 | 97.95 / 74.02 / 92.00 | +0.27 | -1.24 | 865 -> 2,320   |
| TEST rollup   | 97.89 / 68.39 / 90.12 | 96.40 / 70.58 / 89.83 | -0.29 | -1.49 | 1,607 -> 2,877 |
| TEST2 bun     | 98.71 / 77.70 / 93.64 | 96.15 / 78.25 / 91.94 | -1.70 | -2.56 | 424 -> 1,303   |
| TEST2 esbuild | 99.41 / 74.21 / 93.09 | 96.85 / 73.80 / 91.15 | -1.94 | -2.56 | 191 -> 1,037   |
| TEST2 webpack | 99.14 / 69.22 / 91.25 | 99.25 / 71.30 / 92.04 | +0.79 | +0.12 | 1,030 -> 915   |
| TEST2 rollup  | 96.18 / 74.74 / 90.96 | 96.10 / 74.84 / 90.94 | -0.02 | -0.07 | 1,756 -> 1,794 |

App claimed is 0 on every set.

**What the held-out gain is made of** (dev, aggregates: `misses.py` on the new label, and a private per-package
pass over the census packages).

- Not-indexed falls from 24.86 to 1.63 points.
- The largest package (12.90 points) is now named in full.
- The other previously not-indexed packages held 11.96 points. Of those, 10.28 are still missed:
  - 3.99 hash differs, absent;
  - 2.05 too small;
  - 1.63 still not indexed (install failures, or a version outside the density);
  - 1.23 version missing;
  - 1.17 claimed by another package.
- Held-out recall without the largest package moves only about 76.6% -> 77.2%. Its precision falls from 99.2% to
  98.0%, so held-out F0.5 without the largest package goes DOWN, about 93.7 -> 93.0.

**The new wrong bytes** (dev held-out, exp101 `new_wrong.py`, 589 units, 339 KB, read in aggregate):

- 96% are SEEDS (`down` 3%, `module` 1%).
- 94% have a truth ORIGIN: a vendored copy whose origin is a different package, claimed by a package that inlines
  it (zod, supports-color, d3-shape, the AI SDK).
- One claimer is 87% of the bytes, and one app is 93%.

This is exp108's sponge class again, at scale. The download top-25k is full of packages that ship a pre-bundled
dist, and each of them is a new rival for every dependency it inlines. The test sets, whose apps pre-bundle more,
lose 2-6 precision points. Wrangler and pop gain precision: there the new packages are the real shippers of code
the old index named wrongly.

## Reproduce

```bash
D=/work/exp118-data; S=experiments/118-coverage-census/scripts; IX=/work/exp113-data/ix/k2-all
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
$PY -I $S/misses.py exp113-k2all                                    # private per-package misses (outside the repo)
python3 -I $S/fetch_ranks.py $D/eco 20 1000 dependent_packages_count # dependents top-20k
python3 -I $S/fetch_ranks.py $D/eco 500 100 1 downloads              # downloads top-50k (run page ranges in parallel)
python3 -c "import json; v=json.load(open('$IX/versions.json')); print('\n'.join(sorted(v)))" > $D/indexed-names.txt
#   indexed-newest.txt = <name>@<newest indexed version> per owner of versions.json
python3 -I $S/fetch_npm.py $D/npm abbr $D/indexed-names.txt 16       # every indexed package: all versions' deps
python3 -I $S/fetch_npm.py $D/npm ver $D/indexed-newest.txt 16       # newest indexed version: dev deps, maintainers
for L in 1 2 3 4 5 6 7; do python3 -I $S/populations.py $IX $D frontier $L > $D/frontier-$L.txt
  python3 -I $S/fetch_npm.py $D/npm abbr $D/frontier-$L.txt 24; done  # transitive closure, level by level
python3 -I $S/populations.py $IX $D build
# miss packages' own public metadata, for the maintainer/scope/threshold/density tests (private, outside the repo):
python3 -I $S/fetch_npm.py $D/private abbr $D/private-miss-names.txt 8; python3 -I $S/fetch_npm.py $D/private ver $D/private-miss-versions.txt 8
python3 -I experiments/101-index-coverage/scripts/fetch_packuments.py $D/private/registry $D/private-miss-names.txt
python3 -I $S/fetch_eco_pkgs.py $D/private/eco $D/private-miss-names.txt
# thresholds.json: the last row's count on the ranked list's page at rank 50k/100k (per_page=100, pages 500/1000)
python3 -I $S/census.py $D $D/misses-exp113-k2all.json $D/census-k2all.json $D/pop-dl25k.txt
# the build: population, pins, increment on k2-all, scoring (test/test2: summaries only)
python3 -I experiments/101-index-coverage/scripts/fetch_packuments.py $D/idx/registry $D/pop-dl25k.txt  # registry = cp -al of exp113's first
python3 -I experiments/101-index-coverage/scripts/tail_pins.py $D/idx/registry 2026-10-11 year-last3+major1 <new roots> $D/tail-dl25k.txt
cat /work/exp108-data/pins-k2.txt $D/tail-dl25k.txt > $D/pins-dl25k.txt
cp -al /work/exp113-data/store $D/idx/store
$S/build-dl25k.sh $D/idx $D/idx/bin/humanify-index-f568e5bc $D/pins-dl25k.txt &
python3 -I $S/prune_work.py $D/idx k2-all-dl25k $D/idx/build.out &
IX=$D/idx/ix/k2-all-dl25k; C="--index $IX --rule structural --jobs 4"
npm run eval -- library exp118-dl25k $C
for c in apps bundlers; do npm run eval -- library exp118-dl25k-$c $C --corpus $c; done
for c in pop test test2; do npm run eval -- library $c-exp118-dl25k $C --corpus $c; done
python3 -I $S/compare.py experiments/096-library-matching-score/results exp119-k2all-st exp118-dl25k  # etc.
```
