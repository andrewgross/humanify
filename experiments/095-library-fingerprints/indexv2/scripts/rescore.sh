#!/usr/bin/env bash
# Score every held-out app whose score file is missing (score_claims.py), one at a time.
# usage: rescore.sh <out dir> <cpu-list>
set -u
OUT=$1; CPUS=$2; E=/work/exp095-index-v2-data/eval; V=/work/exp095-library-fingerprints/scale/.venv/bin/python; S=$(dirname "$0")
python3 -c "
import json
for slug, js, app in json.load(open('$E/heldout-apps.json')): print(slug, js, app)
" | while read -r slug js app; do
  [ -f "$OUT/heldout/$slug.score.json" ] && continue
  taskset -c "$CPUS" "$V" -I "$S/score_claims.py" "$js" "$js.map" "$OUT/heldout/$slug.units.tsv" "$OUT/heldout/$slug.score.json" "$app" \
    "$OUT/heldout/$slug.struct.json" "$OUT/heldout/$slug.agree.json" > /dev/null 2>> "$OUT/heldout/score.log"
done
echo rescored
