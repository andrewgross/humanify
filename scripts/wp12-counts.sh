#!/usr/bin/env bash
# WP1.2's gate: the ingest counts table — the Rust oxc counts beside Babel's
# over the same TS-beautified fresh texts. Run after the oracle dumps exist.
# Output: /work/rust-port/gates/wp1.2/<date>-counts.txt + stdout.
set -u
REPO=/Users/andrewgross/Development/humanify
ORACLE=${1:-/work/oracle/oracle-0294b28}
GATES=/work/rust-port/gates/wp1.2
mkdir -p "$GATES"
OUT="$GATES/$(date +%F)-counts.txt"
BIN="$REPO/target/debug/humanify"
cd "$REPO"
{
  echo "# WP1.2 ingest counts — oxc (Rust) vs Babel (TS), same beautified texts"
  echo "# oracle: $ORACLE  date: $(date +%F)"
  echo
  printf "%-18s %10s %10s %10s %12s | %10s %10s %10s\n" pair sym_oxc scope_oxc ref_oxc wrap_oxc sym_bab scope_bab ref_bab
  for pair in 2.1.85-2.1.86 2.1.118-2.1.119 2.1.197-2.1.198 2.1.215-2.1.216; do
    F="$ORACLE/dumps/$pair/text/fresh.js"
    [ -f "$F" ] || { echo "$pair: fresh.js ABSENT"; continue; }
    RUST=$("$BIN" ingest "$F")
    BABEL=$(npx tsx test/parity/babel-counts.mjs "$pair" 2>/dev/null | tail -1)
    RS=$(echo "$RUST" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['symbols'], d['scopes'], d['references'], d['wrapper_statements'])")
    BS=$(echo "$BABEL" | grep -oP "symbols=\K[0-9]+")
    BC=$(echo "$BABEL" | grep -oP "scopes=\K[0-9]+")
    BR=$(echo "$BABEL" | grep -oP "references=\K[0-9]+")
    printf "%-18s %s | %s %s %s\n" "$pair" "$RS" "$BS" "$BC" "$BR"
  done
} | tee "$OUT"
