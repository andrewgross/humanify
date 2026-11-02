#!/usr/bin/env bash
# exp181: freeze the three kind corpora with source-map truth, build their scorer layers (exp150's layers.sh), and
# build the kind indexes (ix.sh). Run after kind_corpus.py build, under the heavy lock.
set -euo pipefail
E=$(cd "$(dirname "$0")/../.." && pwd)
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
for k in esbuild webpack rollup; do
  $PY -I "$E/181-index-all-bundlers/scripts/kind_corpus.py" freeze --corpus kind-$k --jobs 16
done
for k in esbuild webpack rollup; do
  "$E/150-bun-testbed/scripts/layers.sh" kind-$k
done
"$E/181-index-all-bundlers/scripts/ix.sh" /work/exp181-data/humanify-index
echo "PREP DONE"
