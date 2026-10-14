# Bundler fixtures (exp105)

Real minified builds of one tiny app (`ms@2.1.3`, `escape-html@1.0.3`, `p-limit@6.2.0` and ten lines of app
code), made with the index builder's own recipes (`crates/humanify-index/recipes/`):

- `webpack5-terser.js.fixture` — webpack 5.111.1 production (terser), target node, one chunk: a module table
  (`ms`), a `.cw()` CommonJS wrapper (`escape-html`) and a concatenated ES module entry (`p-limit`, the app).
- `rollup4-terser.js.fixture` — rollup 4.64.5 + node-resolve + commonjs + json + terser, format cjs: no runtime;
  the commonjs plugin's guarded require functions (`if(t)return r;t=1;`) and its `getDefaultExportFromCjs`.

The `.fixture` extension keeps the formatters (lint-staged prettier/biome) off the bytes: the detector and unit
tests read them exactly as the bundlers wrote them.
