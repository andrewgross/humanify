# exp101: index coverage — which packages the library index holds

## STATUS (2026-10-14)

- **State.** DONE (lab: index selection + two real indexes). Branch `exp101-index-coverage`, merged with main
  `5a04e816`. No pipeline code changed: the work is a generic package-selection policy, two scripts that apply it,
  and the indexes it built. Every label below was scored (or re-scored) with scorer 4 (`exp096-score/4`).
- **Totals first (P / R / F1 over bytes, scorer 4).** Held-out = the 67 apps no selection saw; test = exp100's
  frozen split (Bun recipe, which is the held-out recipe).

  | index (size)             | rule       | held-out 67            | test-bun (55)      | test-esbuild (55)               | Claude Code        | task-master-ai     | wrangler           |
  | ------------------------ | ---------- | ---------------------- | ------------------ | ------------------------------- | ------------------ | ------------------ | ------------------ |
  | step4 (577 MB, baseline) | structural | 96.6 / 31.4 / 47.4     | 52.7 / 14.1 / 22.2 | 51.4 / 14.6 / 22.7              | 100 / 70.6 / 82.8  | 98.0 / 40.5 / 57.4 | 95.1 / 14.8 / 25.6 |
  | **c2-l12m4 (1.82 GB)**   | structural | 98.7 / 54.2 / 70.0     | 75.4 / 44.2 / 55.7 | 74.3 / 45.5 / 56.4              | 100 / 76.7 / 86.8  | 98.3 / 52.5 / 68.5 | 95.5 / 16.5 / 28.2 |
  | c2-l12m4 + exp102 guards | structural | **99.2 / 54.1 / 70.0** | —                  | —                               | 100 / 76.7 / 86.8  | 99.0 / 43.5 / 60.4 | 95.7 / 12.7 / 22.4 |
  | call2k-l12m4 (3.04 GB)   | structural | 81.2 / 59.7 / 68.8     | 79.2 / 52.2 / 62.9 | 77.7 / 53.1 / 63.1              | 100 / 75.2 / 85.8  | 93.9 / 52.6 / 67.5 | 99.0 / 20.7 / 34.2 |
  | call2k-l12m4 + guards    | structural | 81.6 / 59.5 / 68.8     | —                  | —                               | 100 / 74.5 / 85.4  | 94.1 / 46.1 / 61.9 | 100 / 16.9 / 28.9  |
  | step4 (baseline)         | agreement  | 96.5 / 24.3 / 38.8     | 58.6 / 12.9 / 21.1 | 58.9 / 12.0 / 19.9              | 100 / 61.9 / 76.4  | 99.0 / 28.4 / 44.1 | 100 / 10.6 / 19.1  |
  | c2-l12m4                 | agreement  | 95.1 / 38.1 / 54.4     | —                  | —                               | 100 / 63.4 / 77.6  | 99.3 / 34.4 / 51.1 | 100 / 10.1 / 18.4  |
  | c2-l12m4 + guards        | agreement  | 96.5 / 37.9 / 54.5     | —                  | —                               | 100 / 63.4 / 77.6  | 99.3 / 34.3 / 51.0 | 100 / 6.4 / 12.1   |
  | call2k-l12m4             | agreement  | 95.2 / 38.2 / 54.5     | 90.3 / 36.8 / 52.3 | 88.0 / 37.2 / 52.3 (app 765 KB) | 99.9 / 48.0 / 64.8 | 91.8 / 30.1 / 45.3 | 100 / 13.5 / 23.7  |
  | call2k-l12m4 + guards    | agreement  | 96.5 / 38.0 / 54.6     | —                  | —                               | 100 / 47.3 / 64.2  | 92.0 / 30.1 / 45.3 | 100 / 9.8 / 17.8   |

  Guards = exp102's `library.discovery.evidence_conflict=true`, `literal_conflict=true`, `contested_cover=0.05`
  (via `--pipeline-set`). The exp102 baseline with guards on step4 is held-out structural 98.6 / 31.4 / 47.6.

- **Coverage is the lever, as measured.** The held-out F1 under structural goes 47.4 → 70.0 with the first index
  (+22.8 recall at higher precision), and the test split moves the same way (+30 recall, +33 F1 on Bun and esbuild),
  so the gain is not a fit to the held-out apps.
- **The second, bigger index buys recall but costs precision.** It adds 5.5 points held-out recall and 8 test
  points, but one wrong claim of 3.9 MB takes held-out structural precision to 81%. Agreement on Claude Code
  loses 14 points of recall, and task-master precision falls 4–7 points. Details are in §4.
- **One test bar is broken by BOTH new indexes: test-esbuild "app claimed" rises from 13.8 KB to 1.31 MB (c2) / 1.32 MB (call2k) structural, 765 KB call2k agreement.**
  Dev and held-out stay at 0 app bytes on every label. This lane must not read the test data, so the cause is
  unexamined. exp104 found that baseline's 13.8 KB was mostly a truth artifact: esbuild chained
  `@angular/compiler`'s own sourcemap, so its paths escape `node_modules` and are credited as `<app>`. The new
  indexes hold many more Angular packages, so the same artifact growing is plausible, but it is NOT verified. A
  non-tuning lane must check it before either index becomes a default.
