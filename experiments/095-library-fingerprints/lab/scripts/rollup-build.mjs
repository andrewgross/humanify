// usage: node scripts/rollup-build.mjs <entry> <outdir> <min:0|1>  -> outdir/out.js + out.js.map
// rollup 4 + node-resolve + commonjs + json, format cjs, dynamic imports inlined; min=1 adds terser (map chained).
import path from "node:path"; import { builtinModules } from "node:module";
import { createRequire } from "node:module";
const req = createRequire(path.resolve(path.dirname(new URL(import.meta.url).pathname), "../tools/package.json"));
const { rollup } = req("rollup");
const resolve = req("@rollup/plugin-node-resolve").default ?? req("@rollup/plugin-node-resolve");
const commonjs = req("@rollup/plugin-commonjs").default ?? req("@rollup/plugin-commonjs");
const json = req("@rollup/plugin-json").default ?? req("@rollup/plugin-json");
const terser = req("@rollup/plugin-terser").default ?? req("@rollup/plugin-terser");
const [entry, outdir, min, fmt = "cjs"] = process.argv.slice(2);
const ext = new Set([...builtinModules, ...builtinModules.map((m) => "node:" + m)]);
const b = await rollup({ input: entry, external: (id) => ext.has(id), onwarn: () => {},
  plugins: [resolve({ preferBuiltins: true, exportConditions: ["node"] }), commonjs({ ignoreDynamicRequires: true }), json()] });
await b.write({ file: path.join(outdir, "out.js"), format: fmt === "esm" ? "es" : "cjs", sourcemap: true, sourcemapExcludeSources: false,
  inlineDynamicImports: true, plugins: min === "1" ? [terser()] : [] });
