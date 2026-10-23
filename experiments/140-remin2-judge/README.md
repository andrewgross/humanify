# exp140: `remin2`, a never-seen judge for the terser-slice flip

## STATUS (2026-10-23)

DONE on branch `exp140-remin2-judge` (off main `6f604195c`). Harness and corpus only, no `crates/` change. Not
merged, and no default was flipped.

- **The question.** exp139 (`exp139-carriers` @ `bd9cfb19c`) recommends flipping `library.lookup.terser_slices=auto`
  (`terser_share` 0.2) together with three new keys: `library.discovery.terser_carriers=0.5`,
  `terser_factory_majority=1` and `terser_payload_literal=true`. It met the bar on all 21 sets. But `remin` is the
  only corpus whose bundles reach the terser slices, two of the keys were tuned on remin-terser's leftover wrong
  bytes, and the precision margin was 0.015 pts. So the flip was judged once on a NEW corpus that no lane has seen.
- **Verdict: the flip FAILS the bar on the unseen judge.** On `remin2-terser`, precision drops from **99.712 to
  99.518 (-0.194 pts)**. Wrong bytes go from 50.0 KB to 232.7 KB, while named bytes grow from 17.3 MB to 48.0 MB
  and F0.5 rises from 50.18 to 81.07 (+30.90). `remin2-esbuild` is byte-identical under all three labels (its
  bundles do not read the terser slices). App claimed is 0 everywhere.
- **The keys still help on unseen data, but far less than on remin.** Compared with `terser_slices=auto` on its
  own, the three keys lift precision by +0.063 pts (99.455 to 99.518) and cut wrong bytes by 30.7 KB (263.4 to
  232.7 KB). On remin they took 339.8 KB down to the margin. Neither variant gets back to main's precision.
- **remin2 is a JUDGE, private like test2** (`lib096.PRIVATE`). Each label was scored once and only per-set
  summaries were read. No units, packages, bundles or miss classes were looked at, and nothing was tuned on it.
  The per-bundle cards stay under `/work/exp140-data/remin2/frozen/results/`.

## Gate (the flip bar: combo vs main defaults, scorer 14, index `k2-all-r5-origin-prod`, both scopes)

| set              | combo P >= main P                | combo F0.5 >= main F0.5 - 0.002 | app claimed 0 | verdict  |
| ---------------- | -------------------------------- | ------------------------------- | ------------- | -------- |
| `remin2-terser`  | **no**: 99.518 < 99.712 (-0.194) | yes: 81.074 vs 50.178 (+30.896) | yes (0)       | **FAIL** |
| `remin2-esbuild` | yes: 99.499 = 99.499             | yes: 84.191 = 84.191            | yes (0)       | pass     |

Harness gate: `npm run check` run bare on this tree: ALL 13 STAGES PASSED (exit 0).

## Results (aggregates only; P / R / F0.5 in %, bytes are library bytes over both scopes)

