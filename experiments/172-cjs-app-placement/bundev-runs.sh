#!/usr/bin/env bash
# exp172 phase 2: stub-LLM runs of one binary over the bun-dev corpus
# (exp166's stub-run.mts, names or default mode), keeping per bundle the
# diagnostics (appModules = the factories kept as app modules, raw spans;
# eraRestoration.factoryFiles = the vendored ones, names mode) and a tree
# summary (every output file's path and size), never the tree itself.
# The whole batch holds /work/heavy.lock once and runs JOBS bundles at a time.
#   usage: bundev-runs.sh <bin> <label> <names|default> [jobs]
set -u
BIN="$1"; LABEL="$2"; MODE="$3"; JOBS="${4:-4}"
HERE="$(cd "$(dirname "$0")" && pwd)"
LIST="$HERE/../165-claim-boundary/bun-dev-list.tsv"
OUT="/work/exp172-data/bundev/$LABEL"
mkdir -p "$OUT"
one() {
  local id="$1" bundle="$2"
  [ -s "$OUT/$id/tree.tsv" ] && return 0
  npx tsx "$HERE/../166-restore-or-ask/stub-run.mts" "$BIN" "$bundle" "$OUT/$id" --mode "$MODE" \
    > "$OUT/$id.run.txt" 2>&1 || echo "RUN FAIL $id"
  if [ -d "$OUT/$id/tree" ]; then
    (cd "$OUT/$id/tree" && find . -type f -not -path './.humanify/*' -printf '%P\t%s\n' | sort) > "$OUT/$id/tree.tsv"
    rm -rf "$OUT/$id/tree"
  fi
}
export -f one; export OUT BIN HERE MODE
LOCK=(flock /work/heavy.lock)
[ "${HELD_LOCK:-}" = 1 ] && LOCK=()
"${LOCK[@]}" bash -c "cut -f1,2 '$LIST' | xargs -P '$JOBS' -L1 bash -c 'one \"\$0\" \"\$1\"'"
echo "done $LABEL"
