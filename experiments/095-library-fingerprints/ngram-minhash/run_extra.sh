#!/bin/bash
# Variant sims beyond the grid: boilerplate filter (d = max packages a shingle may appear in) and
# content-defined subsampling (s = keep 1 in s shingles), then MinHash/LSH, then the window scans.
cd "$(dirname "$0")"
export PYTHONHASHSEED=0 OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1
run() { [ -f results/sim_$5.npz ] || .venv/bin/python sim.py "$1" "$2" "$3" "$4" "${6:-1}"; }
for cfg in bun-1.4.0 esbuild-0.28.2; do
  run B 5 $cfg 1 B5d1_$cfg
  run B 5 $cfg 2 B5d2_$cfg
  run B 5 $cfg 3 B5d3_$cfg
  run B 3 $cfg 2 B3d2_$cfg
  run L 5 $cfg 2 L5d2_$cfg
  run H 5 $cfg 2 H5d2_$cfg
done
for s in 4 8 16 32; do
  .venv/bin/python sim.py B 5 bun-1.4.0 0 $s
  .venv/bin/python sim.py B 5 bun-1.4.0 2 $s
done
