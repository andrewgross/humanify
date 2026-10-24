# exp146: remin4, a third re-minified dev corpus (144 apps)

## STATUS (2026-10-24)

DONE on branch `exp146-remin4` (off main `27b569ea9`). **Harness only: no crates change, no key, no default
flipped, nothing merged.** remin4 is a DEV corpus (cards committed). The judges (test, test2, remin2) were not
scored or opened: only their manifests' entry ids were read, to keep their apps out of remin4.

- **Corpus: `--corpus remin4`**, exp126's recipe with no changes (`dev_remin.py --corpus remin4`). Seed 146,
  48 apps per kind. **144 apps (48 cli / 48 electron / 48 vscode), 288 bundles**: remin4-terser 144 (536.5 MB of
  bundle, 501.3 MB of library) and remin4-esbuild 144 (559.8 MB, 527.8 MB). That is twice remin3 and four times
  remin or remin2.
- **Overlap: none.** The pool is exp126's (exp108 TRAINING apps of kind cli, electron or vscode, a good closure,
  library closure <= 15 MB). Every app of every other corpus manifest is removed by slug, the way remin3 did it, with
  remin3's 72 removed as well: dev (`corpus.json`), test, test2, apps, pop, bundlers, remin, remin2, remin3. Held-out
  and the test split's candidates are removed by id. Pool 1,232 (remin3's 1,304 minus its own 72). A check after the
  draw found 0 shared apps with each of the nine other manifests, 0 with the test split and 0 with held-out
  (`selection.json`).
- One Electron app's stage 1 failed: esbuild could not resolve a dayjs plugin `require` inside a published `.min`
  entry. Its replacement was drawn by the seeded generator, continued past the draw (`dev_remin.py replace`, exp116's
  rule). 458 package entries were swapped for published minified ones in 97 of the 144 apps (32.4 MB).
