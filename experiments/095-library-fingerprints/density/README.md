# exp095 lane "density": which versions of a package the library index must hold

## STATUS (2026-10-12)

- **Update 2026-10-13 (rule 9):** the builder fixes recommended below (bundled-only literal table, `exports`
  entries, one-file-one-module check) and the `year-all+minor` policy landed in `../indexv2/` (commit 47e3bd6d),
  which also built a real index and re-tested the claim rule. Read that README for current numbers.

- **State.** DONE (lab experiment). No pipeline change. Every number below comes from humanify-index shards and the
  scale lane's design rule (`../selection/scripts/libix.py`), Bun slice unless a row says esbuild.
- **Verdict, totals first.**
  - **Data:** 103 packages (58 that Claude Code bundles + 45 the selection lane's corpus bundles most), **every
    stable release**: 11,814 versions planned, 11,303 built by humanify-index (Bun 1.4.0 and esbuild 0.28.2, a
    `require` and an `import()` entry each, ~45,000 bundles), 511 refused by npm (`ETARGET` / `ENOVERSIONS` under
    `--before`). 101 packages produced CommonJS modules to query (41,769 distinct Bun query modules).
  - **The exact version matters a lot.** With every release indexed, the design rule names **86.4%** of a package's
    own CommonJS module bytes (download-weighted over versions; 83.6% uniform). Take the bundle's own version X
    out and keep only its nearest patch: **54.8%**; every other patch of X's minor: 57.5%; the release published
    just before X: 43.8%; other minors of X's major: 34.6%; other majors only: 11.9%. **About a third of recall
    needs X itself**, because a whole-module key breaks on any byte of change and many packages ship one big
    pre-built file per release (lodash, marked, commander, the AWS SDK's `dist-cjs/index.js`, axios).
  - **Claude Code's own packages are kinder.** 270 (package, CC version) pairs over 8 CC versions: the nearest
    patch keeps 85.0% and the same minor 87.2% of the all-versions recall; the release just before keeps 61.9%.
  - **Dedup is large.** All releases of the 101 packages: 1,260,610 literal rows and 389,037 module rows summed per
    version, **78,634 and 40,650 after identical rows collapse** (16× and 9.6×). Per package, the median release
    adds 13% of one release's literal rows.
  - **Recommended policy: `year-all+minor`** — every release of the last 12 months, plus the latest patch of every
    older minor. **83.0%** download-weighted recall (all releases: 86.7%) for **43%** of the versions and **60%** of
    the literal bytes (177.8 vs 294.8 MB raw for these 101 packages); on Claude Code **96.5%** of the all-versions
    recall. Today's as-of policy (latest, previous patch, major-1, major-2) gets 54.8% / 70.9% at 28 MB.
  - **Wrong names stay near zero at every density** (≤ 0.31% of bytes, uniform; ≤ 0.08% download-weighted), rising
    as density falls: with the right version missing, a copy elsewhere can win.

## Method

**Packages** (`cc-set.txt`, `a-set.txt` in `$DEN`; lists in the scripts' docstrings): the CJS packages Claude Code
2.1.216 vendors (by factory bytes) and its `src/` libraries (negatives lane), plus the 45 packages with the most
apps × median bytes in the selection corpus (i18next dropped: 584 releases).

**Versions** (`scripts/plan.py`): every registry version that is plain `x.y.z` with a publish time, passed to
humanify-index as pins (`build --as-of 2026-10-11 --slices bun,esbuild`, `scripts/build-group.sh`, 4–5 processes
× 4 jobs pinned to 4 cores each through `scripts/queue.py`). Per-version weights: npm's last-week downloads per
version (`api.npmjs.org/versions/<pkg>/last-week`).

**Queries** (`scripts/queries.py`): every build of every version, fingerprinted by `../selection/fpq` (the pipeline's
`bundle_content_keys` + `libindex::literals` in Bundled mode). Only modules owned by the root package at the root's
version (by the build's sourcemap) are kept, deduplicated over the two entries. Truth is exact.

**Index under a policy** (`scripts/evaluate.py`): a policy keeps, per package p, a version set K. The index then
holds p's module and literal rows whose version sets meet K — rows identical across versions are one row, as
humanify-index merges them. Rows of every other owner (dependencies, and for the Bun slice the selection lane's
download-20 index minus the 103 packages) are held at all versions: they are the competition (IDF, margins, keys
held by two families). The rule: whole-module key held by one family AND a literal claim (specific files, 20 bits,
cover 0.9, margin 10) for the same family. Each distinct query is scored once against all rows; a policy then reads
p's best score over rows that meet K.

**Policies**: `all`; `patch/N` (every Nth patch per minor, counted back from the minor's latest); `minor` (latest
patch of every minor); `year-all+minor`; `minor-last2+major` (every minor of the two newest major lines, the latest of
older majors); `major` (latest of every major; for 0.x every minor, as humanify-index does); `asof-default`
(humanify-index's as-of policy today); `latest`. **Scenarios** for a query version X (X itself never indexed):
nearest patch (closest other patch of X's minor, earlier first), same minor, latest-before (published just before
X), same major (other minors), other majors.

**Claude Code** (`scripts/cc_declared.py`, `evaluate.py` cc mode): the CJS factories of 8 CC versions (2.1.86 …
2.1.216, `fpq` without a map). Pass 1 names each with every release indexed; a module named a density package p is
a query of p. X = the declared version where the bundle inlines p's package.json (34 pairs: gaxios, google-auth-
library, @grpc/grpc-js, openid-client, @aws-sdk/nested-clients), else **inferred** = the version whose module rows
hold the most of those modules' keys (ties → latest). Recall is relative to the all-versions recall.

## Results

### 1. Policies: size and recall (Bun slice; esbuild below)

| policy                 | versions kept | module rows | literal rows | literal MB (raw JSON) | rows if not deduped (module / literal) | recall, uniform over versions | recall, download-weighted | wrong (uniform / dl) | Claude Code (rel. to all) |
| ---------------------- | ------------- | ----------- | ------------ | --------------------- | -------------------------------------- | ----------------------------- | ------------------------- | -------------------- | ------------------------- |
| all                    | 11,535        | 40,650      | 78,634       | 294.8                 | 389,037 / 1,260,610                    | 83.9%                         | 86.7%                     | 0.00% / 0.00%        | 100%                      |
| patch/2                | 7,237         | 37,142      | 68,597       | 222.3                 | 249,792 / 806,847                      | 73.7%                         | 83.1%                     | 0.00% / 0.00%        | 98.8%                     |
| patch/5                | 4,933         | 33,172      | 60,937       | 171.2                 | 180,920 / 591,278                      | 63.5%                         | 79.6%                     | 0.00% / 0.00%        | 94.1%                     |
| **year-all+minor**     | 4,964         | 31,764      | 59,225       | 177.8                 | 178,819 / 579,823                      | 58.2%                         | **83.0%**                 | 0.00% / 0.00%        | **96.5%**                 |
| minor                  | 4,187         | 30,697      | 56,431       | 148.9                 | 162,393 / 528,608                      | 55.7%                         | 76.9%                     | 0.00% / 0.00%        | 89.4%                     |
| minor-last2+major      | 2,901         | 23,612      | 45,363       | 115.5                 | 104,190 / 371,832                      | 43.7%                         | 76.0%                     | 0.04% / 0.00%        | 88.4%                     |
| major                  | 1,239         | 15,297      | 30,865       | 52.2                  | 34,425 / 81,973                        | 27.9%                         | 63.0%                     | 0.04% / 0.00%        | 69.6%                     |
| asof-default (today's) | 349           | 7,994       | 18,988       | 28.3                  | 11,373 / 38,960                        | 14.0%                         | 54.8%                     | 0.24% / 0.03%        | 70.9%                     |
| latest                 | 101           | 3,595       | 9,985        | 13.4                  | 3,601 / 14,347                         | 4.9%                          | 27.8%                     | 0.31% / 0.08%        | 41.5%                     |

- Rows and MB count the density packages' OWN rows (owner = the package). "Not deduped" = the sum over kept
  versions of each version's own rows. Recall is the mean over packages of the per-package mean over versions
  (bytes-weighted within a version); download-weighted weights each version by its last-week downloads, which is
  what bundles in the wild use. Uniform weighting counts a 2014 release as much as last month's.
- **esbuild slice** (no background index; literals are the same rows): all 89.1%, patch/2 85.3%, patch/5 81.9%,
  year-all+minor 85.0%, minor 79.4%, minor-last2+major 78.5%, major 64.2%, asof-default 55.3%, latest 25.9%
  (download-weighted). Module rows: all 42,860, minor 32,471, asof-default 8,170. Same shape as Bun.

### 2. Is the exact version needed? (X never indexed)

| scenario                       | packages | Bun: uniform / download-weighted | esbuild: dl-weighted | Claude Code pairs | CC recall (rel. to all) |
| ------------------------------ | -------- | -------------------------------- | -------------------- | ----------------- | ----------------------- |
| exact (only X)                 | 101      | 83.6% / 86.4%                    | 88.7%                | 270               | 97.6%                   |
| nearest patch                  | 101      | 47.5% / 54.8%                    | 55.2%                | 210               | 85.0%                   |
| same minor (all other patches) | 101      | 53.1% / 57.5%                    | 58.2%                | 210               | 87.2%                   |
| latest-before                  | 101      | 39.6% / 43.8%                    | 43.7%                | 270               | 61.9%                   |
| same major, other minors       | 93       | 35.0% / 34.6%                    | 36.4%                | 217               | 75.8%                   |
| other majors only              | 97       | 12.5% / 11.9%                    | 13.5%                | 254               | 22.1%                   |

- **Where a neighbour patch fails** (nearest-patch recall, uniform): dom-serializer, esprima, jquery 0%; async,
  katex, source-map 4%; @azure/msal-common 8%; lodash, @babel/parser, sax 16%. These ship pre-built files: one
  change anywhere changes the file's key (msal-common's CJS build is one big `lib/index-node-<hash>.js` chunk per
  release; its two small re-export files keep their keys). Multi-file packages keep most
  modules: @anthropic-ai/sdk shares 192–198 of 195–199 module keys with its previous release.
- **Claude Code's declared pairs**: gaxios 6.7.1 / 7.1.4 and google-auth-library 9.15.1 / 10.6.2 are fully named
  under `minor`; @grpc/grpc-js 1.14.0 / 1.14.4 keeps 56–67% under `minor` and 52–69% under `asof-default` (its
  modules change across patches); @aws-sdk/nested-clients 3.936.0 is 100% under `minor` and **0% under
  `asof-default`** (a monthly-ish release train: today's 4 picks are all newer).

### 3. Growth per package

Over the 78 packages with rows in their latest release: all releases hold a median **13.1×** the latest release's
literal rows (quartiles 8.4–26.7×) and 16.9× its module rows (9.3–39.0×), for a median of 92 releases; each
extra release adds a median 13% of a release's literal rows. Without deduplication the median package would hold
5.5× more literal rows and 3.8× more module rows. react is the extreme (139 releases, 128× the latest release's
literal rows).

## Recommendation (density policy)

**`year-all+minor`: every release published in the 12 months before the as-of date, plus the latest patch of
every older minor.** Measured on 101 packages: 83.0% of the all-releases recall's 86.7% (download-weighted), 96.5%
of it on Claude Code's packages, at 43% of the releases and 60% of the literal bytes; 0 wrong names. Cost relative
to today's as-of policy: 6.3× the literal bytes (177.8 vs 28.3 MB raw for these packages) for +28 points of
download-weighted recall. If that is too large, `minor` (76.9%, 149 MB) is the next step down;
`minor-last2+major` (76.0%, 116 MB) keeps almost all of `minor`'s recall for 22% less. Pinned versions from the
bundle's own readers (declared package.json, VERSION constants) stay on top of any policy: they are the only way
to get X when X is older than the dense window.

Two things shrink every row above and are worth doing first (selection README): restricting the literal shard to
files a bundle contains (~2–3×), and the builder entering subpath-only packages through their `exports` map —
**@modelcontextprotocol/sdk and @aws-sdk/nested-clients produced no modules at all** (no `.` export, so the
recipe's `require("<pkg>")` entry fails), and MCP SDK is one of Claude Code's largest `src/` libraries.

## Re-run

```bash
export DEN=/work/exp095-library-fingerprints/density SEL=/work/exp095-library-fingerprints/selection
S=experiments/095-library-fingerprints/density/scripts V=/work/exp095-library-fingerprints/scale/.venv/bin/python
python3 -I $S/plan.py $DEN/cc-set.txt --groups 5            # versions.json, groups/g*.txt (then a-set.txt into aset/, merged)
humanify-index build --packages <all pins> ... --dry-run    # fetch packuments once (build-group.sh runs --offline)
python3 -I $S/queue.py --slot 44-47 --slot 48-51 --slot 52-55 --slot 56-59 -- $DEN/groups/*.txt
python3 -I $S/queries.py --threads 16                       # q/<slice>/<pkg@version>.jsonl
python3 -I $S/cc_declared.py $DEN/cc /Users/andrewgross/Development/claude-code-versions/inputs   # after fpq on cc/jobs.tsv
$V -I $S/evaluate.py --slice bun --background $SEL/ix/download-20 && $V -I $S/evaluate.py --slice esbuild
python3 -I $S/report.py bun && python3 -I $S/report.py esbuild
```

## Files (bulk data in `/work/exp095-library-fingerprints/density/`, not in git)

`versions.json`, `groups/`, `store/shards/{bun,esbuild,literals}/` (11,303 versions; 2.6 GB), `work/` (installs and
builds), `q/` (query modules), `cc/` (CC factory fingerprints, `declared.json`), `results/density-{bun,esbuild,cc}.json`.

## Unknown / not done

- Only CommonJS modules are queried: ESM-only packages (execa ≥ 6, lodash-es, chalk ≥ 5) and lazy ESM have no
  module key yet, so they are absent from the recall columns; the literal rows still count in the sizes.
- The 12-month window is one choice; 6 or 24 months were not run.
- Other owners are held at all versions in every policy (more competition than a sparse index would have); the
  wrong-name column is therefore an upper bound.
- CC versions are inferred for 236 of 270 pairs (from module keys, so the inference itself prefers versions whose
  keys match); the 34 declared pairs are the exact test.
- 511 versions failed to install with `npm --before` (ETARGET: a dependency range with no version published by
  then); they are missing from every policy.
