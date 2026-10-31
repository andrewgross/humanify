#!/usr/bin/env node
// exp172 phase 0: per extracted CommonJS factory, the candidate signals for
// "this is the app's own module" next to the bun-dev source-map truth.
//
// Reads, per bundle: the raw bundle (parsed with @babel/parser for the
// factory reference graph), exp166's names-mode stub run diag
// (eraRestoration.factoryFiles = every vendored factory's raw span + the
// file it was written to; libraryDiscovery.claims = the index's statement
// claims) and, when given, the frozen truth (segments/sources, byte offsets).
// Writes one JSON row per factory. Code text never leaves this process:
// rows carry names, spans, byte counts and package labels only.
//
// usage: node census.mjs <corpus.json> <runs-dir> <truth-dir|-> > rows.jsonl
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { createRequire } from "node:module";

// the repo's own node_modules (two levels up from this script)
const require = createRequire(new URL("../../package.json", import.meta.url));
const parser = require("@babel/parser");
const traverse = require("@babel/traverse").default;

const [corpusPath, runsDir, truthDir] = process.argv.slice(2);
const entries = JSON.parse(fs.readFileSync(corpusPath, "utf8")).entries;
const only = process.env.ONLY;

function byteOffsets(text) {
  // char offset -> byte offset (UTF-8), only built when the text is not ASCII
  const buf = Buffer.from(text, "utf8");
  if (buf.length === text.length) return null;
  const map = new Uint32Array(text.length + 1);
  let b = 0;
  for (let i = 0; i < text.length; i++) {
    map[i] = b;
    const c = text.charCodeAt(i);
    if (c < 0x80) b += 1;
    else if (c < 0x800) b += 2;
    else if (c >= 0xd800 && c <= 0xdbff) {
      b += 4;
      map[++i] = b - 4 + 2; // low surrogate: never a node boundary
    } else b += 3;
  }
  map[text.length] = b;
  return map;
}

function truthBytes(truth, start, end) {
  const { segments, sources } = truth;
  let lo = 0;
  let hi = segments.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (segments[mid][0] <= start) lo = mid;
    else hi = mid - 1;
  }
  const per = {};
  for (let k = lo; k < segments.length && segments[k][0] < end; k++) {
    const a = Math.max(segments[k][0], start);
    const b = Math.min(k + 1 < segments.length ? segments[k + 1][0] : end, end);
    if (b <= a) continue;
    const label = sources[segments[k][1]][0];
    per[label] = (per[label] || 0) + (b - a);
  }
  return per;
}

