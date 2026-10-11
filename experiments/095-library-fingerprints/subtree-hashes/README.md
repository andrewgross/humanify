# exp095 / subtree-hashes: fingerprints of sub-trees (statements, functions, classes), not whole modules

Question: can an index of hashes of small pieces of npm library code (each top-level statement, each function,
each class, identifiers masked) recognise libraries in minified bundles: wrapped modules (vendor naming) AND
library code with no boundary (eager ESM, lazy ESM with hoisted functions: Step B)?

The findings were returned to the orchestrating workflow (no REPORT.md was written: the harness refuses report
files from sub-agents); every number they quote is in `results/`. This file lists what is here and how to re-run it.

Nothing in the humanify repo was touched or committed. The inputs are the shared lab
(`/work/exp095-library-fingerprints/lab/`) and the raw Claude Code bundles.

## Layout

```
scripts/canon.mjs          the canonical serialiser: AST -> token stream, unit extraction, loose normalisations
scripts/canon.v1.mjs, canon.v2.mjs   earlier normalisation rounds (kept for the record; not used)
scripts/extract.mjs        units of one lab build (row = label idx)        -> data/units-<mode>/builds/<build>.tsv
scripts/extract-all.sh     every minified lab build + bun-1.4.0/esbuild unminified (56 parallel)
scripts/extract-file.mjs   units of a standalone file (CC bundle with layout rows; CC positives/negatives)
scripts/extract-realworld.sh   all Claude Code inputs                    -> data/units-<mode>/realworld/
scripts/showunit.mjs, sample-diffs.mjs   debugging: print a unit's tokens / where two bundlers' units diverge
scripts/stability.mjs      same module, two build configs: share of unit hashes that survive   -> results/stability.txt
scripts/uniqueness.mjs     how many packages share a hash, by unit size; overlap with app code -> results/uniqueness.txt
scripts/drift.mjs          share of the latest version's units present in patch/minor/major    -> results/drift.txt
scripts/identify.mjs       precision/recall of "k matching units => library X", 8 scenarios -> results/identify-*.txt
scripts/rule.mjs           decision rules (k units / one big unit)                          -> results/rule.txt
scripts/stream.mjs         labelling an unbounded statement stream (combo / eager builds)   -> results/stream.txt
scripts/index-size.mjs     hashes per package-version, growth over versions, bytes          -> results/index-size.txt
scripts/cc-scan.mjs        the index over the raw Claude Code bundle                        -> results/cc-scan-<v>-min<n>.txt
scripts/cc-app-hits.mjs    every unit hit inside CC groups that hold a product string       -> results/cc-app-hits-<v>-min<n>.txt
scripts/cc-msal.mjs        MSAL banner modules as ground truth for lazy ESM in src/         -> results/cc-msal-<v>.txt
scripts/cc-versions.mjs    which library VERSION Claude Code ships                          -> results/cc-versions-<v>.txt
lib/load.mjs, lib/index.mjs, lib/stream.mjs   loaders, the index + voting classifier, the statement-stream labeller
rwlab/                     a second, smaller lab: 18 Claude-Code-relevant packages x up to 40 versions (364 package
                           versions), Bun 1.4.0 minified, `require` AND `import()` entries, labelled with the
                           shared lab's label.mjs (copied; it resolves paths relative to its own folder)
data/                      unit TSVs (row, type, start, end, nodes, tokens, stringChars, h_mask, h_alpha, h_nolit, crosses)
results/                   every number quoted in REPORT.md
```

Unit hash variants in every TSV: `mask` (identifiers masked, literals kept: the one used everywhere),
`alpha` (identifiers numbered by first occurrence), `nolit` (identifiers and literals masked).
Modes: `loose` (minifier-invariant normalisations, the default) and `strict` (AST as parsed, for comparison).

## Re-run (exact commands)

```bash
cd /work/exp095-library-fingerprints/subtree-hashes
scripts/extract-all.sh loose && scripts/extract-all.sh strict        # ~10 s each on 56 cores
scripts/extract-realworld.sh loose                                    # ~8 s
node --max-old-space-size=30000 scripts/stability.mjs                 # writes results/stability.txt
node --max-old-space-size=30000 scripts/uniqueness.mjs > results/uniqueness.txt
node --max-old-space-size=20000 scripts/drift.mjs 20 > results/drift.txt
node --max-old-space-size=30000 scripts/identify.mjs                  # all scenarios -> results/identify-*.txt
node --max-old-space-size=30000 scripts/rule.mjs > results/rule.txt
node --max-old-space-size=30000 scripts/stream.mjs > results/stream.txt
# rwlab (CC-relevant packages, many versions)
(cd rwlab && node scripts/pick.mjs > versions.json)   # NOTE: then msal-node 3.8.1 and chokidar 4.x were added by hand
rwlab/scripts/install.sh
(cd rwlab && node scripts/build.mjs && node scripts/label.mjs --jobs 48 && find builds -name labels.jsonl | sort | xargs cat > labels/all.jsonl)
find rwlab/builds -name out.js | sed 's|/out.js||; s|^rwlab/||' > data/dirs-rw.txt
cat data/dirs-rw.txt | xargs -P 48 -n 8 env LABROOT=/work/exp095-library-fingerprints/subtree-hashes/rwlab/ node --max-old-space-size=8000 scripts/extract.mjs loose data/units-loose-rw
node --max-old-space-size=30000 scripts/index-size.mjs > results/index-size.txt
for v in 2.1.216 2.1.198; do
  node --max-old-space-size=30000 scripts/cc-scan.mjs $v 20 > /dev/null
  node --max-old-space-size=30000 scripts/cc-versions.mjs $v > results/cc-versions-$v.txt
  node --max-old-space-size=20000 scripts/cc-msal.mjs $v > results/cc-msal-$v.txt
  for m in 10 15 20 30; do node --max-old-space-size=20000 scripts/cc-app-hits.mjs $v $m > results/cc-app-hits-$v-min$m.txt; done
done
```

`rwlab/versions.json` as used is committed here; `pick.mjs` reads npm's live release list, so re-picking later
would choose newer versions.
