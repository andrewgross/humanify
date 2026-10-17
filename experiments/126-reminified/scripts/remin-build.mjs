// exp126: build one app the RE-MINIFIED way (corpus `remin`).
//
// usage: node remin-build.mjs <install dir> <out root> <slug> <closure.json>
//   -> <out root>/stage1/<slug>/entry.js(.map)    esbuild 0.28.2 bundle, NOT minified, preferring each package's
//                                                  published minified entry (below); the package's own source
//                                                  map is chained when it ships one
//      <out root>/terser/<slug>/entry.js(.map)    stage 1 re-minified by terser 5 (compress + mangle), maps composed
//      <out root>/esbuild/<slug>/entry.js(.map)   stage 1 re-minified by esbuild 0.28.2 --minify, maps composed
//      <out root>/stage1/<slug>/swaps.json        every package entry the build swapped for a minified one
//
// The minified-entry rule (a bare specifier only: a package's entry or a deep import of it). After the normal
// resolution, the package's published minified file is used instead when one exists:
//   1. the package entry: the first of package.json `unpkg`, `jsdelivr`, `browser` (a string) naming a file
//      that exists and is minified;
//   2. otherwise the resolved file's `.min` sibling (`x.js` -> `x.min.js`, also .cjs/.mjs) when it is minified.
// "Minified": the file name holds `.min.`, or the file is >= 2 KB with a mean line length >= 300.
// All three outputs land at the same directory depth, so the stage-1 map's relative sources resolve the same.
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire("/work/exp095-library-fingerprints/lab/tools/package.json");
const esbuild = require("esbuild");
const terser = require("terser");

const [installDir, outRoot, slug, closurePath] = process.argv.slice(2);
const cl = JSON.parse(fs.readFileSync(closurePath, "utf8"));
const dirs = Object.fromEntries(["stage1", "terser", "esbuild"].map((s) => [s, path.join(outRoot, s, slug)]));
for (const d of Object.values(dirs)) fs.mkdirSync(d, { recursive: true });

function isMinified(file) {
  let st;
  try {
    st = fs.statSync(file);
  } catch {
    return false;
  }
  if (!st.isFile()) return false;
  if (/\.min\.[cm]?js$/.test(file)) return true;
  if (st.size < 2048) return false;
  const text = fs.readFileSync(file, "utf8");
  const lines = text.split("\n").length;
  return text.length / lines >= 300;
}

function packageRoot(file) {
  let d = path.dirname(file);
  while (d.includes("node_modules")) {
    const pj = path.join(d, "package.json");
    if (fs.existsSync(pj)) {
      try {
        const j = JSON.parse(fs.readFileSync(pj, "utf8"));
        if (j.name && d.endsWith(path.join("node_modules", j.name))) return [d, j];
      } catch {}
    }
    d = path.dirname(d);
  }
  return null;
}

function fieldFile(root, v) {
  if (typeof v !== "string") return null;
  for (const c of [v, v + ".js"]) {
    const f = path.resolve(root, c);
    if (f.startsWith(root + path.sep) && isMinified(f)) return f;
  }
  return null;
}

function minAlternative(spec, resolved) {
  if (isMinified(resolved)) return null; // already the minified file
  const pr = packageRoot(resolved);
  if (!pr) return null;
  const [root, pj] = pr;
  if (spec === pj.name) {
    for (const k of ["unpkg", "jsdelivr", "browser"]) {
      const f = fieldFile(root, pj[k]);
      if (f) return [f, k];
    }
  }
  const m = resolved.match(/^(.*)\.([cm]?js)$/);
  if (m) {
    const sib = `${m[1]}.min.${m[2]}`;
    if (isMinified(sib)) return [sib, "sibling"];
  }
  return null;
}

const swaps = [];
const preferMin = {
  name: "prefer-min",
  setup(build) {
    build.onResolve({ filter: /^[^./]/ }, async (args) => {
      if (args.pluginData?.preferMin) return undefined;
      if (args.kind !== "import-statement" && args.kind !== "require-call" && args.kind !== "dynamic-import")
        return undefined;
      const r = await build.resolve(args.path, {
        kind: args.kind,
        importer: args.importer,
        resolveDir: args.resolveDir,
        pluginData: { preferMin: true },
      });
      if (r.errors.length || r.external || !r.path || !path.isAbsolute(r.path)) return undefined;
      const alt = minAlternative(args.path, r.path);
      if (!alt) return undefined;
      swaps.push({
        spec: args.path,
        from: path.relative(installDir, r.path),
        to: path.relative(installDir, alt[0]),
        via: alt[1],
        bytes: fs.statSync(alt[0]).size,
      });
      return { path: alt[0], sideEffects: r.sideEffects };
    });
  },
};

const t0 = Date.now();
const stage1 = path.join(dirs.stage1, "entry.js");
await esbuild.build({
  entryPoints: [path.join(installDir, "entry.cjs")],
  absWorkingDir: installDir,
  bundle: true,
  minify: false,
  platform: "node",
  format: cl.esm ? "esm" : "cjs",
  sourcemap: true,
  outfile: stage1,
  logLevel: "error",
  external: cl.externals,
  loader: Object.fromEntries([".node", ...cl.loaders].map((x) => [x, "empty"])),
  define: { "process.env.NODE_ENV": '"production"' },
  plugins: [preferMin],
});
// dedupe (esbuild may resolve one specifier from several importers)
const seen = new Set();
const uniq = swaps.filter((s) => {
  const k = s.to;
  if (seen.has(k)) return false;
  seen.add(k);
  return true;
});
fs.writeFileSync(path.join(dirs.stage1, "swaps.json"), JSON.stringify(uniq, null, 1));
const t1 = Date.now();

// stage 2a: terser (webpack's TerserPlugin defaults: compress + mangle), map composed onto stage 1's
const code = fs.readFileSync(stage1, "utf8").replace(/\n\/\/# sourceMappingURL=entry\.js\.map\s*$/, "\n");
const map1 = fs.readFileSync(stage1 + ".map", "utf8");
const tr = await terser.minify(
  { "entry.js": code },
  {
    module: !!cl.esm,
    compress: { passes: 1 },
    mangle: true,
    sourceMap: { content: map1, url: "entry.js.map", includeSources: true },
  },
);
fs.writeFileSync(path.join(dirs.terser, "entry.js"), tr.code);
fs.writeFileSync(path.join(dirs.terser, "entry.js.map"), typeof tr.map === "string" ? tr.map : JSON.stringify(tr.map));
const t2 = Date.now();

// stage 2b: esbuild --minify on the stage-1 file (esbuild reads the linked input map and composes)
await esbuild.build({
  entryPoints: [stage1],
  bundle: false,
  minify: true,
  platform: "node",
  format: cl.esm ? "esm" : "cjs",
  sourcemap: true,
  outfile: path.join(dirs.esbuild, "entry.js"),
  logLevel: "error",
});
const t3 = Date.now();
console.log(
  JSON.stringify({
    ok: true,
    swaps: uniq.length,
    swapBytes: uniq.reduce((a, s) => a + s.bytes, 0),
    ms: { stage1: t1 - t0, terser: t2 - t1, esbuild: t3 - t2 },
  }),
);
