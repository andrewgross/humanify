#!/usr/bin/env bash
# view-diff.sh <base-tree> <new-tree>
# Scoped, filtered unified diff between two humanify split trees — the same
# scoping build-history-repo.sh uses (drops vendor/.humanify/scaffold) and the
# same version/sha/build-time -I filters. Pipe to a pager, or into
# classify-tokens.py / alias-classify.py.
#
# Trees are directories containing src/ (either archive checkouts or the -o
# output of a `--split --prior-version` run).
set -uo pipefail
BASE="${1:?usage: view-diff.sh <base-tree> <new-tree>}"
NEW="${2:?usage: view-diff.sh <base-tree> <new-tree>}"
scope() {
  rsync -a \
    --exclude='.humanify' --exclude='vendor' --exclude='node_modules' \
    --exclude='package-lock.json' --exclude='run.cjs' --exclude='index.js' \
    --exclude='RUNNABLE.md' --exclude='package.json' \
    "$1/" "$2/"
}
A="$(mktemp -d)/a"; B="$(mktemp -d)/b"; mkdir -p "$A" "$B"
scope "$BASE" "$A"; scope "$NEW" "$B"
git --no-pager -c core.fileMode=false diff --no-index \
  -I 'VERSION: "2\.1\.' -I 'GIT_SHA:' -I 'BUILD_TIME:' "$A" "$B"
exit 0
