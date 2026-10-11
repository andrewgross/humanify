#!/bin/bash
# re-run the key variants with the margin fields (secondfile / secondpkg)
cd "$(dirname "$0")"
export PYTHONHASHSEED=0 OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1
for cfg in bun-1.4.0 esbuild-0.28.2; do
  .venv/bin/python sim.py B 5 $cfg 2
  .venv/bin/python sim.py B 5 $cfg
  .venv/bin/python sim.py H 5 $cfg
  .venv/bin/python sim.py L 5 $cfg 2
  .venv/bin/python sim.py B 3 $cfg 2
  .venv/bin/python sim.py LIT 0 $cfg
done
