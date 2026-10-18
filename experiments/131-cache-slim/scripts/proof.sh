#!/usr/bin/env bash
# exp131 proof on the 1,000-version sample (run under: flock /work/index-build.lock):
#   A = main's builder on the UNSTRIPPED copies; strip the copies; B = this branch's builder on the stripped copies;
#   C = main's builder on the stripped copies. Every leg re-fingerprints every shard (empty store) from the cached builds.
set -euo pipefail
D=/work/exp131-data; S=$D/sample
RECIPES=(--sponge-regions origin
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify
  --recipe rollup:target=node,format=cjs,minify
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs)
leg() { python3 -I $S/build_chunks.py $D/$1 sample 48-63 4 $D/bin/$2 $S/packages.txt $S/pins.txt 250 -- "${RECIPES[@]}"; }
sums() { (cd $D/work && find . -path '*/build/*' -type f ! -name '*.map' -print0 | sort -z | xargs -0 sha256sum) > $1; }
mapbytes() { find $D/work -path '*/build/*' -name '*.map' -printf '%s\n' | awk '{s+=$1} END {print s}'; }
date -u +"start %FT%TZ"
sums $S/nonmap-0.sha; echo "map bytes before $(mapbytes)"
leg legA humanify-index-main
sums $S/nonmap-1.sha
taskset -c 48-63 $D/bin/humanify-index-slim slim-cache --work $D/work --journal $D/journal-sample > $S/slim-sample.json 2> $S/slim-sample.log
cat $S/slim-sample.json
sums $S/nonmap-2.sha; echo "map bytes after $(mapbytes)"
leg legB humanify-index-slim
leg legC humanify-index-main
sums $S/nonmap-3.sha
date -u +"end %FT%TZ"
echo PROOF DONE
