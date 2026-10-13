# ngram-minhash/verify: commands only

Adversarial re-measure of the ngram-minhash open-world claim with an honest leave-one-package-out filter.

```bash
cd /work/exp095-library-fingerprints/ngram-minhash/verify
export PYTHONHASHSEED=0 OMP_NUM_THREADS=1 NPROC=12
../.venv/bin/python lopo.py B 5 2                       # filter d2, own package removed from index AND filter
../.venv/bin/python lopo.py B 5 0                       # no filter
../.venv/bin/python lopo.py B 5 5                       # filter d5
../.venv/bin/python lopo.py B 5 2 bun-1.4.0 0.5 1       # half the other packages
../.venv/bin/python lopo.py B 5 2 bun-1.4.0 0.25 1      # a quarter
../.venv/bin/python lopo.py B 5 0 bun-1.4.0 0.25 1
../.venv/bin/python ow_eval.py B5d2_bun-1.4.0_f1.0_s0 B5d2_bun-1.4.0
../.venv/bin/python trend.py B5d0_bun-1.4.0_f1.0_s0 B5d2_bun-1.4.0_f1.0_s0 B5d5_bun-1.4.0_f1.0_s0 \
  B5d2_bun-1.4.0_f0.5_s1 B5d2_bun-1.4.0_f0.25_s1 B5d0_bun-1.4.0_f0.25_s1
```

Outputs: results/lopo*\*.npz, results/oweval*\*.json, results/trend.json.
