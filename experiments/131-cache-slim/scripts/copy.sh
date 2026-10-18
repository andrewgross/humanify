#!/usr/bin/env bash
# exp131: copy (real copies, never links) each sampled install's builds from exp130's work dir into /work/exp131-data/work
set -euo pipefail
SRC=/work/exp130-data/work; DST=/work/exp131-data/work
while read -r p; do
  [ -z "$p" ] && continue
  d="${p%@*}"; d="${d//\//__}@${p##*@}"
  s=$SRC/$d; t=$DST/$d
  [ -e "$t" ] && continue
  mkdir -p "$t"
  cp -a "$s/build" "$s/entries" "$t/"
  for f in package.json package-lock.json .installed; do [ -e "$s/$f" ] && cp -a "$s/$f" "$t/"; done
  if [ -L "$s/node_modules" ]; then cp -a "$s/node_modules" "$t/node_modules"; else ln -s "$s/node_modules" "$t/node_modules"; fi
done < "$1"
echo COPY DONE
