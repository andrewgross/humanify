#!/usr/bin/env bash
# exp096 library-matching score: build the binary at a commit, run `humanify discover` with one index and one
# claim rule over the fixed corpus (corpus.json), score it into results/<label>/.
#   experiments/096-library-matching-score/run.sh <label> --index <dir> --rule agreement|structural \
#       [--commit <ref>] [--jobs N] [--cpus LIST] [--force-mixed]
# The scorer needs numpy/scipy (exp095's family table reader): the exp095 labs' venv.
set -euo pipefail
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
exec "$PY" -I "$(dirname "$0")/run.py" "$@"
