#!/bin/bash
# Build `require('highlight.js')` (all grammars) for many highlight.js versions with Bun 1.4.0 --minify,
# to test recall against Claude Code's 188 grammar factories. Re-runnable.
set -e
HERE=/work/exp095-library-fingerprints/exact-module-hash/hljs
BUN=/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun
for v in "$@"; do
  d=$HERE/v$v
  mkdir -p $d
  if [ ! -d $d/node_modules/highlight.js ]; then
    (cd $d && echo '{"private":true}' > package.json && npm install --ignore-scripts --no-audit --no-fund highlight.js@$v >/dev/null 2>&1)
    find $d/node_modules -type f -name '*.js' -exec sed -i '/^\/\/# sourceMappingURL=/d' {} +
  fi
  echo "const h = require('highlight.js'); module.exports = () => h;" > $d/entry.cjs
  (cd $d && $BUN build entry.cjs --target node --format cjs --minify --outdir out >/dev/null)
  echo "built $v"
done