- **Recommendation.** Make **c2-l12m4 with exp102's guards on** the campaign default, once the test-esbuild
  app-claimed bytes are explained:

  - held-out structural 99.2 / 54.1 / 70.0 (F1 +22.6, precision +2.6);
  - /3 copy: `/work/exp103-data/ix/c2-l12m4-v3`.

  Do not adopt call2k-l12m4 until two problems are fixed:

  - generated wasm-bindgen glue is stop-listed or made non-propagating;
  - the agreement rule's literal guard scales with index size.

  After those fixes, call2k is the next point on the curve. Its /3 copy is `/work/exp103-data/ix/call2k-l12m4-v3`.

## 1. Where the held-out misses were (aggregate only; `scripts/miss_buckets.py`)

The script re-runs the scorer with the miss classifier wrapped, so each missed byte keeps its package, and prints
ONLY aggregates. No held-out package name or version was used to choose anything. Scorer 4, structural, shares of
the held-out library bytes (30.4 MB):

| bucket                                               | step4         | c2-l12m4      | call2k-l12m4  |
| ---------------------------------------------------- | ------------- | ------------- | ------------- |
| a — package not indexed                              | 63.7% (288 p) | 39.4% (197 p) | 34.3% (116 p) |
| … in the closure of ≥ 2 TRAINING apps                | 21.8% (86)    | 0.0% (1)      | 0.0% (1)      |
| … in the closure of 1 training app                   | 6.7% (63)     | 4.3% (58)     | 0             |
| … in no training app's closure, npm download top-16k | 14.4% (90)    | 14.4% (90)    | 13.6% (69)    |
| … in no training app's closure, unranked             | 20.7% (49)    | 20.7% (48)    | 20.7% (46)    |
| b — package indexed, version missing                 | 2.7% (32 v)   | 2.5% (21 v)   | 1.9% (34 v)   |

- **Not indexed is two-thirds of the miss; a missing version is under 3%.** Coverage, not density, is the lever. All of
  step4's missing versions were published within 12 months of the as-of date: they are dependency owners indexed only
  at the versions their roots resolve.
- **Over a third of the step4 miss was already in the training corpus's closure.** The re-priced greedy (exp095 index-v2)
  cut it because of cost, not because it was rare. A package that 13 training apps bundle was priced at 57 MB under
  `year-all+minor`: the older-minor history of long-lived packages is what made those packages expensive.
- **What remains after call2k (34.3%) is in no training app's closure.** 21% of it is in no download top-16k either.
  More of it can come only from a larger INDEPENDENT app corpus.
- **The misses are concentrated.** The largest missing package is 20% of the a-bytes, and the top 10 are 54%.

## 2. Selection policy (generic, reproducible)

The corpus is exp095's selection corpus minus its 67 held-out apps: 293 training apps with closures. exp100's test
apps are disjoint from this corpus by exp100's construction. Indexes are built as of 2026-10-11.

- **c2-l12m4.** step4's roots and pins unchanged, plus every package bundled by **≥ 2 training apps** that has a
  registry packument: 1,811 new roots. Each new root is indexed at the versions of the `year-last12+major4` policy.
- **call2k-l12m4.** The same, plus every package bundled by ANY training app, plus npm-high-impact's download
  top-2,000: 5,827 new roots in all.
- **Density `year-last12+major4`** (`scripts/tail_pins.py`, emitted as `--pinned` specs for the new roots):

  - the 12 newest stable releases published in the 12 months before as-of;
  - plus the newest release of each older release line (a major, or a 0.minor);
  - at most 4 lines in all.

  This is what caret ranges resolve to now or within the last year. Without the line cap, 0.x packages that cut a
  minor per release index hundreds of versions each (lucide-react 564, flow-parser 313). Without the 12-release cap,
  release trains publish 100+ releases a year. The first, uncapped attempt (`year-last12+major`, 20,509 versions for
  c2) was stopped for that reason. `year-last12+major4` gives c2 11,316 versions, at 2.5% held-out version misses.

| index        | roots        | owners | owner-versions | size    | shards built              | build wall (30 cores)                                      | merge RSS       |
| ------------ | ------------ | ------ | -------------- | ------- | ------------------------- | ---------------------------------------------------------- | --------------- |
| step4        | 1,821 + pins | 2,989  | 32,722         | 577 MB  | —                         | —                                                          | —               |
| c2-l12m4     | + 1,811      | 5,263  | 46,735         | 1.82 GB | ~21.5 k new + 6 k failing | ~75 min (shards, partly in the stopped run) + 18 min merge | 112 GB (exp103) |
| call2k-l12m4 | + 5,827      | 9,756  | 74,164         | 3.04 GB | ~82 k more                | ~110 min shards + 32 min merge                             | not measured    |

Lookup (`humanify discover`, /2 format, per-run index load included; measured while other jobs shared the box):

| index        | median wall / bundle | peak RSS |
| ------------ | -------------------- | -------- |
| step4        | 2.6 s                | 2.4 GB   |
| c2-l12m4     | 9.9 s                | 3.6 GB   |
| call2k-l12m4 | 13.3 s               | 5.9 GB   |

