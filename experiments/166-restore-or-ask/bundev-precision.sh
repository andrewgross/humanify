#!/usr/bin/env bash
# exp166: names-mode stub runs over the bun-dev corpus (source-map truth) and
# the vendor-placement precision check on each, then the aggregate. The whole
# batch holds /work/heavy.lock once and runs JOBS bundles at a time inside it.
# Each card judges BOTH worlds from the one run: the binary's exact
# placement and the retired ordinal conversion's (the dump's baseline).
#   usage: bundev-precision.sh <bin> <label> [jobs]
set -u
BIN="$1"; LABEL="$2"; JOBS="${3:-4}"
HERE="$(cd "$(dirname "$0")" && pwd)"
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
[ -x "$PY" ] || PY=python3
LIST="$HERE/../165-claim-boundary/bun-dev-list.tsv"
OUT="/work/exp166-data/bundev/$LABEL"
mkdir -p "$OUT"
one() {
  local id="$1" bundle="$2"
  [ -s "$OUT/$id.precision.json" ] && return 0
  npx tsx "$HERE/stub-run.mts" "$BIN" "$bundle" "$OUT/$id" > "$OUT/$id.run.txt" 2>&1 || { echo "RUN FAIL $id"; return 0; }
  "$PY" -I "$HERE/vendor_precision.py" "$OUT/$id/diag.json" --truth "bun-dev:$id" \
    > "$OUT/$id.precision.json" 2> "$OUT/$id.precision.err" || echo "CHECK FAIL $id"
  rm -rf "$OUT/$id/tree"
}
export -f one; export OUT BIN HERE PY
# HELD_LOCK=1: the caller already holds /work/heavy.lock for this batch.
LOCK=(flock /work/heavy.lock)
[ "${HELD_LOCK:-}" = 1 ] && LOCK=()
"${LOCK[@]}" bash -c "cut -f1,2 '$LIST' | xargs -P '$JOBS' -L1 bash -c 'one \"\$0\" \"\$1\"'"
"$PY" -I "$HERE/aggregate_precision.py" "$OUT" > "$OUT/aggregate.json"
echo "done $LABEL"
