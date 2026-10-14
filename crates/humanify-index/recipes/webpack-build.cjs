// humanify-index's webpack recipe (crates/humanify-index/src/recipe.rs runs it as `node webpack-build.cjs ...`):
//   node webpack-build.cjs --version                                  -> webpack's version
//   node webpack-build.cjs <entry> <outdir> <cjs|esm> [external...]   -> <outdir>/out.js + out.js.map
// webpack 5 production mode (its default minimizer: terser), target node, ONE chunk (dynamic imports folded in),
// a source map whose sources are paths RELATIVE TO THE OUTPUT DIR (the builder's attribution reads
// node_modules/<pkg> paths, as it does for Bun's and esbuild's maps); webpack's runtime modules have no file and
// are spelled "\0webpack/<name>". The same settings as exp100's test-split recipe (webpack-build.cjs there).
// `webpack` resolves from this script's own directory: install it beside a copy of this file.
const path = require("node:path");
const args = process.argv.slice(2);
const webpack = require("webpack");
if (args[0] === "--version") {
  console.log(require("webpack/package.json").version);
  process.exit(0);
}
const [entryArg, outdir, format, ...externalList] = args;
const entry = path.resolve(entryArg);
const externals = new Set(externalList);
const esm = format === "esm";
const config = {
  mode: "production",
  target: "node",
  context: process.cwd(),
  entry,
  devtool: "source-map",
  output: {
    path: path.resolve(outdir),
    filename: "out.js",
    library: { type: esm ? "module" : "commonjs2" },
    module: esm,
    chunkFormat: esm ? "module" : "commonjs",
    devtoolModuleFilenameTemplate: (info) =>
      path.isAbsolute(info.absoluteResourcePath)
        ? path.relative(path.resolve(outdir), info.absoluteResourcePath)
        : "\0webpack/" + info.identifier,
  },
  experiments: esm ? { outputModule: true } : {},
  externalsPresets: { node: true },
  externals: [({ request }, cb) => (externals.has(request) ? cb(null, (esm ? "module " : "commonjs ") + request) : cb())],
  module: {
    rules: [{ test: /\.node$/, type: "asset/resource" }],
    parser: { javascript: { commonjsMagicComments: true } },
  },
  node: { __dirname: false, __filename: false, global: false },
  optimization: { splitChunks: false, runtimeChunk: false },
  plugins: [
    new webpack.optimize.LimitChunkCountPlugin({ maxChunks: 1 }),
    new webpack.DefinePlugin({ "process.env.NODE_ENV": JSON.stringify("production") }),
  ],
  performance: { hints: false },
  infrastructureLogging: { level: "error" },
};
webpack(config, (err, stats) => {
  if (err) {
    console.error(err.stack || String(err));
    process.exit(1);
  }
  if (stats.hasErrors()) {
    const errors = stats.toJson({ all: false, errors: true }).errors.slice(0, 20);
    // the builder retries what it cannot resolve as externals: say it in esbuild's words
    for (const e of errors) {
      const m = /Can't resolve '([^']+)'/.exec(e.message);
      if (m) console.error(`Could not resolve "${m[1]}"`);
      if (/top-level-await|top level await/i.test(e.message)) console.error("top-level await");
    }
    console.error(errors.map((x) => x.message).join("\n").slice(0, 3000));
    process.exit(2);
  }
  process.exit(0);
});
