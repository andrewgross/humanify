#!/usr/bin/env bash
#
# MOVED. The tree-level draw-pinned self-hop now lives at experiments/lib/selfhop.sh, so
# every experiment shares one implementation instead of forking it — exp058
# needed a flag this script had hard-coded, and the fix reached only this copy.
#
# This shim stays because 054/RESULTS.md and 058/RESULTS.md cite the old path,
# and a published result should keep resolving to the thing that produced it.
# All arguments and environment variables are unchanged.
exec "$(cd "$(dirname "$0")/../lib" && pwd)/selfhop.sh" "$@"
