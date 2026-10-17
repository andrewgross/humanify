#!/usr/bin/env bash
# exp124 lab: a store that reuses every k2-all shard the region-aware rule cannot change, so a lab binary keyed
# like k2-all (unit-rows/4) rebuilds ONLY the module shards whose notes record a bundler-marker sponge drop (the
# only shards the rule changes), from the finished builds of a seeded work dir.
# usage: lab-store.sh <lab-data-dir>     (k2-all = /work/exp113-data; shared work dir /work/exp124-data/work)
set -euo pipefail
LAB=$1; SRC=/work/exp113-data; WORK=/work/exp124-data/work
mkdir -p "$LAB/store/shards" "$LAB/logs" "$WORK"
ln -sfn "$SRC/registry" "$LAB/registry"
ln -sfn "$WORK" "$LAB/work"
AFF=/work/exp124-data/affected.txt   # <slice>/<shard file>
if [ ! -s "$AFF" ]; then
  (cd "$SRC/store/shards" && for s in bun-bun-cjs-min bun-node-cjs-min esbuild-node-cjs-min rollup-node-cjs-min webpack-node-cjs-min; do
     grep -l '"bundler-marker"' -r "$s" ; done) | sort > "$AFF.tmp"
  mv "$AFF.tmp" "$AFF"
fi
for s in "$SRC"/store/shards/*; do
  b=$(basename "$s")
  [ -e "$LAB/store/shards/$b" ] || cp -al "$s" "$LAB/store/shards/$b"
done
# unlink the affected shards IN THE LAB COPY (the k2-all store keeps its link)
while read -r f; do rm -f "$LAB/store/shards/$f"; done < "$AFF"
cut -d/ -f2 "$AFF" | sed 's/\.json$//' | sort -u > "$LAB/versions.txt"
"$(dirname "$0")/seed-work.sh" "$LAB/versions.txt" "$WORK"
echo "affected shards $(wc -l < "$AFF"), versions $(cut -d/ -f2 "$AFF" | sort -u | wc -l)"
