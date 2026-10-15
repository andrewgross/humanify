#!/usr/bin/env bash
# exp109: seed a work dir for one version from earlier builds: install hard-linked (package.json, lock,
# node_modules, .installed), each finished build/<slice> COPIED (a rebuild must never write through a link).
# The first source holding an install wins; webpack/rollup builds come from exp105's work.
set -uo pipefail
spec=$1; DST=/work/exp109-data/work
dir="${spec%@*}"; dir="${dir//\//__}@${spec##*@}"
[ -f "$DST/$dir/.installed" ] && exit 0
for src in /work/exp101-data/work /work/exp095-index-v2-data/selABC/work /work/exp105-data/work; do
  [ -f "$src/$dir/.installed" ] || continue
  mkdir -p "$DST/$dir/build"
  for f in package.json package-lock.json node_modules; do
    [ -e "$src/$dir/$f" ] && cp -al "$src/$dir/$f" "$DST/$dir/$f"
  done
  break
done
[ -d "$DST/$dir" ] || exit 0
for src in /work/exp101-data/work /work/exp095-index-v2-data/selABC/work /work/exp105-data/work; do
  for b in "$src/$dir/build/"*-cjs-min; do
    [ -f "$b/req/.recipe" ] || continue
    s=$(basename "$b"); [ -e "$DST/$dir/build/$s" ] || cp -a "$b" "$DST/$dir/build/$s"
  done
done
touch "$DST/$dir/.installed"
