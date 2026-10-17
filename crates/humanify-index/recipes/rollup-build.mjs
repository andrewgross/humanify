// humanify-index's rollup recipe (crates/humanify-index/src/recipe.rs runs it as `node rollup-build.mjs ...`):
//   node rollup-build.mjs --version                                  -> rollup's version
//   node rollup-build.mjs <entry> <outdir> <cjs|esm> [external...]   -> <outdir>/out.js + out.js.map
// rollup 4 + node-resolve + commonjs + json + terser, dynamic imports inlined, node builtins external, a chained
// source map without sourcesContent (exp131: the builder never reads it; otherwise the same settings as exp095's lab
// rollup-build.mjs, min=1, which built exp100's test split).
// The plugins resolve from this script's own directory: install them beside a copy of this file.
import path from "node:path";
import { builtinModules, createRequire } from "node:module";
const req = createRequire(import.meta.url);
const args = process.argv.slice(2);
const { rollup, VERSION } = req("rollup");
if (args[0] === "--version") {
  console.log(VERSION);
  process.exit(0);
}
const pick = (m) => m.default ?? m;
const resolve = pick(req("@rollup/plugin-node-resolve"));
const commonjs = pick(req("@rollup/plugin-commonjs"));
const json = pick(req("@rollup/plugin-json"));
const terser = pick(req("@rollup/plugin-terser"));
const [entry, outdir, fmt, ...externalList] = args;
const ext = new Set([...builtinModules, ...builtinModules.map((m) => "node:" + m), ...externalList]);
try {
  const b = await rollup({
    input: path.resolve(entry),
    external: (id) => ext.has(id),
    onwarn: () => {},
    plugins: [resolve({ preferBuiltins: true, exportConditions: ["node"] }), commonjs({ ignoreDynamicRequires: true }), json()],
  });
  await b.write({
    file: path.join(path.resolve(outdir), "out.js"),
    format: fmt === "esm" ? "es" : "cjs",
    sourcemap: true,
    sourcemapExcludeSources: true,
    inlineDynamicImports: true,
    plugins: [terser()],
  });
} catch (e) {
  // the builder retries top-level await as ESM: say it in its words
  if (e && e.code === "INVALID_TLA_FORMAT") console.error("top-level await");
  console.error(String((e && e.stack) || e).slice(0, 3000));
  process.exit(2);
}
