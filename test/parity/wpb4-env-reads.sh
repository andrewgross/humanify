#!/usr/bin/env bash
# WPB.4 env-reads gate: the TS subcommand (npx tsx src/index.ts env-reads)
# and the Rust one (humanify env-reads) over the same inputs; the text and
# Markdown reports, stderr and exit codes must be byte-identical.
#
#   test/parity/wpb4-env-reads.sh <humanify-binary> [extra inputs...]
#
# Default inputs: the committed shape corpus (a file and a tree with
# .js/.mjs/.cjs, .humanify/ and node_modules/ to skip), a missing path.
# Extra inputs (the gate run adds the eight eval-pair bundles and a split
# tree) are appended. Exit 0 = identical everywhere.
set -u
BIN=${1:?usage: wpb4-env-reads.sh <humanify-binary> [inputs...]}
shift
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# The tree fixture (built here: .humanify/ and node_modules/ are gitignored).
mkdir -p "$WORK/tree/sub" "$WORK/tree/.humanify" "$WORK/tree/node_modules"
printf 'module.exports = process.env.TREE_A;\n' > "$WORK/tree/a.js"
printf 'export const b = process.env.TREE_B;\n' > "$WORK/tree/sub/b.mjs"
printf 'exports.c = process.env.TREE_C;\n' > "$WORK/tree/C.cjs"
printf 'exports.u = process.env.TREE_U;\n' > "$WORK/tree/_u.js"
printf 'ignored\n' > "$WORK/tree/notjs.txt"
printf 'process.env.HIDDEN;\n' > "$WORK/tree/.humanify/h.js"
printf 'process.env.NM;\n' > "$WORK/tree/node_modules/n.js"

inputs=("$HERE/wpb4-env-reads-cases.js" "$WORK/tree" "$WORK/does-not-exist" "$@")
cases=0
fails=0
for input in "${inputs[@]}"; do
  for md in "" "--markdown"; do
    cases=$((cases + 1))
    (cd "$REPO" && npx tsx src/index.ts env-reads "$input" $md) > "$WORK/ts.out" 2> "$WORK/ts.err"
    ts_rc=$?
    "$BIN" env-reads "$input" $md > "$WORK/rs.out" 2> "$WORK/rs.err"
    rs_rc=$?
    if [ "$ts_rc" = "$rs_rc" ] && cmp -s "$WORK/ts.out" "$WORK/rs.out" && cmp -s "$WORK/ts.err" "$WORK/rs.err"; then
      echo "IDENTICAL  $input $md (exit $rs_rc, $(wc -l < "$WORK/rs.out") lines)"
    else
      fails=$((fails + 1))
      echo "DIVERGES   $input $md (exit ts=$ts_rc rust=$rs_rc)"
      diff "$WORK/ts.out" "$WORK/rs.out" | head -20
      diff "$WORK/ts.err" "$WORK/rs.err" | head -10
    fi
  done
done
echo "env-reads: $((cases - fails))/$cases identical"
[ "$fails" = 0 ]
