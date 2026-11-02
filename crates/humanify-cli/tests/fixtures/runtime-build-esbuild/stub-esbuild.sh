#!/bin/sh
# The test's stand-in for esbuild 0.28.2 (the real binary is an 11 MB
# download the gate never makes): it answers --version, which the plan
# keys the shards by, and REFUSES to build — the committed shard must
# serve, so a build that reaches the bundler fails loudly.
if [ "$1" = "--version" ]; then
  echo 0.28.2
  exit 0
fi
echo "stub esbuild: asked to build ($*), but the committed shard should have served" >&2
exit 1
