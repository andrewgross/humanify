# exp109: the c2-all build's shared flags (sourced)
D=/work/exp109-data
IX=$D/bin/humanify-index-5fe24646
COMMON=(--as-of 2026-10-11 --registry /work/exp101-data/registry --store $D/store --work $D/work --policy year-all+minor --offline
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify
  --recipe rollup:target=node,format=cjs,minify
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs)
LIST=/work/exp101-data/roots-base.txt; PINS=/work/exp101-data/pins-c2-l12m4.txt
