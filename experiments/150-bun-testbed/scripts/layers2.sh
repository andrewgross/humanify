#!/usr/bin/env bash
# exp150: the scorer-14 layers that read a run's units (exp140's recipe): copies2 (derived with scorer 13's and 14's
# rules off, as for every other corpus), misattr13, mapdefect14; then the score.
#   experiments/150-bun-testbed/scripts/layers2.sh <corpus> <units label>
# copies2's evidence index: /work/library-index/current only (exp140 read three older indexes, deleted 2026-10-25).
set -euo pipefail
C=$1; L=$2
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
E=$(cd "$(dirname "$0")/../.." && pwd)
W=/work/exp150-data/layers/$C/copies2
mkdir -p "$W"
$PY -I "$E/129-helpers-copies/copies2.py" hashes "$W/hashes.pkl" "$C=$L"
$PY -I "$E/129-helpers-copies/copies2.py" rows /work/library-index/current "$W/hashes.pkl" "$W/rows.pkl"
$PY -I -c "import sys,runpy; sys.path.insert(0,'$E/096-library-matching-score'); import truth_audit as t; \
  t.RULES13['misattributed']=False; t.RULES14['mapdefect']=False; \
  sys.argv=['copies2.py','derive','$W/rows.pkl','$L','--corpus','$C','--jobs','16']; \
  runpy.run_path('$E/129-helpers-copies/copies2.py', run_name='__main__')"
$PY -I "$E/132-factory-precision/scorer13/misattr13.py" build --corpus "$C" --units "$L"
$PY -I "$E/137-scorer-mapdefect/scorer14/mapdefect14.py" build --corpus "$C" --units "$L" --jobs 16
$PY -I "$E/096-library-matching-score/score.py" "$L" --corpus "$C"
echo "LAYERS2 DONE $C"
