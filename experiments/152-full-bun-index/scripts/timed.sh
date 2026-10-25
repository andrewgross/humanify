#!/usr/bin/env bash
# exp152: run a tool and append one timing line to $EXP152_TIMING_LOG:
#   <label> <start epoch s> <end epoch s> <exit code> <cwd>
# The builder reaches npm through a PATH shim and Bun through --bun, both of which call this, so the build's
# install and bundle wall times are measured per call without changing the builder.
# usage: timed.sh <label> <real binary> [args...]
label=$1 bin=$2
shift 2
start=$EPOCHREALTIME
"$bin" "$@"
rc=$?
printf '%s %s %s %s %s\n' "$label" "$start" "$EPOCHREALTIME" "$rc" "$PWD" >> "${EXP152_TIMING_LOG:?}"
exit $rc
