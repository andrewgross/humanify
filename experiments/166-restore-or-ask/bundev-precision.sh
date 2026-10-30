#!/usr/bin/env bash
# exp166: names-mode stub runs over the bun-dev corpus (source-map truth) and
# the vendor-placement precision check on each, then the aggregate. The whole
# batch holds /work/heavy.lock once and runs JOBS bundles at a time inside it.
# A binary without the raw-to-era bridge (main's logic) is judged with the
# bridge spans of a bridged label's run of the same bundle (RAW_FROM).
#   usage: bundev-precision.sh <bin> <label> [jobs] [raw-from-label]
set -u
BIN="$1"; LABEL="$2"; JOBS="${3:-4}"; RAW_FROM="${4:-}"
HERE="$(cd "$(dirname "$0")" && pwd)"
LIST="$HERE/../165-claim-boundary/bun-dev-list.tsv"
OUT="/work/exp166-data/bundev/$LABEL"
mkdir -p "$OUT"
one() {
  local id="$1" bundle="$2"
  [ -s "$OUT/$id.precision.json" ] && return 0
  npx tsx "$HERE/stub-run.mts" "$BIN" "$bundle" "$OUT/$id" > "$OUT/$id.run.txt" 2>&1 || { echo "RUN FAIL $id"; return 0; }
  local extra=()
  [ -n "$RAW_FROM" ] && extra=(--raw-from "/work/exp166-data/bundev/$RAW_FROM/$id/diag.json")
  python3 -I "$HERE/vendor_precision.py" "$OUT/$id/diag.json" --truth "bun-dev:$id" "${extra[@]}" \
    > "$OUT/$id.precision.json" 2> "$OUT/$id.precision.err" || echo "CHECK FAIL $id"
  rm -rf "$OUT/$id/tree"
}
export -f one; export OUT BIN HERE RAW_FROM
# HELD_LOCK=1: the caller already holds /work/heavy.lock for this batch.
LOCK=(flock /work/heavy.lock)
[ "${HELD_LOCK:-}" = 1 ] && LOCK=()
"${LOCK[@]}" bash -c "cut -f1,2 '$LIST' | xargs -P '$JOBS' -L1 bash -c 'one \"\$0\" \"\$1\"'"
python3 -I "$HERE/aggregate_precision.py" "$OUT" > "$OUT/aggregate.json"
echo "done $LABEL"