exp103's /3 format loads call2k in 0.10 s and 106 MB. Use main's streaming merge for any future build: the old merge
needs ~60× the index size in RAM.

## 3. New wrong claims, read (structural rule; `scripts/new_wrong.py`)

- **c2-l12m4 vs step4 (scorer 3, before exp104's families).**
  - 98% of the new wrong bytes were in one app: **mermaid** claimed over `@mermaid-js/parser`, katex, cytoscape,
    dagre-d3-es and d3-\*. The cause is mermaid's index rows from its own pre-bundled `dist/mermaid.js` /
    `mermaid.min.js`, which inline every dependency. These are sponge files that exp097's sponge filter (marker or
    vendored dir only) cannot see. The claims spread by the `module` and `down` rules.
  - Scorer 4's metadata families count `@mermaid-js/*` as mermaid, so under scorer 4 c2's held-out precision is
    HIGHER than step4's.
- **The rest, read by hand (15 claims):**
  - `@noble/hashes` / `@noble/curves` on openpgp's pre-bundled `dist/node/openpgp.mjs`: the claim names the real
    origin, and the copy is unmarked (truth gap).
  - `dotenv-expand` on the same @noble/curves code: really wrong, a `down` growth.
  - `@eslint/css-tree` on css-tree: a scoped fork.
  - `micromark` on `micromark-core-commonmark`: same repo.
  - `got` on `@szmarczak/http-timer`: `down` growth into a dependency.
  - `@angular/*` on `@ng-bootstrap/ng-bootstrap`.
  - `@opentelemetry/instrumentation-*` on `@sentry/node`, whose pre-bundled copy is unmarked.
- **call2k-l12m4 vs c2-l12m4.**
  - 3.90 MB of the 4.04 MB new wrong is ONE claim: `@biomejs/wasm-nodejs` on `@baszalmstra/rattler`'s
    `dist/rattler.bundler.mjs`. Both are **wasm-bindgen generated glue** (`__wbg_*`, `__wbindgen_malloc`). A matched
    glue function seeds a `module` claim, and that claim covers a function returning the base64 wasm payload.
  - This is generated template code, the exp102/exp106 class. A stoplist of wasm-bindgen glue, or "a module claim
    never covers a unit with no print of its own", would remove it. Without that one claim, call2k's held-out
    structural precision would be about 98.4%.
  - The rest are the classes above, plus `emoticon` on `@docusaurus/mdx-loader`, `lighthouse` on
    `@paulirish/trace_engine` (a bundled copy), and `@langchain/core` / `openai` on `zod-to-json-schema` / `uuid`
    (bundled copies).
- **Agreement rule on call2k.** Claude Code's recall falls 63.4 → 48.0 and its "guard refused" share rises 10.0% →
  23.6%. The literal agreement rule refuses more as more families share literals: its margin and IDF do not scale
  with index size. exp102's guards cut recall on esbuild wrangler (−4 points on both indexes) and do not touch this.

## 4. Things found on the way

- **Main's merge needs ~60× the index in RAM** (112 GB for c2). Two parts of the first call2k attempt were killed
  with exit 137 at 06:0x, cause not established. The build was resumed: the store keeps every finished shard.
  exp103's streaming merge (`--merge-only`) is the fix.
- **1,509 step4 versions npm cannot install are retried on every build** (6,036 shards); `--merge-only` skips them.
- **The scorer's per-app mean is a poor summary:** 37 of 67 held-out apps hold < 1 MB of library code in statement
  units (median per-app recall 0).

## Reproduce

```bash
D=/work/exp101-data   # registry/ (cp -al of index-v2's + fetched), store/ (cp -al of exp097's), work/
python3 -I scripts/fetch_packuments.py $D/registry $D/names-closure.txt $D/names-top5000.txt
# training closure = exp095 selection apps/*/closure.json minus selection.json "heldout"; new roots = c2 (>=2 apps)
python3 -I scripts/tail_pins.py $D/registry 2026-10-11 year-last12+major4 $D/new-c2.txt $D/tail-c2-l12m4.txt
cat /work/exp097-data/pins-step4.txt $D/tail-c2-l12m4.txt > $D/pins-c2-l12m4.txt
scripts/build-index.sh $D c2-l12m4 34-63 6 <humanify-index @ main> $D/roots-base.txt $D/pins-c2-l12m4.txt -- \
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify \
  --recipe esbuild:target=node,format=cjs,minify --bun <bun 1.4.0> --esbuild <esbuild 0.28.2>
npm run eval -- library exp101-c2-l12m4-structural --index $D/ix/c2-l12m4 --rule structural
<exp095 venv python> -I scripts/miss_buckets.py <label> out.json       # aggregate miss buckets
<exp095 venv python> -I scripts/new_wrong.py <base label> <cand label> 15
```

Labels → commits: `exp101-c2-l12m4-*` ran at e9b8c0f0 (re-scored with scorer 4); every other `exp101-*` and
`test-exp101-*` label ran at 5a04e816.
