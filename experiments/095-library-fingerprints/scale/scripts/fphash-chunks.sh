#!/bin/bash
# whole-module hashes in 10 chunks (sorted by bundle) to bound memory; RAYON 16 threads
cd /work/exp095-library-fingerprints/scale/data
for i in 0 1 2 3 4 5 6 7 8 9; do
  [ -s fphash.$i.jsonl ] && continue
  RAYON_NUM_THREADS=16 /work/exp095-library-fingerprints/exact-module-hash/fphash/target/release/fphash < fphash-jobs.$i.jsonl > fphash.$i.tmp 2>> fphash.err && mv fphash.$i.tmp fphash.$i.jsonl
  echo chunk $i $(wc -l < fphash.$i.jsonl)
done
cat fphash.?.jsonl > fphash.jsonl; echo ALLDONE
