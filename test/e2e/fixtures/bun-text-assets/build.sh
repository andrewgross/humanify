#!/usr/bin/env bash
# Rebuild bun-text-assets' two committed builds from source/ (PINNED: Bun
# 1.3.14; unminified `--format=cjs`, Claude Code's own layout). Each version
# is built from inside a scratch copy (Bun bakes `// src/…` comments relative
# to the working directory). No npm dependencies.
#   PATH="$HOME/.bun/bin:$PATH" bash test/e2e/fixtures/bun-text-assets/build.sh
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
if [ "$(bun --version)" != "1.3.14" ]; then
  echo "bun 1.3.14 required, found $(bun --version)" >&2
  exit 1
fi
SCRATCH=$(mktemp -d)
trap 'rm -rf "$SCRATCH"' EXIT
for v in 1.0.0 1.1.0; do
  cp -r "$HERE/source/v$v" "$SCRATCH/v$v"
  mkdir -p "$HERE/build/v$v/build"
  (cd "$SCRATCH/v$v" && bun build src/main.js --target=bun --format=cjs \
    --outfile="$HERE/build/v$v/build/index.js" > /dev/null)
done
sha256sum "$HERE"/build/v*/build/index.js | sed "s#$HERE/##"
