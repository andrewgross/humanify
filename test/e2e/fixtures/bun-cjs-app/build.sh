#!/usr/bin/env bash
# Rebuild bun-cjs-app's two committed builds from source/ (PINNED: Bun
# 1.3.14; `bun build bin/cli.js --target=bun --format=cjs --minify`: Bun's
# wrapped CommonJS layout, minified as in the bun-dev corpus. The bin only
# requires the app, so the app text outside the factories is `require_cli();`).
# Each version is built from inside a scratch copy. No npm dependencies:
# the library is a local `lib/tinylib/` (node_modules/ is gitignored).
#   PATH="$HOME/.bun/bin:$PATH" bash test/e2e/fixtures/bun-cjs-app/build.sh
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
  (cd "$SCRATCH/v$v" && bun build bin/cli.js --target=bun --format=cjs --minify \
    --outfile="$HERE/build/v$v/build/index.js" > /dev/null)
done
sha256sum "$HERE"/build/v*/build/index.js | sed "s#$HERE/##"
