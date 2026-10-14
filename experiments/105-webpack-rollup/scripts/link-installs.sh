#!/usr/bin/env bash
# exp105: seed a work dir with HARD-LINKED copies of installs another build already made (package.json, lock,
# node_modules, the .installed stamp; never build/ or entries/): the builder then skips npm for them. Hard links
# are safe here: the builder replaces entries with fresh files and writes builds into new directories, and the
# bundlers only read node_modules.
# usage: link-installs.sh <versions.txt (name@version per line)> <source work dir> <dest work dir>
set -euo pipefail
[ $# -eq 3 ] || { sed -n 2,6p "$0"; exit 2; }
LIST=$1; SRC=$2; DST=$3
mkdir -p "$DST"
link_one() {
  local spec=$1 src=$2 dst=$3
  local dir="${spec%@*}"; dir="${dir//\//__}@${spec##*@}"
  [ -f "$src/$dir/.installed" ] || return 0
  [ -f "$dst/$dir/.installed" ] && return 0
  rm -rf "${dst:?}/$dir"; mkdir -p "$dst/$dir"
  for f in package.json package-lock.json node_modules; do
    [ -e "$src/$dir/$f" ] && cp -al "$src/$dir/$f" "$dst/$dir/$f"
  done
  touch "$dst/$dir/.installed"
}
export -f link_one
xargs -a "$LIST" -P 8 -I{} bash -c 'link_one "$@"' _ {} "$SRC" "$DST"
echo "linked: $(find "$DST" -maxdepth 2 -name .installed | wc -l) installs in $DST"
