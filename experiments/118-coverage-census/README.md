# exp118: which generic package population reaches the held-out residual? (census, then one build)

## STATUS (2026-10-15)

IN PROGRESS on branch `exp118-coverage-census` (off main `0b2b136b`). Census DONE (section 1): **npm's
last-month download rank is the one generic policy that reaches the not-indexed residual** (top-50k: 24.65 of
24.86 points; top-25k: 23.23, 10.33 without the single largest package). Building `k2-all-dl25k` (k2-all + the
13,785 non-`@types` packages of the download top-25k it does not index, `year-last3+major1`, all five slices).

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

Running (section 1 chose the population; results to follow).

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
```
