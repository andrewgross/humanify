# exp108: a bigger independent training app corpus, and an index of its closure

## STATUS (2026-10-14)

- **State.** DONE (lab: corpus, selection, two real indexes, scored dev + test). Branch `exp108-app-corpus`, off main
  `116090f2`. No pipeline code changed. One harness flag was added: `eval library --sets A,B` (run.py / score.py),
  which runs and scores a subset of a corpus's sets. Indexes without a webpack or rollup slice need it, because the
  test corpus now holds webpack and rollup bundles. It has a unit test. Every label is scorer 4 (`exp096-score/4`).
  Scorer v5 (exp106, f05) was not on origin/main, so F0.5 = 1.25·P·R / (0.25·P + R) is computed here from P and R.
- **Totals first. P / R / F0.5 / F1 over bytes, structural rule** (held-out = the 67 apps no selection saw; test =
  exp100's frozen split, Bun and esbuild sets; all at main `116090f2`):

  | index (size)                 | held-out 67               | test-bun (55)             | test-esbuild (55)                       | Claude Code              | task-master (esbuild-app) | wrangler                  |
  | ---------------------------- | ------------------------- | ------------------------- | --------------------------------------- | ------------------------ | ------------------------- | ------------------------- |
  | c2-l12m4 (1.73 GB, baseline) | 98.7 / 54.2 / 84.8 / 70.0 | 75.4 / 44.2 / 66.1 / 55.8 | 74.3 / 45.5 / 65.9 / 56.4 (app 1.31 MB) | 100 / 76.7 / 94.3 / 86.8 | 98.3 / 52.5 / 83.7 / 68.5 | 95.5 / 16.5 / 48.8 / 28.2 |
  | **k2-l12m4 (4.65 GB)**       | 82.9 / 66.5 / 79.0 / 73.8 | 79.5 / 55.9 / 73.3 / 65.7 | 80.6 / 65.5 / 77.0 / 72.2 (app 1.32 MB) | 100 / 81.9 / 95.8 / 90.1 | 83.9 / 68.2 / 80.2 / 75.2 | 96.0 / 18.4 / 52.1 / 30.9 |
  | k5-l12m4 (2.91 GB)           | 81.6 / 61.1 / 76.5 / 69.9 | 79.6 / 53.1 / 72.4 / 63.7 | 81.1 / 63.4 / 76.8 / 71.2 (app 1.32 MB) | 100 / 76.7 / 94.3 / 86.8 | 84.7 / 65.2 / 79.9 / 73.6 | 95.9 / 18.1 / 51.6 / 30.5 |

  With exp102's guards (`evidence_conflict`, `literal_conflict`, `contested_cover=0.05` via `--pipeline-set`):

  | index | held-out 67               | test-bun                  | test-esbuild              | Claude Code              | task-master               | wrangler                  |
  | ----- | ------------------------- | ------------------------- | ------------------------- | ------------------------ | ------------------------- | ------------------------- |
  | c2    | 99.2 / 54.1 / 85.0 / 70.0 | 75.7 / 44.1 / 66.2 / 55.8 | 74.3 / 45.4 / 65.9 / 56.3 | 100 / 76.7 / 94.3 / 86.8 | 99.0 / 43.5 / 78.9 / 60.4 | 95.7 / 12.7 / 41.5 / 22.4 |
  | k2    | 82.9 / 65.4 / 78.7 / 73.1 | 80.1 / 55.7 / 73.7 / 65.7 | 81.3 / 65.4 / 77.5 / 72.5 | 100 / 81.9 / 95.8 / 90.1 | 95.3 / 59.1 / 84.9 / 72.9 | 96.2 / 14.5 / 45.3 / 25.3 |
  | k5    | 81.7 / 60.0 / 76.2 / 69.2 | 80.3 / 53.0 / 72.8 / 63.8 | 81.5 / 63.2 / 77.0 / 71.2 | 100 / 76.7 / 94.3 / 86.8 | 97.9 / 56.1 / 85.2 / 71.3 | 96.1 / 14.2 / 44.7 / 24.8 |

  Agreement rule, P / R / F0.5 / F1:

  | index       | held-out 67               | Claude Code              | task-master               | wrangler                 |
  | ----------- | ------------------------- | ------------------------ | ------------------------- | ------------------------ |
  | c2          | 95.1 / 38.1 / 73.2 / 54.4 | 100 / 63.4 / 89.6 / 77.6 | 99.3 / 34.4 / 72.1 / 51.1 | 100 / 10.1 / 36.0 / 18.4 |
  | c2 + guards | 96.5 / 37.9 / 73.7 / 54.5 | 100 / 63.4 / 89.6 / 77.6 | 99.3 / 34.3 / 72.0 / 51.0 | 100 / 6.4 / 25.5 / 12.1  |
  | k2          | 95.1 / 40.7 / 75.1 / 57.0 | 100 / 36.0 / 73.8 / 53.0 | 95.6 / 28.3 / 64.8 / 43.7 | 100 / 11.6 / 39.7 / 20.9 |
  | k2 + guards | 96.5 / 40.6 / 75.7 / 57.1 | 100 / 36.0 / 73.8 / 53.0 | 99.0 / 28.1 / 65.8 / 43.7 | 100 / 7.9 / 30.1 / 14.7  |
  | k5          | 95.3 / 40.9 / 75.3 / 57.2 | 100 / 34.9 / 72.8 / 51.8 | 99.6 / 35.5 / 73.2 / 52.3 | 100 / 11.4 / 39.1 / 20.4 |
  | k5 + guards | 96.6 / 40.7 / 75.7 / 57.2 | 100 / 34.9 / 72.8 / 51.8 | 99.8 / 35.1 / 72.9 / 51.9 | 100 / 7.7 / 29.4 / 14.3  |

  Test split, agreement rule (test-bun / test-esbuild, P / R / F0.5 / F1):

  | index       | test-bun                  | test-esbuild                            |
  | ----------- | ------------------------- | --------------------------------------- |
  | c2          | 73.6 / 38.2 / 62.1 / 50.3 | 72.0 / 39.4 / 61.8 / 50.9 (app 1.30 MB) |
  | c2 + guards | 73.7 / 38.0 / 62.1 / 50.2 | 72.2 / 39.2 / 61.8 / 50.8 (app 1.30 MB) |
  | k2          | 86.8 / 36.8 / 68.3 / 51.7 | 87.3 / 46.1 / 74.1 / 60.4 (app 774 KB)  |
  | k2 + guards | 87.7 / 36.8 / 68.7 / 51.8 | 90.3 / 45.8 / 75.6 / 60.8 (app 774 KB)  |

- **The enlarged corpus works as a recall lever, on held-out and on test alike.** k2 adds 12.3 points of held-out
  recall, +11.7 on test-bun and +20.0 on test-esbuild. Not-indexed falls from 39.4% to 24.8% of the held-out library
  bytes. On the TEST split, PRECISION RISES too (bun 75.4 → 79.5, esbuild 74.3 → 80.6), so test F0.5 goes up 7–11
  points.
- **Held-out precision falls to 83% because of ONE generated-code claim. It is the same one exp101 found on
  call2k.** `@matrix-org/matrix-sdk-crypto-wasm` (newly indexed: bundled by ≥ 2 of the new apps) claims ≥ 3.94 MB of
  wasm-bindgen glue in renovate's `@baszalmstra/rattler` / `jsonc-morph`. 3.9 MB of that is ONE unit, a function that
  returns the base64 wasm payload, reached by the `module` rule. That is 94% of k2's 4.17 MB held-out wrong bytes.
  On k5 the SAME unit is claimed by a different wasm-bindgen package, `@one-ini/wasm`
  (3.91 MB). Without that one class, held-out structural would be about **98.9 / 66.5 / 90.1 / 79.5** (k2) and
  98.6 / 61.1 / 87.8 / 75.4 (k5), against c2's 98.7 / 54.2 / 84.8 / 70.0. exp102's guards do not touch it.
- **Agreement rule: Claude Code's recall halves** (63.4 → 36.0). This is exp101's literal-guard dilution, measured
  again: the literal agreement rule's margin and IDF do not scale with index size.
- **test-esbuild "app claimed" is ~1.3 MB under structural on every index, c2 included** (1.31 MB on c2,
  1.32 MB on k2 and k5); under agreement it FALLS on k2 (1.30 MB → 774 KB). The bigger index does not cause it. This
  lane must not read the test data, so it stays unexamined here (exp107's lane).
- **Recommendation.** Do NOT adopt k2 as default yet: held-out F0.5 under structural is DOWN (84.8 → 79.0) until
  the generated-glue class is fixed. The fix is one generic rule, which exp101 already proposed: stop-list
  wasm-bindgen glue (`__wbg_*` / `__wbindgen_*`), or "a `module` claim never covers a unit with no print of its
  own". Once that rule lands, re-score k2 and expect held-out F0.5 ≈ 90. Index: `/work/exp108-data/idx/ix/k2-l12m4`
  (format /3). The agreement rule needs its literal guard re-scaled before any bigger index helps Claude Code
  under it.

## 1. The corpus (`scripts/select_apps.py`, `corpus.json`)

Mechanical rules. Qualifying rules are exp095 `corpus.py`'s (imported, unchanged) unless stated. Fetched
2026-10-14; all HTTP responses are cached under `/work/exp108-data/cache`.

| stratum  | rule                                                                                                                                                                                 | qualified                                  | closure OK |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------ | ---------- |
| cli      | npm-high-impact's union list (17,338: downloads + dependents) MINUS download ranks 1–7,863 (exp095 read 1–6,000, exp100 6,001–7,863): `bin`, not deprecated, ≥ 5 deps; 8,896 checked | 347                                        | 323        |
| catalog  | electron/apps catalog entries exp095 never tried                                                                                                                                     | 0 (exp095 had read all 369 GitHub entries) | —          |
| electron | GitHub `topic:electron fork:false archived:false` by stars, read past the 1,000-result cap (`stars:<=S`); root package.json, electron in (dev)deps, ≥ 3 runtime deps; first 900      | 900                                        | 894        |
| vscode   | GitHub `topic:vscode-extension …` by stars: `engines.vscode`, ≥ 3 runtime deps (bundler rule as electron); first 350                                                                 | 350                                        | 347        |
| server   | GitHub `topic:self-hosted …`, language JavaScript/TypeScript, by stars: root package.json, no electron, ≥ 5 runtime `dependencies`; first 300                                        | 300                                        | 298        |

**Exclusion (applied, recorded in `corpus.json` → `exclusion`):** every app exp095's selection tried (360-app corpus,
which holds the 67 held-out apps, and every app its log dropped: 5,830 CLI names, 369 repos), every electron/apps
catalog repo (for the GitHub strata, as exp100 did), and ALL 90 candidates in `experiments/100-test-split/selection.json`
(45 CLI names, 45 repos), built or not. A repo that carries two topics is counted once, in the first stratum.

**Closure** (`scripts/closures.py`) is exp095's `closure.one`, unchanged: npm install
(`--ignore-scripts --omit=dev --legacy-peer-deps`, ranges resolved today), an entry requiring every runtime
dependency (or a CLI's bin files), then esbuild 0.28.2 `--bundle --minify --metafile`. A package's weight is the sum
of its files' `bytesInOutput`. Nothing is bundled to disk, and node_modules is deleted after each app. 1,862 of 1,897
apps closed (12 installs failed, 4 bundles failed, 19 bundled no library code). They hold 11.2 GB of minified library
code (electron 6.4 GB, server 2.1, vscode 2.0, cli 0.6) from 18,279 distinct packages, together with exp095's 271
training closures. About 80 minutes on 16 workers.

## 2. Selection and sizing (`scripts/select_roots.py`, `new-roots-k2.txt`, `new-roots-k5.txt`)

New roots = packages bundled by ≥ K apps of the ENLARGED training corpus (exp095's training apps + exp108's), minus
c2's root list. Each new root is indexed at exp101's `year-last12+major4` (`tail_pins.py`). c2's roots and pins are
unchanged.

| K      | new roots over c2 | of them in call2k already | pins added | owners | owner-versions | index   |
| ------ | ----------------- | ------------------------- | ---------- | ------ | -------------- | ------- |
| — (c2) | —                 | —                         | —          | 5,263  | 46,735         | 1.73 GB |
| 5      | 2,298             | 1,257                     | 12,802     | 7,626  | 63,491         | 2.91 GB |
| 2      | 6,343             | 2,489                     | 34,766     | 12,116 | 91,922         | 4.65 GB |

(K = 1 would be 14,648 roots; K = 10, 990.)

**Build cost.** This is the k2 build. k5 is a `--merge-only` of the same store. The build used 30 cores (34–63),
under the lock, with exp101's shard store.

- **Shards:** 101,296 to build and 171,920 reused. 23,898 package versions were built in 7,078 s, plus ~15 min of a
  first attempt whose shards were kept.
- **Merge:** main's streaming `--merge-only`, 804 s at 27.5 GB peak RSS. The k5 merge took 422 s.
- **Builder memory, a finding.** A `humanify-index build` process grows by tens of MB per version it builds. The
  first attempt used exp101's six long-lived parts. After ~800 versions each part was at 31–35 GB, and one part was
  OOM-killed (137); the others were stopped before the box ran out. This is likely the "cause not established" 137s
  in exp101 §4. `scripts/build_chunks.py` runs ~200 short processes of about 125 builds each, 7 at a time, and
  records each one's peak RSS (13–26 GB; `logs/chunks-k2-l12m4.tsv` in the data dir). With chunks, no part failed.
  The growth itself is unexplained (a leak or allocator fragmentation in the build path), and it is worth a look
  before a 10× index.
- **Lookup** (/3 format): median `humanify discover` per dev bundle is 0.67 s on k2 vs 0.60 s on c2. Max peak RSS is
  2.05 GB on both, and the dev corpus takes 138 s on both. The index's size has no lookup cost.

## 3. Where the held-out misses are now (aggregate only; `scripts/miss_buckets.py`)

This is exp101's script with "training closure" = the enlarged corpus. Shares are of held-out library bytes (30.4 MB),
structural:

| bucket                                  | c2            | k2           |
| --------------------------------------- | ------------- | ------------ |
| a — not indexed                         | 39.4% (197 p) | 24.8% (80 p) |
| … bundled by ≥ 2 enlarged-corpus apps   | 14.9% (115)   | 0.0% (1)     |
| … bundled by 1 enlarged-corpus app      | 2.2% (27)     | 2.4% (26)    |
| … bundled by none, npm download top-16k | 2.5%          | 2.6%         |
| … bundled by none, unranked             | 19.9%         | 19.9%        |
| b — indexed, version missing            | 2.5%          | 2.5%         |
| c / d / e / f (all others)              | 4.0%          | 6.1%         |

- The enlarged corpus reached what it could. 115 packages that are bundled by ≥ 2 of its apps were missing from c2,
  and k2 indexes all of them but one.
- What remains not-indexed (22.5%) is in NO app of 2,133. 19.9% of it is in no npm download top-16k either, so a
  further corpus of the same populations is unlikely to reach it.
- The largest remaining lever is wrong claims, not recall: 4.17 MB wrong vs 7.5 MB not-indexed.

## 4. New wrong claims, read by hand (k2 vs c2, held-out, structural; exp101's `new_wrong.py`)

There are 1,140 new wrong-family units, 4.06 MB. The 15 largest distinct claims, read by their code and file:

1. **`@matrix-org/matrix-sdk-crypto-wasm` on wasm-bindgen glue.** It covers `@baszalmstra/rattler`'s
   `dist/rattler.bundler.mjs` (3.90 MB unit: the base64 wasm payload function, `module` rule) and `jsonc-morph`'s
   `rs_lib.internal.js` (`__wrap` / `__wbg_ptr` classes). This is generated template code, the exp101 call2k class
   again, with a different glue owner.
2. **`@netlify/redirect-matcher` on `@cdktf/hcl2json`'s `wasm/wasm_exec.js`.** Go's wasm_exec runtime shim, which
   both packages ship. Generated or shared runtime.
3. **`@rspack/core` on `tinypool` `dist/index.js`.** rspack ships a pre-bundled tinypool. The claim names the shipper
   of a copy, a sponge.
4. **`@modelcontextprotocol/ext-apps`, `@mastra/core` and `npm-check-updates` on `mcp-proxy`'s chunk.** The truth's
   origins are content-type, statuses and zod. The claims name packages whose dist inlines them (sponges).
5. **`valibot` on `rolldown`'s `create-bundler-option` chunk.** rolldown inlines valibot. The claim names the real
   origin, and the truth has no origin for this copy, so this is likely a truth gap and not a wrong claim.
6. **`@livekit/components-react` on `puppeteer-core`'s `third_party/rxjs`.** A sponge (livekit bundles rxjs); seed
   plus `down` growth.
7. **`ora` on `yoctocolors`.** A dependency inlined in ora's build. Sponge.
8. **`@antv/vendor` on `d3-dispatch` (3 apps).** @antv/vendor is a re-bundle of d3. Sponge.
9. **`mermaid` on `d3-shape`.** `down` growth, exp101's known class.
10. **`@vscode/tree-sitter-wasm` on `web-tree-sitter`.** The same emscripten-generated `tree-sitter.js`.
11. **`@mastra/core` on `ai@5`.** mastra bundles the AI SDK. Sponge.
12. **`@ai-sdk/openai` on `ollama-ai-provider-v2`.** Shared provider template code, a sibling.
13. **`debug` on `@sentry/server-utils`' vendored `ms`.** The truth's origin is ms; the claim names the package that
    depends on it.
14. **`dompurify` on `@sentry/server-utils`' vendored esquery (613 B).** Really wrong, a small seed.
15. **`@vitest/utils` on `pathe`'s shared chunk.** vitest pre-bundles pathe. Sponge.

**Classes.** Generated glue (1, 2, 10) is 97% of the bytes. Most of the rest are SPONGES: packages whose own `dist/`
inlines their dependencies, now indexed because more apps bundle them. Their rows claim the inlined origin's code.
Two generic levers follow:

- the glue rule above;
- at index-build time, drop a package's rows whose prints another indexed package owns as its own source (the origin
  wins over the shipper of a copy). Today this is applied only to marked/vendored copies (exp097).

## Reproduce

```bash
D=/work/exp108-data; S=experiments/108-app-corpus/scripts; S101=experiments/101-index-coverage/scripts
python3 -I $S/select_apps.py                         # -> $D/corpus.json (cached HTTP under $D/cache)
SEL=$D taskset -c 16-47 python3 -I $S/closures.py --jobs 16
python3 -I $S/select_roots.py 2 /work/exp101-data/list-c2.txt $D/new-k2.txt     # --counts: sizing per K
python3 -I $S101/fetch_packuments.py $D/idx/registry $D/new-k2.txt               # registry = cp -al of exp101's
python3 -I $S101/tail_pins.py $D/idx/registry 2026-10-11 year-last12+major4 $D/new-k2.txt $D/tail-k2.txt
cat /work/exp101-data/pins-c2-l12m4.txt $D/tail-k2.txt > $D/pins-k2.txt
flock /work/index-build.lock python3 -I $S/build_chunks.py $D/idx k2-l12m4 34-63 7 <humanify-index @116090f2> \
  /work/exp101-data/roots-base.txt $D/pins-k2.txt 250 -- --recipe bun:target=bun,format=cjs,minify \
  --recipe bun:target=node,format=cjs,minify --recipe esbuild:target=node,format=cjs,minify --bun <bun 1.4.0> --esbuild <esbuild 0.28.2>
npm run eval -- library exp108-k2-structural --index $D/idx/ix/k2-l12m4 --rule structural
npm run eval -- library test-exp108-k2-structural --index $D/idx/ix/k2-l12m4 --rule structural --corpus test --sets test-bun,test-esbuild
<exp095 venv python> -I $S/miss_buckets.py exp108-k2-structural out.json
<exp095 venv python> -I $S101/new_wrong.py exp108-base-c2-structural exp108-k2-structural 25
```

`$D/idx/store` is a symlink to exp101's shard store, so the new shards live there and future builds reuse them.
Labels: `exp108-base-c2-*` / `test-exp108-c2v3-*` are c2-l12m4-v3 re-scored at `116090f2` (identical to exp101's
numbers); `exp108-k2-*`, `exp108-k5-*`, `test-exp108-k2-*`, `test-exp108-k5-*` are the candidates.
