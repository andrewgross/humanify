# exp095 / bundler-variation-and-boundaries

Two questions (the write-up was returned to the orchestrating workflow; every number in it is in `results/`):

1. **Bundler variation.** How does the SAME library module differ across Bun 1.3.14 / 1.4.0 / 1.4.3, esbuild 0.28.2
   and rollup+terser, across entry modes (lazy vs eager vs tree-shaken) and across library versions? Which
   normalisations make the fingerprints converge?
2. **Step B.** Can library code be found in a bundle where there is no module boundary (lazy / eager ES modules), and
   how often is app code wrongly called library?

Everything reads the shared lab (`../lab/`, read-only) and Claude Code's raw bundles. Nothing in the humanify repo is
touched.

## Layout

```
scripts/
  canon.mjs          canonical serializer: levels L1..L3 (+ masked L4), 39 named, switchable normalisations
  selftest.mjs       20 hand cases that must hash equal at L3
  scan.mjs           fingerprints every unit (top-level statement / declarator / statement inside a module wrapper)
                     and every function of every minified lab build, with its sourcemap truth label
  load.py            per-module summaries of the scan output
  variation.py       Part 1 table: module / function / statement match rates per bundler pair, mode pair, version step
  ablate.mjs         Part 1 transform catalogue: matches lost when ONE normalisation is switched off; syntax counts
  residual.mjs       + residual_diff.py: what still differs after normalisation (clustered diff hunks with examples)
  stepb.py           Step B detector (index, evidence, growth, scoring)
  stepb_run.py       Step B grid on lab combo bundles, incl. leave-one-package-out
  stepb_final.py     the candidate final configurations, lab + Claude Code
  stepb_inspect.py   lists false positives / leave-one-out confusions with code
  window.mjs         sliding-window baseline: winnowed lexical k-grams (k=20, w=8)
  window_stepb.py    sliding-window detector on lab combos;  cc_window.mjs + cc_window_stepb.py on Claude Code
  cc_scan.mjs        Claude Code raw bundle -> units, with the lab's positives/negatives as truth
  cc_stepb.py        Step B on Claude Code (negatives, positives, census by region, string corroboration)
  cc_exact.py        Step B on Claude Code with the exact library versions it ships, MSAL banner ground truth
  index_size.py      how many fingerprints an index needs (per package-version, per bundler, version dedup)
exactver/            a mini-lab with Claude Code's exact versions (msal-node 3.8.1 + msal-common 15.13.1,
                     axios 1.15.2, zod 3.25.76 incl. zod/v4), built with Bun 1.4.0 and labelled by a copy of the lab's
                     labeler (exactver/build.sh; exactver/tools -> ../lab/tools, read-only)
data/                scan output (units/, windows/), logs, residual pairs
results/             every number in REPORT.md (json), stepb-cc-examples.txt (flagged code samples)
```

## Re-run (exact commands, from this folder)

```bash
cd /work/exp095-library-fingerprints/bundler-variation-and-boundaries
node scripts/selftest.mjs                                   # 20 cases, all "h3:SAME"
node scripts/scan.mjs --jobs 56                             # ~30 s, data/units/<bundler>/min/<app>/<mode>.jsonl
for v in 2.1.216 2.1.198; do node --max-old-space-size=30000 scripts/cc_scan.mjs $v; done
exactver/build.sh                                           # npm installs + Bun 1.4.0 builds + labels
LAB_ROOT=$PWD/exactver UNITS_OUT=$PWD/data/units/exactver node scripts/scan.mjs --jobs 12
node scripts/window.mjs --jobs 40 && node scripts/cc_window.mjs 2.1.216

python3 scripts/variation.py                                # results/variation.json
A="node --max-old-space-size=16000 scripts/ablate.mjs"
$A bun-1.3.14 bun-1.4.0 1500 results/ablation/bun1314-bun140.json & $A bun-1.4.0 bun-1.4.3 1500 results/ablation/bun140-bun143.json &
$A bun-1.4.0 esbuild-0.28.2 1500 results/ablation/bun140-esbuild.json & $A bun-1.4.0 rollup-4.64.5 1500 results/ablation/bun140-rollup.json &
$A bun-1.4.0:req bun-1.4.0:imp 1500 results/ablation/bun140-lazy-eager.json & wait
node --max-old-space-size=16000 scripts/residual.mjs bun-1.4.0 esbuild-0.28.2 600 data/residual/bun140-esbuild.jsonl
python3 scripts/residual_diff.py data/residual/bun140-esbuild.jsonl 30

export PYTHONHASHSEED=0                                     # determinism (set iteration order)
python3 scripts/stepb_run.py      > data/stepb-lab.log      # ~25 min
python3 scripts/stepb_final.py                              # ~15 min
python3 scripts/cc_stepb.py       > data/stepb-cc.log
python3 scripts/cc_exact.py
python3 scripts/window_stepb.py && python3 scripts/cc_window_stepb.py
python3 scripts/index_size.py
```

Notes: never wait on `pgrep -f` / `pkill -f` patterns (they match their own shell; one killed my shell here). All
Python runs need `PYTHONHASHSEED=0` for bit-identical tie-breaks.
