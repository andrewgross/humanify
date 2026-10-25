#!/usr/bin/env bash
# exp150: every scorer-14 truth layer that needs no run, for one source-mapped corpus (exp140's list, in its order).
#   experiments/150-bun-testbed/scripts/layers.sh bun-dev|bun-held
# The layers that read a run's units (copies2, misattr13, mapdefect14) are layers2.sh, after the first score attempt.
set -euo pipefail
C=$1
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
E=$(cd "$(dirname "$0")/../.." && pwd)
W=/work/exp150-data/layers/$C
mkdir -p "$W"
$PY -I "$E/106-matcher-hygiene/scorer5/chained.py" build --corpus "$C"
$PY -I "$E/112-test-precision/scorer7/origins7.py" needs --corpus "$C" "$W/needs7.json"
python3 -I "$E/104-truth-audit/fetch_metadata.py" /work/exp112-data/registry "$W/needs7.json"
$PY -I "$E/112-test-precision/scorer7/origins7.py" owners --corpus "$C" "$W/owners7.json"
python3 -I "$E/112-test-precision/scorer7/bundled.py" "$W/owners7.json" --jobs 6
$PY -I "$E/112-test-precision/scorer7/origins7.py" verify --corpus "$C"
$PY -I "$E/112-test-precision/scorer7/origins7.py" build --corpus "$C"
$PY -I "$E/120-truth-families/scorer9/chained9.py" build --corpus "$C"
$PY -I "$E/125-nested-chunk-publisher/scorer10/origins10.py" build --corpus "$C"
$PY -I "$E/125-nested-chunk-publisher/scorer10/origins10.py" verify --corpus "$C"
$PY -I "$E/128-cjs-scope/scorer11/chained11.py" build --corpus "$C"
$PY -I "$E/129-helpers-copies/scorer12/origins12.py" verify --corpus "$C"
$PY -I "$E/129-helpers-copies/scorer12/origins12.py" build --corpus "$C"
echo "LAYERS DONE $C"
