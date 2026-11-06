#!/usr/bin/env bash
# exp188: score the cover-threshold variants with exp096's runner. ONE lock layer: the caller wraps this whole
# script in `flock /work/heavy.lock`; nothing inside takes the lock.
#   flock /work/heavy.lock experiments/188-partial-claims/run-variants.sh <commit>
# Labels: exp188-<variant>-<corpus>. `ctl` = today's default (cover 0.5) at the same commit, so the variants differ
# from it only by `library.lookup.cover` (an existing registry key; no code change).
set -euo pipefail
COMMIT=${1:?commit}
HERE=$(cd "$(dirname "$0")" && pwd)
RUN=$HERE/../096-library-matching-score/run.sh
INDEX=$(jq -r .libraryIndex "$HERE/../034-eval-harness/pairs.json")
for corpus in cc bun-held bun-dev; do
  for v in ctl c30 c10; do
    case $v in
      ctl) set_args=() ;;
      c30) set_args=(--pipeline-set library.lookup.cover=0.3) ;;
      c10) set_args=(--pipeline-set library.lookup.cover=0.1) ;;
    esac
    echo "=== exp188-$v-$corpus"
    "$RUN" "exp188-$v-$corpus" --index "$INDEX" --rule structural --corpus "$corpus" --commit "$COMMIT" \
      --jobs 8 --cpus 0-31 "${set_args[@]}"
  done
done
echo "ALL DONE"
