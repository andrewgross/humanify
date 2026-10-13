# esbuild-minified-0.28 — esbuild 0.28's guarded runtime, minified

A real esbuild 0.28.2 build of `../node-app` with
`--bundle --minify --format=iife --platform=node`.

esbuild 0.28 guards its runtime helpers with `try`: the CommonJS helper is
`var a=(n,e)=>()=>{try{return e||n((e={exports:{}}).exports,e),e.exports}catch(o){throw e=0,o}}`
and the lazy-init helper takes a third parameter that keeps the first error
(`(n,e,o)=>()=>{if(o)throw o[0];try{return n&&(e=n(n=0)),e}catch(l){throw o=[l],l}}`).
Bun never writes the guard, so the helper shape alone detects esbuild
(finding #96), and the guarded `__esm` is an init helper (`twins::fossil`).
The e2e holds it to: bundler and adapter esbuild, 26 files in `vendor/`, the
fresh grouping, deterministic legs, a split tree that boots with the
input's stdout.

## The build (PINNED: esbuild 0.28.2)

`bash test/e2e/fixtures/node-app/build.sh` (see its README).
sha256 (2026-10-12):

- v1.0.0: `cf310cd12b9a988775449c62933ca78568bc0c23cebe2cb5651cde307da82eae`
- v1.1.0: `89fe6e663eeb3896827ddf50ba711546dc5b416a0c22caab13afef844ddcd204`