function main() {
  for (const e of entries) {
    if (only && !e.id.includes(only)) continue;
    const diagPath = path.join(runsDir, e.id, "diag.json");
    if (!fs.existsSync(diagPath)) continue;
    const diag = JSON.parse(fs.readFileSync(diagPath, "utf8"));
    const factoryFiles = diag.eraRestoration?.factoryFiles || [];
    const claims = diag.libraryDiscovery?.claims || [];
    const text = fs.readFileSync(e.files.bundle.origin, "utf8");
    const map = byteOffsets(text);
    const B = (i) => (map ? map[i] : i);
    let truth = null;
    if (truthDir && truthDir !== "-") {
      const tp = path.join(truthDir, `${e.id}.truth.json.gz`);
      if (fs.existsSync(tp))
        truth = JSON.parse(zlib.gunzipSync(fs.readFileSync(tp)).toString());
    }
    let ast;
    try {
      ast = parser.parse(text, {
        sourceType: "script",
        errorRecovery: true,
        allowReturnOutsideFunction: true,
      });
    } catch (err) {
      process.stderr.write(`${e.id}: parse failed ${err.message}\n`);
      continue;
    }
    // Every declarator whose raw byte span starts a factoryFiles span.
    const byStart = new Map(factoryFiles.map(([[s, end], p], i) => [s, { s, end, p, i }]));
    const factories = []; // {name, binding, s, end, p, order}
    const declStarts = new Map();
    traverse(ast, {
      VariableDeclarator(p) {
        const init = p.node.init;
        if (!init || init.type !== "CallExpression") return;
        if (p.node.id.type !== "Identifier") return;
        const fn = init.arguments[0];
        if (!fn) return;
        const ok =
          fn.type === "ArrowFunctionExpression" ||
          fn.type === "FunctionExpression" ||
          fn.type === "ObjectExpression";
        if (!ok) return;
        declStarts.set(B(p.node.start), p);
        declStarts.set(B(p.parentPath.node.start), p);
      },
    });
    for (const [s, f] of byStart) {
      const p = declStarts.get(s) ?? declStarts.get(s + 4);
      if (!p) {
        // the span may start at `var `; find a declarator inside the span
        continue;
      }
      const fn = p.node.init.arguments[0];
      factories.push({
        name: p.node.id.name,
        binding: p.scope.getBinding(p.node.id.name),
        s: f.s,
        end: f.end,
        bodyS: B(fn.start),
        bodyE: B(fn.end),
        p: f.p,
        order: f.i,
      });
    }
    factories.sort((a, b) => a.s - b.s);
    const starts = factories.map((f) => f.bodyS);
    const owner = (off) => {
      let lo = 0;
      let hi = factories.length - 1;
      let ans = -1;
      while (lo <= hi) {
        const mid = (lo + hi) >> 1;
        if (starts[mid] <= off) {
          ans = mid;
          lo = mid + 1;
        } else hi = mid - 1;
      }
      return ans >= 0 && off < factories[ans].bodyE ? ans : -1;
    };
    const out = factories.map(() => new Set());
    const inn = factories.map(() => new Set());
    const fromRuntime = factories.map(() => 0);
    factories.forEach((f, j) => {
      for (const r of f.binding?.referencePaths || []) {
        const o = owner(B(r.node.start));
        if (o === -1) fromRuntime[j] += 1;
        else if (o !== j) {
          out[o].add(j);
          inn[j].add(o);
        }
      }
    });
    // claims by factory (bytes per package; seeds only = the index's own answers)
    const claimed = factories.map(() => ({}));
    for (const c of claims) {
      if (c.rule !== "seed" && c.rule !== "module") continue;
      const o = owner(c.start);
      if (o === -1) continue;
      claimed[o][c.package] = (claimed[o][c.package] || 0) + (c.end - c.start);
    }
    // runtime bytes outside every factory declaration
    const buf = Buffer.from(text, "utf8");
    const total = buf.length;
    const inFactories = factories.reduce((n, f) => n + (f.end - f.s), 0);
    factories.forEach((f, j) => {
      const bodyText = map
        ? buf.subarray(f.bodyS, f.bodyE).toString()
        : text.slice(f.bodyS, f.bodyE);
      const row = {
        bundle: e.id,
        appPackage: e.appPackage,
        factory: f.name,
        order: f.order,
        start: f.s,
        end: f.end,
        bytes: f.end - f.s,
        vendorFile: f.p,
        out: [...out[j]].map((k) => factories[k].name),
        in: [...inn[j]].map((k) => factories[k].name),
        fromRuntime: fromRuntime[j],
        claims: claimed[j],
        banner: /\/\*!|@license|@preserve/.test(bodyText),
        // an inlined package.json: `module.exports = {name: "…", version: "…", …}`
        manifestOf:
          bodyText.length < 20000
            ? (/^[^{]*\{\s*[\w$]+\.exports\s*=\s*\{\s*"?name"?\s*:\s*"([^"]+)"\s*,\s*"?version"?\s*:/.exec(
                bodyText,
              ) || [])[1] || null
            : null,
        requires: [...bodyText.matchAll(/require\("([^"]+)"\)/g)].map((m) => m[1]).slice(0, 20),
        bundleBytes: total,
        runtimeBytes: total - inFactories,
      };
      if (truth) row.truth = truthBytes(truth, f.s, f.end);
      process.stdout.write(`${JSON.stringify(row)}\n`);
    });
  }
}

main();
