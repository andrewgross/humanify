#!/bin/bash
# All variant x index sims (each ~30-60 s on 48 cores).
cd "$(dirname "$0")"
export PYTHONHASHSEED=0 OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1
for cfg in bun-1.4.0 esbuild-0.28.2; do
  for s in H B BX L LX LIT; do
    for n in $( [ $s = LIT ] && echo 0 || echo 3 5 8 ); do
      [ -f results/sim_${s}${n}_${cfg}.npz ] || .venv/bin/python sim.py $s $n $cfg
    done
  done
done
