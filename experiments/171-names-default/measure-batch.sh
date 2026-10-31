#!/usr/bin/env bash
# exp171: the default run's library-index cost, calibrated. For each bundle:
# COLD (an empty store; only the pinned Bun pre-placed in tools/), WARM (the
# same store with its merged index removed: every shard is reused, only the
# plan + merge run again), and FOUND (the merged index left in place: the
# search finds it, nothing is built). The found path against the full
# prebuilt index on Claude Code was measured once with the pre-flip binary
# (the path is unchanged): measure-preflip/cc216-prebuilt.
# Holds /work/heavy.lock for the whole batch; one bundle at a time.
#   usage: measure-batch.sh <bin> <data-dir> [--pre-flip]
set -u
# the whole batch holds the heavy lock once (re-exec under it)
if [ -z "${EXP171_HELD:-}" ]; then
  exec flock /work/heavy.lock env EXP171_HELD=1 "$0" "$@"
fi
BIN="$1"; DATA="$2"; shift 2
EXTRA=("$@")
HERE="$(cd "$(dirname "$0")" && pwd)"
BUN_TOOL=/work/tools/bun-1.4.0
CC=/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js
BUNDEV=/work/exp150-data/bun/build
mkdir -p "$DATA"
measure() { # <tag> <input> <store> [args...]
  local tag="$1" input="$2" store="$3"; shift 3
  echo "=== $tag $(date -u +%FT%TZ)"
  npx tsx "$HERE/measure-index.mts" "$BIN" "$input" "$DATA/$tag" "$store" "$@" \
    > "$DATA/$tag.out" 2>&1
  echo "    exit $? -> $DATA/$tag/card.json"
  rm -rf "$DATA/$tag/tree"
}
one_bundle() { # <name> <input> [nobun]  (nobun: the cold run fetches Bun itself)
  local name="$1" input="$2" store="$DATA/store-$1"
  rm -rf "$store"; mkdir -p "$store/tools"
  [ "${3:-}" = nobun ] || cp -a "$BUN_TOOL" "$store/tools/"
  measure "$name-cold" "$input" "$store" ${EXTRA[@]+"${EXTRA[@]}"}
  measure "$name-found" "$input" "$store" ${EXTRA[@]+"${EXTRA[@]}"}
  # warm: keep store/ (the shards), registry/, tools/; drop the merged index
  find "$store" -mindepth 1 -maxdepth 1 -type d -regextype posix-extended \
    -regex '.*/[0-9a-f]{64}' -exec rm -rf {} +
  measure "$name-warm" "$input" "$store" ${EXTRA[@]+"${EXTRA[@]}"}
}
# doctoc reveals no package (the inert path); the other three do, at three sizes
measure doctoc-inert "$BUNDEV/bun-bun/cli_doctoc-fa09a2/entry.js" "$DATA/store-doctoc" ${EXTRA[@]+"${EXTRA[@]}"}
one_bundle bettersvg "$BUNDEV/bun-node/vscode_gh-midudev__better-svg-2d5738/entry.js" nobun
one_bundle novel "$BUNDEV/bun-bun/electron_gh-john-paul-ruf__novel-engine-3b897e/entry.js"
one_bundle inlay "$BUNDEV/bun-node/vscode_gh-antfu__vscode-array-index-inlay-336411/entry.js"
# Claude Code reveals no npm package name (two `azure/msal-…` banners
# without the `@`): the default run is inert — no build. Measured as a run.
measure cc216-inert "$CC" "$DATA/store-cc216" ${EXTRA[@]+"${EXTRA[@]}"}
echo "done"
