#!/usr/bin/env bash
# Library discovery with an index on every held-out app bundle (Bun 1.4.0 + sourcemap), both claim rules, then the
# exact sourcemap score (lane "indexv2").
# usage: run_heldout.sh <index dir> <out dir> <cpu-list>     env RULES (default "struct agree")
set -u
IX=$1; OUT=$2; CPUS=$3
E=/work/exp095-index-v2-data/eval
T=${PROTO:-/tmp/claude-1000/-Users-andrewgross-Development-humanify/a3a6231d-5923-4c13-976d-1f7a4f545a0f/scratchpad/v2/target/release}
V=/work/exp095-library-fingerprints/scale/.venv/bin/python
S=$(dirname "$0")
mkdir -p "$OUT/heldout"
python3 -c "
import json
for slug, js, app in json.load(open('$E/heldout-apps.json')):
    for rule in '${RULES:-struct agree}'.split():
        print(f'{js}\t$OUT/heldout/{slug}.{rule}.json\t$OUT/heldout/{slug}.units.tsv', file=open(f'$OUT/heldout/jobs-{rule}.tsv', 'a'))
"
for rule in ${RULES:-struct agree}; do
  A=1; [ $rule = struct ] && A=0
  RAYON_NUM_THREADS=8 INDEX=$IX AGREEMENT=$A taskset -c "$CPUS" "$T/exp095-core-discover" --many "$OUT/heldout/jobs-$rule.tsv" \
    > "$OUT/heldout/run-$rule.log" 2>&1 &
done
wait
python3 -c "
import json
for slug, js, app in json.load(open('$E/heldout-apps.json')):
    print(slug, js, app)
" | while read -r slug js app; do
  reps=(); for rule in ${RULES:-struct agree}; do reps+=("$OUT/heldout/$slug.$rule.json"); done
  taskset -c "$CPUS" "$V" -I "$S/score_claims.py" "$js" "$js.map" "$OUT/heldout/$slug.units.tsv" "$OUT/heldout/$slug.score.json" "$app" \
    "${reps[@]}" > /dev/null 2>> "$OUT/heldout/score.log"
done
echo done
