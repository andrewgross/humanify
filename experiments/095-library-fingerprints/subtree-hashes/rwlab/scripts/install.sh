#!/bin/bash
# scripts/install.sh : one npm project per package@version under pkgs/ (re-runnable), then strip input sourcemaps.
cd /work/exp095-library-fingerprints/subtree-hashes/rwlab
python3 -c "
import json
for p in json.load(open('versions.json')):
    for v in p['versions']: print(p['name'].replace('/','__')+'@'+v, json.dumps({p['name']:v}))
" > logs/install-jobs.txt
inst() {
  local d="$1" deps="$2" dir=pkgs/$1
  [ -f "$dir/.installed" ] && return 0
  mkdir -p "$dir"; printf '{"private":true,"name":"rw-app","dependencies":%s}\n' "$deps" > "$dir/package.json"
  if (cd "$dir" && npm install --ignore-scripts --no-audit --no-fund --loglevel=error) > logs/install-$d.log 2>&1; then touch "$dir/.installed"; echo "ok $d"; else echo "FAIL $d"; fi
}
export -f inst
while IFS=' ' read -r d deps; do printf '%s\0%s\0' "$d" "$deps"; done < logs/install-jobs.txt | xargs -0 -n 2 -P 24 bash -c 'inst "$0" "$1"' > logs/install.out 2>&1
echo "installed: $(grep -c ^ok logs/install.out) failed: $(grep -c ^FAIL logs/install.out)"
# esbuild/bun follow a library's own //# sourceMappingURL: strip them (as the lab does)
find pkgs -type f -path '*/node_modules/*' \( -name '*.js' -o -name '*.mjs' -o -name '*.cjs' \) -print0 | xargs -0 -P 32 sed -i '/^\/\/# sourceMappingURL=/d'
