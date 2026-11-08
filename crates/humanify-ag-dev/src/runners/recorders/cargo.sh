#!/usr/bin/env bash
echo "cargo :: $* (in $PWD)" >> "$RECORDER_ROOT/calls.log"
pkg="" prev=""
for a in "$@"; do [[ "$prev" == "-p" ]] && pkg="$a"; prev="$a"; done
case "$pkg" in
  humanify-ag-cli) bin=humanify-ag ;;
  humanify-cli) bin=humanify ;;
  *) echo "cargo recorder: unknown package $pkg" >&2; exit 1 ;;
esac
mkdir -p target/release
cp "$RECORDER_ROOT/bin-recorder" "target/release/$bin"
chmod +x "target/release/$bin"
