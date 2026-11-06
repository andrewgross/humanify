#!/usr/bin/env bash
# exp189: score the isolation veto's existing relaxation `library.discovery.veto_support = "all-rules"` (a registry
# key since exp143; config only, no code change) on the Bun corpora and cc, with exp096's runner at main's commit.
# ONE lock layer: the caller wraps this whole script in `flock /work/heavy.lock`; nothing inside takes the lock.
#   flock /work/heavy.lock experiments/189-dropped-seeds/run-variants.sh <commit>
# Control: exp188-ctl-* (the default at the same commit).
set -euo pipefail
COMMIT=${1:?commit}
HERE=$(cd "$(dirname "$0")" && pwd)
RUN=$HERE/../096-library-matching-score/run.sh
INDEX=$(jq -r .libraryIndex "$HERE/../034-eval-harness/pairs.json")
for corpus in bun-dev bun-held cc; do
  echo "=== exp189-allrules-$corpus"
  "$RUN" "exp189-allrules-$corpus" --index "$INDEX" --rule structural --corpus "$corpus" --commit "$COMMIT" \
    --jobs 8 --cpus 0-31 --pipeline-set library.discovery.veto_support=all-rules
done
echo "ALL DONE"
