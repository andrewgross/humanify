#!/usr/bin/env bash
# exp113: seed a work dir for one version from earlier builds, ALL hard-linked: the install of the first source that
# finished one (.installed; package.json, lock, node_modules), then each build/<slice>/<tag> from the first source
# whose copy is FINISHED (has its .recipe stamp). Safe to link: the builder never writes through a seeded file. A
# rebuild calls remove_dir_all on its out dir first and writes its stamp fresh; entries are written by write_fresh
# (remove, then write); an install is kept as is once .installed exists.
# usage: seed-work.sh <dst-work> <name@version> <src-work>...
set -uo pipefail
DST=$1; spec=$2; shift 2
dir="${spec%@*}"; dir="${dir//\//__}@${spec##*@}"
[ -f "$DST/$dir/.installed" ] && exit 0
for src in "$@"; do
  [ -f "$src/$dir/.installed" ] || continue
  mkdir -p "$DST/$dir.tmp/build"
  for f in package.json package-lock.json node_modules; do
    [ -e "$src/$dir/$f" ] && cp -al "$src/$dir/$f" "$DST/$dir.tmp/$f"
  done
  for s in "$@"; do
    for t in "$s/$dir/build/"*/*/; do
      [ -f "$t.recipe" ] || continue
      rel=${t#"$s/$dir/build/"}; rel=${rel%/}
      [ -e "$DST/$dir.tmp/build/$rel" ] && continue
      mkdir -p "$DST/$dir.tmp/build/${rel%/*}"
      cp -al "${t%/}" "$DST/$dir.tmp/build/$rel"
    done
  done
  touch "$DST/$dir.tmp/.installed"
  mv "$DST/$dir.tmp" "$DST/$dir"
  break
done
exit 0
