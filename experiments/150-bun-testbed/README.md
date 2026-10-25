# exp150: the Bun test bed (library-names plan, step 0)

## STATUS (2026-10-25)

DONE on branch `exp150-bun-testbed` (off main `2a24ae54e`). **Harness only: no crates change, no key, no default
flipped.** This rebuilds what `docs/design/library-names-plan.md` step 0 asks for, after the 2026-10-25 cleanup
deleted every corpus and Claude Code's truth labels: two Bun corpora with source-map truth (`--corpus bun-dev`,
`--corpus bun-held`), Claude Code 2.1.216's labelled sample (`--corpus cc`), and the baseline of main's matcher on
all three. Every later step of the plan is judged against the baseline below.

### Totals

| corpus     | apps | bundles (target node + target bun) | bundle MB | library MB (truth) | role                                             |
| ---------- | ---: | ---------------------------------: | --------: | -----------------: | ------------------------------------------------ |
| `bun-dev`  |   45 |                       90 (45 + 45) |     243.3 |              214.3 | dev: cards committed                             |
| `bun-held` |   15 |                       30 (15 + 15) |      66.0 |               54.2 | held out: summaries only, private                |
| `cc`       |    1 |                    1 (Claude Code) |      20.5 |    2.22 (estimate) | 620-unit labelled sample, truth outside the repo |

### Baseline: main's defaults (binary `2a24ae54e`, index `/work/library-index/current` = `c4d2bd6c...`, rule structural, scorer 14)

| label                  | set           |      P |     R |   F0.5 | named MB | wrong-family KB | app claimed B |
| ---------------------- | ------------- | -----: | ----: | -----: | -------: | --------------: | ------------: |
| `bun-dev-exp150-main`  | bun-dev-node  | 99.743 | 82.96 | 95.863 |    90.90 |           234.2 |             0 |
|                        | bun-dev-bun   | 99.825 | 81.53 | 95.539 |    85.39 |           149.9 |             0 |
| `bun-held-exp150-main` | bun-held-node | 99.708 | 81.09 | 95.329 |    22.75 |            66.7 |             0 |
|                        | bun-held-bun  | 99.973 | 83.68 | 96.227 |    21.86 |             5.9 |             0 |
| `cc-exp150-main`       | cc            | 100.00 | 81.90 | 95.768 |     1.82 |             0.0 |             0 |

(cc bytes are the stratified estimate over src/'s 15.28 MB of statement units; 0 of 620 sample units app-claimed.)
Precision is at or above the 99.8% bar on bun-dev-bun, bun-held-bun and cc, and just under it on both target-node
sets (99.74 / 99.71). App claimed is 0 everywhere. bun-dev's largest miss classes (share of library bytes): `a` not
indexed 3.5-3.6%, `c` too small 2.8-3.8%, `e-veto` 2.6-3.4%, `e-other` 1.6-3.2%, `e-ambig` 1.5-1.6%.

## The Bun corpora (`scripts/bun_corpus.py`)

- **Pool:** exp108's TRAINING apps as committed (`experiments/108-app-corpus/corpus.json`: its selection, so no
  re-query of npm or GitHub), kind cli / electron / vscode, library closure 0 < libBytes <= 15 MB, minus every app of
  every committed corpus manifest (550 slugs: dev, test, test2, apps, pop, bundlers, remin, remin2-4) and every exp100
  test candidate. Pool 1,088 (cli 208, electron 654, vscode 226).
- **Draw:** `random.Random(150)`, 20 apps per kind = 60. **Split:** per kind, 5 of the 20 drawn with
  `random.Random(1501)` are held out: dev 45 (15 per kind), held 15 (5 per kind). A failed build would be replaced by
  the plan generator continued, in the failed app's split (`replace`, exp116's rule); none failed: 60/60 apps built
  on both targets.
- **Install:** exp095's `closure.install`, i.e. `npm install --ignore-scripts --omit=dev --legacy-peer-deps`, plus
  `--before=2026-10-15T23:59:59.999Z`, the installed index's as-of (library-index.md §10's as-of policy: the corpus
  never asks for a version the index could not hold). A CLI installs exp108's recorded version; an Electron app or
  VS Code extension installs its runtime dependencies (exp108's `runtime_deps` rule) read from its root package.json
  at exp108's recorded default branch, fetched today (exp108 kept no copy, and its lock files were deleted).
- **Build:** exp100's `build_bun` (exp095 `closure.bundle`: externals, loaders and the top-level-await fallback),
  `bun build --target node|bun --format cjs --minify --sourcemap=external --define process.env.NODE_ENV="production"`.
  `closure.bundle` and `build_bun` gained a `target` parameter (default `node`: every earlier caller unchanged).
- **Truth:** exp096 `corpus.py`'s source-map truth, then every scorer-14 layer, built the way exp140 built remin2's
  (`scripts/layers.sh`, then `scripts/layers2.sh` after the first score attempt was refused for the missing copies
  layer): origins, chained, origins7 (rules off: 90/90 and 30/30 equal), chained9, origins10 (90/90, 30/30), chained11,
  origins12 (patched off: 90/90, 30/30), copies2 (scorer 13's and 14's rules off), misattr13, mapdefect14. The
  claim-blind layers take their units from the `*-exp150-main` run.
- **One recipe difference, stated:** copies2's evidence rows come from `/work/library-index/current` only (exp140 read
  step4-wr, k2-all-r5h and k2-all-r5-origin-h, all deleted).
- `selection.json`: the draw, split, Bun provenance and per-app build status.

### Bun 1.4.0 provenance