| label (settings)                                   | set              |          P |     R |       F0.5 |  wrong KB | named MB | library MB | app |
| -------------------------------------------------- | ---------------- | ---------: | ----: | ---------: | --------: | -------: | ---------: | --: |
| `remin2-exp140-main` (main defaults)               | `remin2-terser`  | **99.712** | 16.80 |     50.178 |  **50.0** |   17.328 |     103.16 |   0 |
| `remin2-exp140-combo` (exp139's four settings)     | `remin2-terser`  | **99.518** | 46.56 | **81.074** | **232.7** |   48.028 |     103.16 |   0 |
| `remin2-exp140-terser` (`terser_slices=auto` only) | `remin2-terser`  |     99.455 | 46.57 |     81.050 |     263.4 |   48.044 |     103.16 |   0 |
| `remin2-exp140-main`                               | `remin2-esbuild` |     99.499 | 52.12 |     84.191 |     288.1 |   57.161 |     109.67 |   0 |
| `remin2-exp140-combo`                              | `remin2-esbuild` |     99.499 | 52.12 |     84.191 |     288.1 |   57.161 |     109.67 |   0 |
| `remin2-exp140-terser`                             | `remin2-esbuild` |     99.499 | 52.12 |     84.191 |     288.1 |   57.161 |     109.67 |   0 |

The same split by scope, for `remin2-terser` (P / R / F0.5, wrong KB):

| label  | statement scope                | factory scope                 |
| ------ | ------------------------------ | ----------------------------- |
| main   | 98.148 / 23.26 / 59.703, 40.5  | 99.937 / 16.16 / 49.072, 9.6  |
| combo  | 97.346 / 68.22 / 89.686, 171.4 | 99.853 / 44.43 / 79.917, 61.3 |
| terser | 97.359 / 68.57 / 89.816, 171.4 | 99.780 / 44.42 / 79.869, 92.0 |

The keys act in the factory scope only here: they cut wrong bytes there from 92.0 to 61.3 KB. The statement-scope
wrong bytes (171.4 KB) are the same with and without the keys. That is a summary-level observation. It was not
diagnosed further, because remin2 is a judge.

All three labels ran exp139's binary (`--commit bd9cfb19c`). The `main` label runs it with every key off. Main's
own binary (built from this branch's `crates/`, which are `6f604195c`'s) gave **72/72 byte-identical reports** on
remin2 with the same config, so `remin2-exp140-main` is main's default.

## The corpus (`--corpus remin2`)

- **Recipe: exp126's, unchanged** (`experiments/126-reminified/scripts/dev_remin.py --corpus remin2`,
  `remin-build.mjs`). Stage 1 is an esbuild 0.28.2 bundle, not minified, that prefers each package's published
  minified entry. Stage 2 runs terser 5 (compress + mangle) or esbuild `--minify`. The truth comes from the
  composed source map. Every entry runs with `--bundler esbuild`.
- **Selection** (`selection.json`: ids, kinds, strata, counts, bundle sha256 / bytes). The pool is exp126's:
  exp108 TRAINING apps of kind cli, electron or vscode with a good closure and a library closure of at most 15 MB.
  Every app of every other corpus manifest is removed by slug: dev (`corpus.json`: held-out, Claude Code, wrangler),
  test, test2, apps, pop, bundlers and **remin's 36**. That leaves a pool of 1,340, which is 1,376 minus remin's 36. From it, 12 apps per kind were drawn with `random.Random(140)`. One Electron app's stage 1 failed (esbuild
  could not resolve a `require`). Its replacement was drawn by the same seeded generator continued past the draw
  (`dev_remin.py replace`: a failed build moves the cut and never changes the draw). The result is **36 apps
  (12 cli / 12 electron / 12 vscode) and 72 bundles**: `remin2-terser` 36 (109.5 MB of bundle, 103.2 MB of
  library), and `remin2-esbuild` 36 (115.3 MB, 109.7 MB). 80 package entries were swapped for published minified
  ones in 23 of the 36 apps (5.3 MB).
- **Truth: every scorer-14 layer, frozen BEFORE any score.** The layers are origins and chained (scorer 4-6),
  origins7 (`verify`: rules off reproduce the origin layer on 72/72), chained9, origins10 (`via` read as chunk:
  72/72), chained11, origins12 (`patched` off: 72/72), copies2, misattr13 and mapdefect14. copies2 was derived with
  scorer 13's and 14's rules off, which is how it was derived for every other corpus (at scorer 12). Leaving them
  on would make copies2 and mapdefect14 depend on each other. The units reference for copies2, misattr13 and
  mapdefect14 is `remin2-exp140-main`'s run (claim-blind: the unit model is the same for every setting of one
  binary). The pipeline ran first, and its first score attempt was refused for the missing layer, exactly as
  test2's reference was. The layers were then built. `/work/exp140-data/FROZEN.json` records the sha256 of the 11
  manifests, 792 truth files and 144 corpus files, and the corpus and truth dirs were made read-only before the
  first score.
