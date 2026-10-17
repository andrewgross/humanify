#!/usr/bin/env bash
# exp124: seed a work dir with every listed install dir name (<name with __>@<version>) from the earlier work dirs.
# usage: seed-work.sh <list> <work dir>
set -euo pipefail
LIST=$1; WORK=$2; SRC=/work/exp113-data
mkdir -p "$WORK"
# seed the work dir for the listed versions: manifest hard-linked, node_modules SYMLINKED (read only: an
# .installed dir is never reinstalled), each FINISHED build hard-linked (a stamped build is never rebuilt; an
# unstamped one is not seeded, so a retry writes only here)
SOURCES="$SRC/work /work/exp109-data/work /work/exp101-data/work /work/exp095-index-v2-data/selABC/work /work/exp105-data/work"
while read -r dir; do
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
done < "$LIST"
