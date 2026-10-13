#!/usr/bin/env bash
# usage: scripts/install.sh <spec>...   installs each spec into pkgs/<name with / -> __>@<ver>/ with --ignore-scripts; strips sourcemap comments
set -u
N=$(cd "$(dirname "$0")/.." && pwd)
one() {
  spec=$1; name=${spec%@*}; ver=${spec##*@}; dir="$N/pkgs/${name//\//__}@$ver"
  [ -f "$dir/.done" ] && return 0
  mkdir -p "$dir"; echo '{"private":true}' > "$dir/package.json"
  if (cd "$dir" && npm install --no-audit --no-fund --ignore-scripts "$spec" > "$N/logs/install-${name//\//__}@$ver.log" 2>&1); then
    grep -rlZ --include='*.js' --include='*.mjs' --include='*.cjs' -E '^//[#@] sourceMappingURL=' "$dir/node_modules" 2>/dev/null | xargs -0 -r sed -i -E '/^\/\/[#@] sourceMappingURL=/d'
    touch "$dir/.done"; echo "ok $spec"
  else echo "FAIL $spec"; fi
}
export -f one; export N
printf '%s\n' "$@" | xargs -P 8 -I{} bash -c 'one "$@"' _ {}
