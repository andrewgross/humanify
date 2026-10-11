#!/bin/bash
# Units for the Claude Code real-world set: whole bundles (with layout rows) + every positive/negative file.
cd /work/exp095-library-fingerprints/subtree-hashes
MODE=${1:-loose}
RW=/work/exp095-library-fingerprints/lab/realworld
OUT=data/units-$MODE/realworld
for v in 2.1.198 2.1.216; do
  node --max-old-space-size=16000 scripts/extract-file.mjs $MODE $OUT/bundle-$v.tsv \
    /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-$v/binary-decompiled/src/entrypoints/index.js $RW/layout-$v.json &
done
for v in 2.1.198 2.1.216; do
  for set in positives negatives negative-functions; do
    for f in $RW/cc-$v/$set/*.min.js; do echo "$f $OUT/cc-$v/$set/$(basename $f .min.js).tsv"; done
  done
done | xargs -P 48 -n 2 sh -c 'node scripts/extract-file.mjs '$MODE' "$1" "$0"' 2>&1 | grep -v '^$' > data/extract-realworld-$MODE.log
wait
echo "done: $(find $OUT -name '*.tsv' | wc -l) tsv; parse failures: $(grep -c PARSEFAIL data/extract-realworld-$MODE.log)"