The shared copy `/work/tools/bun-1.4.0` (npm `bun@1.4.0` with `@oven/bun-linux-x64@1.4.0`, integrity
`sha512-Du44zebt...MTbZIg==`), `bun --revision` `1.4.0+34cbb9a40`, binary sha256 `33d56b07...17eb4b`. The pinned
GitHub release `bun-v1.4.0/bun-linux-x64.zip` (sha256 `2d03fb5f...2fe452`, equal to the release's `SHASUMS256.txt`)
was downloaded to `/work/exp150-data/tools/bun-release/`: its binary has the SAME sha256 `33d56b07...17eb4b`.

## Claude Code truth (`--corpus cc`)

- **Redraw:** exp095's `population.py` and `sample.py` (seed 95) over the units of `cc_scan.mjs` (acorn 8.19.0; the
  lab's real-world labels it also read are gone, and are not needed for the units). It reproduces the README exactly:
  population 73,414 units / 15,280,719 B (CJS factories 5,052,733 B, excluded), and per stratum N / bytes / n
  S0 56,859 / 834,509 / 150, S1 13,785 / 4,377,939 / 180, S2 2,608 / 6,591,066 / 180, S3 162 / 3,477,205 / 110. The
  units the README names as in the sample (28561, 25111 at 3355959, 36572) are in it; those it names as traps or queue
  entries are not.
- **Labels:** the README's conventions (`app` with forks, `library:<pkg>`, `compiler-helper`, `bundler-runtime`;
  sandbox-runtime and mcpb are libraries, claude-agent-sdk is the app), aided by `scripts/cc_evidence.py` (which
  reference packages hold a unit's long literals; never the index). 620 units: library 108, app 511, compiler-helper 1,
  per stratum S0 36 / 114 / 0, S1 33 / 146 / 1, S2 28 / 152 / 0, S3 11 / 99 / 0: the README's counts exactly.
  Library share 14.54% (95% CI 11.24-18.11%, 2.22 MB), the README's 14.5% (11.2-18.1%). Family estimates equal the
  README's (AWS + smithy 560 KB, zod 384, @anthropic-ai/_ 273, @azure/_ 225, MCP SDK 148, ...). App forks 1.7% (README
  3.5%: the fork column is informational and labelled less often here; it does not enter any rate).
- **Blind second pass:** 124 units (20%, `scripts/blind_draw.py`, seed 9520), labelled without the first pass: class
  agreement **124/124**, package agreement **23/24** where both said library (one `@smithy/smithy-client` vs
  `@smithy/core`, one family).
- The labels, units, evidence and the frozen truth stay in `/work/cc-library-truth/` (never committed); this repo
  holds `data/cc-rates.json` (rates and package names only) and `corpus-cc.json` (hashes). `corpus.py`'s `sample`
  truth reads `/work/cc-library-truth/2.1.216` (`freeze --corpus cc`).
- **Newer version:** none. `claude-code-versions/inputs` ends at 2.1.216.

## Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python   # recreated: python3 -m venv + numpy scipy orjson
S=experiments/150-bun-testbed/scripts
python3 -I $S/bun_corpus.py plan && flock /work/heavy.lock python3 -I $S/bun_corpus.py build --jobs 6
$PY -I $S/bun_corpus.py freeze --corpus bun-dev --jobs 16 && $PY -I $S/bun_corpus.py freeze --corpus bun-held --jobs 16
$S/layers.sh bun-dev && $S/layers.sh bun-held
# Claude Code: units, sample, evidence (all under /work/cc-library-truth/)
N=/work/cc-library-truth/2.1.216; X=experiments/095-library-fingerprints
ACORN=/work/exp150-data/tools/acorn/node_modules/acorn CC_UNITS_OUT=/work/cc-library-truth/units \
  node --max-old-space-size=30000 $X/bundler-variation-and-boundaries/scripts/cc_scan.mjs 2.1.216
(cd $N && CC_RAW=/work/cc-library-truth/units/min/cc-2.1.216/raw.jsonl NEG_DATA=$N/data python3 -I $X/negatives/scripts/population.py \
  && python3 -I $X/negatives/scripts/sample.py)
python3 -I $S/cc_evidence.py $N <bundle> /work/exp150-data/ccref/node_modules && python3 -I $S/blind_draw.py $N
# label (pass1/*.tsv -> data/pass1.tsv; blind/pass2-*.tsv), then (cd $N && python3 -I $X/negatives/scripts/emit_labels.py)
$PY -I experiments/096-library-matching-score/corpus.py freeze --corpus cc && $S/layers.sh cc
C="--index /work/library-index/current --rule structural --commit 2a24ae54e"
npm run eval -- library bun-dev-exp150-main $C --corpus bun-dev     # refused: no copies layer yet
$S/layers2.sh bun-dev bun-dev-exp150-main && npm run eval -- library bun-dev-exp150-main $C --corpus bun-dev
# the same for cc; bun-held: eval (refused), then layers2.sh, whose score.py is its one score
```

The deleted scorer caches the layer scripts read at fixed paths are symlinks into `/work/exp150-data/caches/`
(`/work/exp104-data/registry`, `/work/exp112-data/{registry,bundled}`) and `/work/exp105-data/tools/node_modules` ->
acorn 8.19.0, refilled from the npm registry by the scripts themselves.

## Gate

`npm run check` bare on this branch at the committed tree: **ALL 13 STAGES PASSED** (log `/work/exp150-data/gate.log`). The `--corpus bun-dev|bun-held|cc` argument test was watched red,
then green. No crates change, so no neutrality run.