- **Not done (as for remin):** the families universe (`crates/.../families-derived.json`) was not re-derived with
  remin2's truth labels. That is a `crates/` file, and a truth package outside the universe can only be
  under-credited, never over-credited.

## Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python; S=experiments/126-reminified/scripts
python3 -I $S/dev_remin.py plan --corpus remin2 && python3 -I $S/dev_remin.py build --jobs 10 --corpus remin2
python3 -I $S/dev_remin.py replace --corpus remin2 && python3 -I $S/dev_remin.py build --jobs 10 --corpus remin2
$PY -I $S/dev_remin.py freeze --jobs 16 --corpus remin2
$PY -I experiments/106-matcher-hygiene/scorer5/chained.py build --corpus remin2
$PY -I experiments/112-test-precision/scorer7/origins7.py needs --corpus remin2 /work/exp140-data/needs7.json
python3 -I experiments/104-truth-audit/fetch_metadata.py /work/exp112-data/registry /work/exp140-data/needs7.json
$PY -I experiments/112-test-precision/scorer7/origins7.py owners --corpus remin2 /work/exp140-data/owners7.json
python3 -I experiments/112-test-precision/scorer7/bundled.py /work/exp140-data/owners7.json --jobs 6
$PY -I experiments/112-test-precision/scorer7/origins7.py verify --corpus remin2 && ... build --corpus remin2
$PY -I experiments/120-truth-families/scorer9/chained9.py build --corpus remin2
$PY -I experiments/125-nested-chunk-publisher/scorer10/origins10.py build|verify --corpus remin2
$PY -I experiments/128-cjs-scope/scorer11/chained11.py build --corpus remin2
$PY -I experiments/129-helpers-copies/scorer12/origins12.py verify|build --corpus remin2
C="--index /work/exp135-data/full/ix/k2-all-r5-origin-prod --rule structural --corpus remin2 --commit bd9cfb19c"
npm run eval -- library remin2-exp140-main $C          # first score refused: no copies layer yet
W=/work/exp140-data/copies2
$PY -I experiments/129-helpers-copies/copies2.py hashes $W/hashes.pkl remin2=remin2-exp140-main
$PY -I experiments/129-helpers-copies/copies2.py rows /work/exp105-data/ix/step4-wr /work/exp129-data/ix/k2-all-r5h \
  /work/exp129-data/ix/k2-all-r5-origin-h $W/hashes.pkl $W/rows.pkl
# derive with truth_audit.RULES13/RULES14 off (scorer 12's truth, as for every other corpus), via runpy:
$PY -I -c "import sys,runpy; sys.path.insert(0,'experiments/096-library-matching-score'); import truth_audit as t; \
  t.RULES13['misattributed']=False; t.RULES14['mapdefect']=False; \
  sys.argv=['copies2.py','derive','$W/rows.pkl','remin2-exp140-main','--corpus','remin2','--jobs','16']; \
  runpy.run_path('experiments/129-helpers-copies/copies2.py', run_name='__main__')"
$PY -I experiments/132-factory-precision/scorer13/misattr13.py build --corpus remin2 --units remin2-exp140-main
$PY -I experiments/137-scorer-mapdefect/scorer14/mapdefect14.py build --corpus remin2 --units remin2-exp140-main --jobs 24
$PY -I experiments/096-library-matching-score/score.py remin2-exp140-main --corpus remin2
K="--pipeline-set library.lookup.terser_slices=auto --pipeline-set library.lookup.terser_share=0.2"
npm run eval -- library remin2-exp140-combo $C $K --pipeline-set library.discovery.terser_carriers=0.5 \
  --pipeline-set library.discovery.terser_factory_majority=1 --pipeline-set library.discovery.terser_payload_literal=true
npm run eval -- library remin2-exp140-terser $C $K
```
