// usage: node webpack-build.cjs <tools dir> <entry> <outdir> <externals.json>  -> outdir/entry.js + entry.js.map
// webpack 5 production mode (its default minimizer: terser), target node, CommonJS output, ONE chunk (dynamic
// imports folded in), source map with every source written RELATIVE TO THE OUTPUT DIR (the corpus truth reads
// node_modules/<pkg> paths, as it does for Bun and esbuild maps). Webpack's own runtime modules have no file: their
// sources are spelled "\0webpack/<name>", which the truth reads as bundler-generated (no package).
const path = require("node:path");
const [tools, entry, outdir, extFile] = process.argv.slice(2);
const webpack = require(path.join(tools, "node_modules/webpack"));
const externals = new Set(JSON.parse(require("node:fs").readFileSync(extFile, "utf8")));
const config = {
  mode: "production",
  target: "node",
  context: path.dirname(entry),
  entry: entry,
  devtool: "source-map",
  output: {
    path: outdir,
    filename: "entry.js",
    library: { type: "commonjs2" },
    devtoolModuleFilenameTemplate: (info) =>
      path.isAbsolute(info.absoluteResourcePath)
        ? path.relative(outdir, info.absoluteResourcePath)
        : "\0webpack/" + info.identifier,
  },
  externalsPresets: { node: true },
  externals: [({ request }, cb) => (externals.has(request) ? cb(null, "commonjs " + request) : cb())],
  module: { rules: [{ test: /\.node$/, type: "asset/resource" }], parser: { javascript: { commonjsMagicComments: true } } },
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
    const e = stats.toJson({ all: false, errors: true }).errors.slice(0, 5);
    console.error(e.map((x) => x.message).join("\n").slice(0, 3000));
    process.exit(2);
  }
  process.exit(0);
});
