#!/usr/bin/env bash
# exp122 step 1c: the lab tool over the plan (plan.py): every captured bundle and every planned index build,
# under each candidate normalisation; variant 0 also with canonical streams (for the diffs). <= 16 cores.
# usage: run_lab.sh <variants, space-separated, quoted>
set -euo pipefail
D=${LAB_D:-/work/exp126-data/lab}
B=$D/target/release/exp122-lab
VARIANTS=${1:-"0 K T C P M D A KTCPMDA"}
export RAYON_NUM_THREADS=2
for v in $VARIANTS; do
  mkdir -p "$D/q/$v" "$D/ix/$v"
  canon=""; [ "$v" = 0 ] && canon="--canon"
  # query side: one process per bundle, 8 at a time x 2 threads
  awk -F'\t' '{print $2"\t"$3"\t"$4}' "$D/plan/queries.tsv" | sort -u |
    while IFS=$'\t' read -r id bundler path; do
      [ -s "$D/q/$v/$id.jsonl" ] || echo "$id $bundler $path"
    done | xargs -r -P 8 -L 1 sh -c "$B query $v \$1 \$2 $D/q/$v/\$0.jsonl.tmp $canon && mv $D/q/$v/\$0.jsonl.tmp $D/q/$v/\$0.jsonl"
  # index side: the planned builds in 16 chunks, 8 processes x 2 threads
  rm -rf "$D/ix/$v/chunks"; mkdir -p "$D/ix/$v/chunks"
  split -n r/16 -d "$D/plan/builds.txt" "$D/ix/$v/chunks/c"
  ls "$D/ix/$v/chunks"/c* | xargs -P 8 -I{} sh -c "$B index $v {}.jsonl $canon < {}"
  echo "variant $v done"
done
