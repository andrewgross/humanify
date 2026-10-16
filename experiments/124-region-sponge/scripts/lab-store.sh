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
# seed the work dir for the affected versions: manifest hard-linked, node_modules SYMLINKED (read only: an
# .installed dir is never reinstalled), each FINISHED build hard-linked (a stamped build is never rebuilt; an
# unstamped one is not seeded, so a retry writes only here)
SOURCES="$SRC/work /work/exp109-data/work /work/exp101-data/work /work/exp095-index-v2-data/selABC/work /work/exp105-data/work"
cut -d/ -f2 "$AFF" | sed 's/\.json$//' | sort -u | while read -r dir; do
  [ -f "$WORK/$dir/.installed" ] && continue
  from=""
  for src in $SOURCES; do [ -f "$src/$dir/.installed" ] && { from="$src/$dir"; break; }; done
  [ -n "$from" ] || { echo "no install: $dir" >&2; continue; }
  mkdir -p "$WORK/$dir/build"
  for f in package.json package-lock.json; do
    [ -e "$from/$f" ] && cp -al "$from/$f" "$WORK/$dir/$f"
  done
  [ -e "$from/node_modules" ] && ln -sfn "$from/node_modules" "$WORK/$dir/node_modules"
  for src in $SOURCES; do  # the first source with a finished build of each slice/tag (seed-work.sh's order)
    for b in "$src/$dir"/build/*/*; do
      [ -f "$b/.recipe" ] || continue
      rel=${b#$src/$dir/}
      [ -e "$WORK/$dir/$rel" ] && continue
      mkdir -p "$WORK/$dir/$(dirname "$rel")"
      cp -al "$b" "$WORK/$dir/$rel"
    done
  done
  touch "$WORK/$dir/.installed"
done
echo "affected shards $(wc -l < "$AFF"), versions $(cut -d/ -f2 "$AFF" | sort -u | wc -l)"
