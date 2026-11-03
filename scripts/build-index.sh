#!/usr/bin/env bash
# Build a library fingerprint index from its spec, chunked (exp184).
#
#   scripts/build-index.sh <spec> <registry> <store> <work> <out> [N]
#
# The build subcommand holds the plan and the execution, but a full
# default-spec build is hours of installs and bundles, so this script
# splits it into N chunked invocations (default 4) plus a final merge:
# each chunk resolves the plan in FULL (the plan is the same everywhere —
# the registry snapshot is fetched once, in chunk 0), builds only its
# packages' shards into <store>, and writes nothing to <out>; the final
# `--merge-only` merges every shard the store holds.
#
# Heavy jobs run ALONE (parallel heavy jobs have OOM-killed runs), so the
# whole script holds /work/heavy.lock for its duration. It also takes
# /work/index-build.lock — a second lock file, not a nesting of the same
# one — so two index builds of any kind cannot share a store.
#
# No other input than the spec, the registry snapshot (fetched once, into
# <registry>) and the public npm registry: see the spec itself
# (crates/humanify-index/specs/default.toml) for what the rules are.
set -euo pipefail

if [ "$#" -lt 5 ] || [ "$#" -gt 6 ]; then
  echo "usage: $0 <spec> <registry> <store> <work> <out> [chunks]" >&2
  exit 2
fi
SPEC=$1 REGISTRY=$2 STORE=$3 WORK=$4 OUT=$5
CHUNKS=${6:-4}
JOBS=${JOBS:-4}
MERGE_JOBS=${MERGE_JOBS:-1}
BIN=target/release/humanify-index

export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
# -p humanify-INDEX: that is the builder binary this script runs (-p
# humanify-cli builds the pipeline AND humanify-index's lib, but not its
# binary — a stale binary silently re-runs the previous code).
cargo build --release --locked -p humanify-index

mkdir -p "$REGISTRY" "$WORK"
(
  flock 9
  flock 8
  # The registry snapshot first: the plan's resolution fetches every
  # packument the universe needs. npm rate-limits a bulk view sweep, so
  # the fetch is RETRIED until it completes (each attempt resumes —
  # fetched packuments are never re-fetched; a missing packument the
  # registry does not know is skipped and recorded). No build happens:
  # --dry-run prints the plan and stops.
  until $BIN build --spec "$SPEC" --registry "$REGISTRY" --store "$STORE" \
      --work "$WORK" --dry-run; do
    echo "registry fetch incomplete; retrying in 5 minutes"
    sleep 300
  done
  for i in $(seq 0 $((CHUNKS - 1))); do
    $BIN build --spec "$SPEC" --registry "$REGISTRY" --store "$STORE" \
      --work "$WORK" --jobs "$JOBS" --chunk "$i/$CHUNKS" --offline
  done
  $BIN build --spec "$SPEC" --registry "$REGISTRY" --store "$STORE" \
    --work "$WORK" --merge-jobs "$MERGE_JOBS" --merge-only --out "$OUT"
) 9>/work/heavy.lock 8>/work/index-build.lock
echo "index written to $OUT"
