#!/usr/bin/env bash
# usage: scripts/strip-input-maps.sh
# Removes `//# sourceMappingURL=...` comment lines from every .js/.cjs/.mjs under pkgs/*/node_modules.
# Why: esbuild silently CHAINS a library's own sourcemap, so its labels would name the library's TS sources
# (src/foo.ts) instead of the published JS file the other bundlers name. Code is untouched (comment lines only).
set -eu
LAB=$(cd "$(dirname "$0")/.." && pwd)
grep -rlZ --include='*.js' --include='*.mjs' --include='*.cjs' -E '^//[#@] sourceMappingURL=' "$LAB"/pkgs/*/node_modules 2>/dev/null \
  | xargs -0 -r -P 32 -n 200 sed -i -E '/^\/\/[#@] sourceMappingURL=/d'
echo "remaining: $(grep -rl --include='*.js' --include='*.mjs' --include='*.cjs' -E '^//[#@] sourceMappingURL=' "$LAB"/pkgs/*/node_modules 2>/dev/null | wc -l)"