- **Truth: every scorer-14 layer**, built the way exp140 and exp141 built them: origins, chained, origins7 (rules off
  give 288/288 equal), chained9, origins10 (288/288), chained11, origins12 (288/288), copies2 (with scorer 13's and
  14's rules off), misattr13 and mapdefect14. The claim-blind layers take their units from `remin4-exp142-asof`. The
  first score attempt was refused because the copies layer was missing, as for remin2 and remin3.
- **Scored (scorer 14, index `k2-all-r5-origin-prod-asof20261015`, rule structural):**

| label                                                                    | set            |      P |     R |   F0.5 | named KB | wrong KB |  app B |
| ------------------------------------------------------------------------ | -------------- | -----: | ----: | -----: | -------: | -------: | -----: |
| `remin4-exp142-asof` (main defaults, binary `673c72cc0` = main's crates) | remin4-terser  | 99.598 | 17.59 | 51.547 |   88,198 |    356.0 |    248 |
|                                                                          | remin4-esbuild | 99.401 | 46.37 | 80.896 |  244,718 |  1,420.8 | 54,957 |
| `remin4-exp144-shown2` (`8592e0e9d`, `veto_support = shown`)             | remin4-terser  | 99.611 | 18.21 | 52.587 |   91,269 |    356.0 |    248 |
|                                                                          | remin4-esbuild | 99.425 | 48.38 | 82.102 |  255,355 |  1,422.1 | 55,549 |

exp144 on remin4: terser **P +0.013, F0.5 +1.04**, 3.1 MB more named, **0 new wrong bytes** (unit level: 3.11 MB
newly right, 0.16 MB right lost to unclaimed, 0 newly wrong). esbuild **P +0.024, F0.5 +1.21**, 9.7 MB newly right,
**1.88 KB newly wrong in 17 bundles**. The largest single-bundle wrong gains (all remin4-esbuild): 592 B
`ag-psd` (app claimed: one app's own code), 237 B (`@aws-amplify/datastore` + `@aws-amplify/analytics`), 209 B
`@elevenlabs/elevenlabs-js`, 80 B `@inversifyjs/common`, then three 74 B `cockatiel` claims. Nothing on remin4 is
above 0.6 KB.

- Main's own precision is lower on remin4 than on remin3 (terser 99.598 vs 99.849, esbuild 99.401 vs 99.679). This
  experiment did not diagnose it.
- **exp145's terser bar on exp144, with remin4 added (`scripts/bar146.py` -> `data/bar.json`):**

| terser pool                    | apps | P_base | dP pts | p05 of 36-app dP | check 1 | T36 KB | largest dev bundle gain KB | check 2 | dev bundles >= T36 | P(fail, 36 bundles) | bar    |
| ------------------------------ | ---: | -----: | -----: | ---------------: | ------- | -----: | -------------------------: | ------- | -----------------: | ------------------: | ------ |
| remin + remin3 (exp145's pool) |  108 | 99.873 | +0.005 |          +0.0012 | pass    |   0.90 |             8.35 (pop-esb) | fail    |            9 / 789 |               0.338 | reject |
| remin4 alone                   |  144 | 99.598 | +0.013 |          +0.0026 | pass    |   3.09 |                       8.35 | fail    |            1 / 789 |               0.045 | reject |
| remin + remin3 + remin4        |  252 | 99.701 | +0.010 |          +0.0018 | pass    |   2.21 |                       8.35 | fail    |            2 / 789 |               0.087 | reject |

**Verdict: the bar still REJECTS exp144.** Check 1 passes on every pool (not one of 20,000 36-app draws has dP
< 0). Check 2 fails on every pool because of exp144's 8.35 KB pop-esbuild event. remin4 does not add a larger one
(max 0.59 KB). What remin4 changes is the tolerance: its main baseline is less precise, so T36 is 3.1 KB (2.2 KB
pooled) instead of 0.9 KB. Only 1-2 of 789 dev bundles reach that, so the estimated chance that one event fails a
36-app terser set drops from about 34% to 5-9%. A 144-app terser set like remin4 can absorb
(1 - 0.99598) x 3,071 KB = **12.3 KB**, close to the 13.6 KB the remin2 judge lost.

- **What it does and does not show.** remin4 is 4x the judge's app count. exp144 added **zero** wrong bytes on all
  252 terser dev apps (remin + remin3 + remin4). So remin2's +13.6 KB is a mechanism that is absent from three
  independent dev draws, not a frequent one: it was found once, in 36 apps. It is still the bar's rule that one
  8.35 KB dev event, larger than a 36-app tolerance, rejects the change. Whether to read the remin2 result as
  advisory for a change this small is Andrew's call (exp145 section 5).

## Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python; S=experiments/126-reminified/scripts
python3 -I $S/dev_remin.py plan --corpus remin4 && python3 -I $S/dev_remin.py build --jobs 12 --corpus remin4
python3 -I $S/dev_remin.py replace --corpus remin4 && python3 -I $S/dev_remin.py build --jobs 12 --corpus remin4
/work/exp146-data/layers.sh      # freeze + origins/chained/origins7/chained9/origins10/chained11/origins12 (exp140's list)
C="--index /work/exp142-data/full/ix/k2-all-r5-origin-prod-asof20261015 --rule structural --corpus remin4"
npm run eval -- library remin4-exp142-asof $C --commit 673c72cc0       # first score refused: no copies layer yet
/work/exp146-data/layers2.sh     # copies2 (rules 13/14 off), misattr13, mapdefect14 from remin4-exp142-asof; score
npm run eval -- library remin4-exp144-shown2 $C --commit 8592e0e9d --pipeline-set library.discovery.veto_support=shown
$PY -I experiments/133-dl25k-r5/scripts/judge13.py remin4 /work/exp146-data/judge/remin4-144.jsonl remin4-exp142-asof remin4-exp144-shown2
python3 experiments/145-terser-predictivity/scripts/delta145.py /work/exp146-data/judge/remin4-144.jsonl
python3 experiments/146-remin4/scripts/bar146.py experiments/146-remin4/data/bar.json   # exp144's dev labels from /work/exp144-veto
```

`673c72cc0` and `27b569ea9` have the same `crates/` (no crates commit between them). That makes the prebuilt
`673c72cc0` binary main's defaults. It is also the binary exp144's `*-exp142-asof` bases were scored with.

## Gate

`npm run check` bare on this branch at the committed tree: **ALL 13 STAGES PASSED** (typecheck, lint, rust:fmt,
rust:clippy, knip, knip:prod, census:clones, census:concurrency, unit, rust:unit, rust:build, rust:format-golden,
e2e; log `/work/exp146-data/gate.log`). The new `--corpus remin4` argument test was watched red, then green. No
crates change, so no neutrality run.
