#!/usr/bin/env bash
#
# MOVED. The draw-pinned four-pair A/B now lives at experiments/lib/gate.sh, so
# every experiment shares one implementation instead of forking it — exp058
# needed a flag this script had hard-coded, and the fix reached only this copy.
#
# This shim stays because 054/RESULTS.md and 058/RESULTS.md cite the old path,
# and a published result should keep resolving to the thing that produced it.
# All arguments and environment variables are unchanged.
exec "$(cd "$(dirname "$0")/../lib" && pwd)/gate.sh" "$@"
