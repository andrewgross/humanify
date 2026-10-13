#!/usr/bin/env bash
# usage: scripts/install.sh   (re-runnable; skips installed dirs)
# One npm project per package@version under pkgs/<slug>@<ver>/ (--ignore-scripts).
set -u
LAB=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$LAB/pkgs" "$LAB/logs"
python3 - "$LAB" <<'PY' > "$LAB/logs/install-jobs.txt"
import json,sys
lab=sys.argv[1]
cat={p['name']:p for p in json.load(open(lab+'/scripts/catalog.json'))['packages']}
for p in json.load(open(lab+'/versions.json')):
    for role,ver in p['versions'].items():
        slug=p['name'].replace('/','__')
        deps={p['name']:ver}; deps.update(cat[p['name']].get('extraDeps',{}))
        print(f"{slug}@{ver}\t{json.dumps(deps)}")
PY
inst() {
  local d="$1" deps="$2" dir="$LAB/pkgs/$1"
  [ -f "$dir/.installed" ] && return 0
  mkdir -p "$dir"
  printf '{"private":true,"name":"lab-app","dependencies":%s}\n' "$deps" > "$dir/package.json"
  if (cd "$dir" && npm install --ignore-scripts --no-audit --no-fund --loglevel=error) > "$LAB/logs/install-$d.log" 2>&1; then
    touch "$dir/.installed"; echo "ok $d"
  else echo "FAIL $d"; fi
}
export -f inst; export LAB
tr '\t' '\n' < "$LAB/logs/install-jobs.txt" | xargs -d '\n' -n 2 -P 16 bash -c 'inst "$0" "$1"'
