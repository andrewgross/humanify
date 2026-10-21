#!/usr/bin/env bash
# exp133: the build's store and registry, hard-linked (the builder writes shards with write-then-rename, so a link is
# never written through). exp130's shards win; exp118's fill only what exp130 lacks (cp -n).
# usage: assemble-store.sh <data-dir>
set -euo pipefail
D=$1
mkdir -p "$D/store/shards" "$D/logs" "$D/work"
cp -al /work/exp118-data/idx/registry "$D/registry"
for s in /work/exp130-data/full/store/shards/*; do cp -al "$s" "$D/store/shards/"; done
for s in /work/exp118-data/idx/store/shards/*; do cp -aln "$s" "$D/store/shards/"; done
