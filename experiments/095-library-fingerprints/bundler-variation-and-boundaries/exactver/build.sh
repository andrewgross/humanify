#!/usr/bin/env bash
# Exact-version index for the packages whose Claude Code version is known (banners / VERSION constants).
# usage: exactver/build.sh   (re-runnable; skips finished steps)
set -eu
E=$(cd "$(dirname "$0")" && pwd)
BUN=/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun
PKGS="@azure/msal-node@3.8.1 axios@1.15.2 zod@3.25.76 zod@3.25.76:zod/v4"
for spec in $PKGS; do
  imp=""; case "$spec" in *:*) imp=${spec#*:}; spec=${spec%%:*};; esac
  name=${spec%@*}; ver=${spec##*@}; dir="$E/pkgs/${name//\//__}@$ver"; app="$(basename "$dir")"
  if [ -n "$imp" ]; then app="$app+${imp//\//_}"; else imp=$name; fi
  if [ ! -d "$dir/node_modules" ]; then
    mkdir -p "$dir/entries"; echo '{"private":true}' > "$dir/package.json"
    (cd "$dir" && npm install --no-audit --no-fund --ignore-scripts "$spec" > "$E/logs/install-${name//\//__}.log" 2>&1)
  fi
  grep -rlZ --include='*.js' --include='*.mjs' --include='*.cjs' -E '^//[#@] sourceMappingURL=' "$dir/node_modules" 2>/dev/null | xargs -0 -r sed -i -E '/^\/\/[#@] sourceMappingURL=/d'
  mkdir -p "$dir/entries-$app"; ent="$dir/entries-$app"
  printf 'function labAppMain_dyn(x){ if (!x) throw new Error("lab-app dyn: no input given"); return Object.keys(x).length + ":dyn"; }\nexport const loaders = [() => import("%s")];\nexport { labAppMain_dyn };\n' "$imp" > "$ent/dyn.mjs"
  printf 'function labAppMain_imp(x){ if (!x) throw new Error("lab-app imp: no input given"); return Object.keys(x).length + ":imp"; }\nimport * as m0 from "%s";\nexport { labAppMain_imp, m0 };\n' "$imp" > "$ent/imp.mjs"
  printf 'function labAppMain_req(x){ if (!x) throw new Error("lab-app req: no input given"); return Object.keys(x).length + ":req"; }\nconst m0 = require("%s");\nmodule.exports = { run: labAppMain_req, m0 };\n' "$imp" > "$ent/req.cjs"
  for mode in dyn imp req; do
    entry=$(ls "$ent/$mode".*); out="$E/builds/bun-1.4.0/min/$app/$mode"
    [ -f "$out/out.js.map" ] && continue
    mkdir -p "$out"
    (cd "$ent" && "$BUN" build "$entry" --target node --format cjs --minify --sourcemap=external --outdir "$out/.tmp" > /dev/null)
    base=$(basename "$entry"); base=${base%.*}
    mv "$out/.tmp/$base.js" "$out/out.js"; mv "$out/.tmp/$base.js.map" "$out/out.js.map"; rm -rf "$out/.tmp"
  done
done
node "$E/scripts/label.mjs" --jobs 16
for d in "$E"/builds/*/min/*/*; do echo "$d $(wc -l < "$d/labels.jsonl") spans"; done
