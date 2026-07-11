#!/bin/bash
# Present two split trees: folder tree, cross-version diff, and the
# file-length distribution.  usage: present-split.sh <priorTree> <newTree>
set -euo pipefail
PRIOR=$1
NEW=$2

app_files() { # ledger-listed app files only (excludes co-located lib files)
  python3 -c "import json,sys; [print(f) for f in json.load(open('$1/_split-ledger.json'))['files']]"
}

echo "=================================================================="
echo "FILE TREE (new leg) — folders with file counts, sample contents"
echo "=================================================================="
app_files "$NEW" | awk -F/ '{print $1}' | sort | uniq -c | sort -rn |
  awk '{printf "  %-42s %s files\n", $2"/", $1}'
echo
for folder in $(app_files "$NEW" | awk -F/ '{print $1}' | sort -u | head -3); do
  echo "  $folder/"
  app_files "$NEW" | grep "^$folder/" | head 2>/dev/null | sed 's|^|    |'
done

echo
echo "=================================================================="
echo "CROSS-VERSION DIFF (git diff -M --stat style)"
echo "=================================================================="
git diff --no-index --name-status -M "$PRIOR" "$NEW" 2>/dev/null |
  grep -v "_split-ledger" | awk '{print substr($1,1,1)}' | sort | uniq -c |
  while read -r n s; do
    case $s in
      M) label="modified in place" ;;
      R) label="RENAMED (churn!)" ;;
      A) label="added" ;;
      D) label="deleted" ;;
      *) label=$s ;;
    esac
    printf "  %-5s %s\n" "$n" "$label"
  done || true
echo
echo "  top 12 files by diff size (hunks):"
while read -r f; do
  if [ -f "$PRIOR/$f" ] && [ -f "$NEW/$f" ]; then
    h=$(diff "$PRIOR/$f" "$NEW/$f" 2>/dev/null | grep -c '^[0-9]' || true)
    if [ "$h" -gt 0 ]; then echo "$h $f"; fi
  fi
done < <(app_files "$NEW") | sort -rn | head -12 | awk '{printf "  %5s  %s\n", $1, $2}'
total=0; identical=0; nfiles=0
while read -r f; do
  nfiles=$((nfiles+1))
  if [ -f "$PRIOR/$f" ] && cmp -s "$PRIOR/$f" "$NEW/$f"; then identical=$((identical+1)); fi
  h=$(diff "$PRIOR/$f" "$NEW/$f" 2>/dev/null | grep -c '^[0-9]' || true)
  total=$((total+h))
done < <(app_files "$NEW")
echo
echo "  total hunks across tree: $total   byte-identical files: $identical/$nfiles"

echo
echo "=================================================================="
echo "FILE LENGTH DISTRIBUTION (new leg, lines)"
echo "=================================================================="
while read -r f; do wc -l < "$NEW/$f"; done < <(app_files "$NEW") | sort -n |
  python3 -c "
import sys
xs = [int(l) for l in sys.stdin]
buckets = [(0,500),(500,1000),(1000,2000),(2000,3000),(3000,4000),(4000,10**9)]
for lo,hi in buckets:
    n = sum(1 for x in xs if lo <= x < hi)
    label = f'{lo}-{hi}' if hi < 10**9 else f'{lo}+'
    print(f'  {label:>12} lines: {n:4d} files ' + '#' * (n // 2))
import statistics
print(f'\n  files={len(xs)}  total={sum(xs):,}  min={xs[0]}  median={int(statistics.median(xs))}  p90={xs[int(len(xs)*0.9)]}  max={xs[-1]}')
"
