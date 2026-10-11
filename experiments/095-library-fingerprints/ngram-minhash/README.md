# exp095 / ngram-minhash: probabilistic fingerprints (token n-grams, MinHash, containment)

Question: can an index of n-gram fingerprints of npm library modules recognise library code in a
minified bundle, for naming vendor files and for finding library code inside `src/` (Step B)?
Results and verdict: **REPORT.md**. Input: the shared lab (`../lab/README.md`). Nothing in the humanify
repo was touched.

## Files

| file                                        | what it does                                                                                                                                                |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lex.py`                                    | tokenizer (a rule-for-rule port of humanify `vendor_pairing.rs` `lex_with`) and the five token streams H, B, BX, L, LX                                      |
| `extract.py`                                | lexes 143,243 spans (every minified library span of the lab, every app span, the Claude Code sets) -> `data/tok_*.npy`, `data/spans.jsonl`, `data/lits.pkl` |
| `srcmap.py`                                 | (pkg, version, file) -> sha256 and path of the installed SOURCE file (`data/srcsha.json`, `data/srcpath.json`)                                              |
| `srcsets.py`                                | source-level shingle sets; `genuine()` says whether two packages really ship the same code                                                                  |
| `fp.py`                                     | shingle sets (CSR, cached in `data/sets_*`), the index builder, humanify's literal sets                                                                     |
| `sim.py`                                    | every query span x every index entry, exact intersections -> Jaccard / containment summaries (`results/sim_<tag>.npz`)                                      |
| `analyze.py`                                | first-pass precision/recall tables for every sim (`results/analysis.json`)                                                                                  |
| `tables.py`                                 | the report's module-level tables (size gate, margin, open-world check) (`results/tables_*`)                                                                 |
| `pairs.py`                                  | similarity by relation (bundler pairs, versions, files, packages) (`results/pairs.json`)                                                                    |
| `minhash.py`                                | MinHash (k = 64/128/256) error vs exact Jaccard, LSH banding (`results/minhash_*.json`)                                                                     |
| `scan.py`, `scan_eval.py`                   | sliding windows over whole bundles (combo apps, Claude Code) -> what each window is (`results/scan*`)                                                       |
| `cc_hljs.py`                                | Claude Code's 188 highlight.js grammars vs our Bun-1.4.0 builds of hljs 10.7.3 / 11.9.0 / 11.11.1 (`hljs/`)                                                 |
| `humanify_cmp.py`                           | humanify's current content-pairing score used as an index lookup, vs this study's variants                                                                  |
| `npm/npm_stats.py`, `npm/tarball_sample.py` | npm scale for the index-size extrapolation                                                                                                                  |

Tag names: `<stream><n>[d<k>][s<m>]_<index config>`. `d<k>` = boilerplate filter (a shingle found in more
than k packages of the index is ignored), `s<m>` = keep only shingles whose hash is 0 mod m.

## Exact commands (re-runnable; ~1 h total on 64 cores, peak < 20 GB per job)

```bash
cd /work/exp095-library-fingerprints/ngram-minhash
python3 -m venv .venv && .venv/bin/pip install numpy scipy
export PYTHONHASHSEED=0 OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1   # token hashes need the fixed seed
.venv/bin/python extract.py                 # ~15 s
.venv/bin/python srcmap.py && .venv/bin/python srcsets.py
./run_sims.sh                               # 5 streams x n in {3,5,8} + LIT, two index configs (~30 min)
./run_extra.sh                              # boilerplate filter d1/d2/d3, subsampling s4..s32
./rerun_key.sh                              # key variants again with the margin fields
.venv/bin/python analyze.py                 # all tags (slow: ~15 min)
for t in B5d2_bun-1.4.0 B5_bun-1.4.0 H5_bun-1.4.0 L5d2_bun-1.4.0 B3d2_bun-1.4.0 LIT0_bun-1.4.0 B5d2_esbuild-0.28.2; do
  for M in J CQ; do for a in file pkg; do .venv/bin/python tables.py $t $M 0.1 $a > results/tables_${t}_${M}_${a}.txt; done; done; done
.venv/bin/python pairs.py
.venv/bin/python minhash.py B 5 bun-1.4.0
for W in 64 128 256; do .venv/bin/python scan.py B 5 bun-1.4.0 $W combos; .venv/bin/python scan.py B 5 bun-1.4.0 $W combos 2; done
.venv/bin/python scan.py B 5 bun-1.4.0 128 cc; .venv/bin/python scan.py B 5 bun-1.4.0 128 cc 2
.venv/bin/python scan.py L 5 bun-1.4.0 128 combos 2; .venv/bin/python scan.py L 5 bun-1.4.0 128 cc 2
for t in results/scan_*.npz; do t=${t#results/scan_}; .venv/bin/python scan_eval.py ${t%.npz}; done   # MIN_INF=20 default
# Claude Code highlight.js check: build the references first
(cd hljs && for v in 10.7.3 11.9.0 11.11.1; do mkdir -p p$v && cd p$v && echo '{"private":true}' > package.json \
  && npm install --ignore-scripts highlight.js@$v && echo "module.exports=require('highlight.js');" > entry.cjs \
  && B=../../../lab/tools/bun-1.4.0/node_modules/.bin/bun && $B build entry.cjs --target node --format cjs --minify --outfile out.min.js \
  && $B build entry.cjs --target node --format cjs --outfile out.unmin.js && cd .. && node split.mjs p$v; done)
.venv/bin/python cc_hljs.py
.venv/bin/python humanify_cmp.py
python3 npm/npm_stats.py 3000 && python3 npm/tarball_sample.py 200
```

`scratch/` holds the two chaining scripts used during the run (they only sequence the commands above).
