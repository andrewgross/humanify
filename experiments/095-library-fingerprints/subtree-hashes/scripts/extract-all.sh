#!/bin/bash
# Usage: scripts/extract-all.sh <loose|strict>   -> data/units-<mode>/builds/.../<mode>.tsv
cd /work/exp095-library-fingerprints/subtree-hashes
MODE=${1:-loose}
# biggest builds first so the pool stays busy
shuf --random-source=<(yes) data/dirs.txt | xargs -P 56 -n 20 node --max-old-space-size=6000 scripts/extract.mjs $MODE data/units-$MODE 2>&1 | grep -v '^$' > data/extract-$MODE.log
echo "done: $(find data/units-$MODE -name '*.tsv' | wc -l) tsv; failures: $(grep -c FAIL data/extract-$MODE.log)"
