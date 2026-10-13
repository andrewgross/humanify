// usage: node scripts/label.mjs [--jobs N] [--only substring] [--force]
// For every builds/<bundler>/<min|unmin>/<app>/<mode>/out.js(+.map):
//   writes labels.jsonl (one row per module span) and modules/<k>.js (the span's exact text) in that build dir.
// Labels come from the SOURCEMAP: each top-level statement (or, when a `var` mixes files, each declarator)
// is attributed to the source file owning the majority of its mapping segments; consecutive units of one
// file form a span. Kind comes from the bundler's wrapper helpers (found by their definition text).
// Re-runnable: skips builds that already have labels.jsonl unless --force.
import fs from "node:fs"; import path from "node:path"; import crypto from "node:crypto";
import { createRequire } from "node:module";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
const lab = process.env.LABROOT || "/work/exp095-library-fingerprints/scale/ix"; // corpus root: builds/<bundler>/<min>/<app>/<mode>/
const SRCROOT = process.env.SRCROOT || lab; // where node_modules/ resolves for corpora without pkgs/
const req = createRequire("/work/exp095-library-fingerprints/lab/tools/package.json");

if (isMainThread) {
  const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, arr) => (x.startsWith("--") ? [...a, [x.slice(2), arr[i + 1] && !arr[i + 1].startsWith("--") ? arr[i + 1] : true]] : a), []));
  const dirs = [];
  if (args.dirs) { for (const d of fs.readFileSync(args.dirs, "utf8").split("\n").filter(Boolean)) if (args.force || !fs.existsSync(d + "/labels.jsonl")) dirs.push(d); }
  else for (const b of fs.readdirSync(lab + "/builds")) for (const m of ["min", "unmin"]) {
    const bm = path.join(lab, "builds", b, m); if (!fs.existsSync(bm)) continue;
    for (const app of fs.readdirSync(bm)) for (const mode of fs.readdirSync(path.join(bm, app))) {
      const d = path.join(bm, app, mode);
      if (args.only && !d.includes(args.only)) continue;
      if (!fs.existsSync(d + "/out.js.map")) continue;
      if (!args.force && fs.existsSync(d + "/labels.jsonl")) continue;
      dirs.push(d);
    }
  }
  const N = Math.min(Number(args.jobs || 48), dirs.length || 1); let i = 0, done = 0;
  fs.mkdirSync(lab + "/logs", { recursive: true }); const errs = fs.createWriteStream(lab + "/logs/label-errors.jsonl", { flags: "a" });
  await Promise.all(Array.from({ length: N }, () => new Promise((res) => {
    const w = new Worker(new URL(import.meta.url), { workerData: {}, resourceLimits: { maxOldGenerationSizeMb: 8000 } });
    const next = () => { if (i >= dirs.length) { w.terminate(); return res(); } w.postMessage(dirs[i++]); };
    w.on("message", (m) => { if (m.err) errs.write(JSON.stringify(m) + "\n"); if (++done % 500 === 0) console.log(done, "/", dirs.length); next(); });
    w.on("error", (e) => { errs.write(JSON.stringify({ err: String(e) }) + "\n"); res(); });
    next();
  })));
  console.log("labelled", done, "builds");
} else {
  const acorn = req("acorn");
  const { decode } = req("@jridgewell/sourcemap-codec");
  const pkgCache = new Map();
  parentPort.on("message", (d) => {
    try { labelBuild(d); parentPort.postMessage({ ok: d }); }
    catch (e) { parentPort.postMessage({ err: String(e && e.stack || e).slice(0, 800), dir: d }); }
  });

  function pkgOf(abs) {
    // abs: absolute source path. -> {pkg, pkgVersion, file}
    const i = abs.lastIndexOf("/node_modules/");
    if (i < 0) {
      const m = abs.match(/\/pkgs\/[^/]+\/(.*)$/);
      return { pkg: "<app>", pkgVersion: null, file: m ? m[1] : abs };
    }
    const rest = abs.slice(i + 14).split("/");
    const n = rest[0].startsWith("@") ? 2 : 1;
    const name = rest.slice(0, n).join("/"), root = abs.slice(0, i + 14) + name;
    let v = pkgCache.get(root);
    if (v === undefined) { try { v = JSON.parse(fs.readFileSync(root + "/package.json", "utf8")).version ?? null; } catch { v = null; } pkgCache.set(root, v); }
    return { pkg: name, pkgVersion: v, file: rest.slice(n).join("/") };
  }
  function srcFormat(file, content) {
    if (/\.json$/.test(file)) return "json";
    if (content == null) return "unknown";
    if (/(^|[;\n}])\s*(import\s*[\w{*$"']|export\s+(\{|\*|default|const|let|var|function|class|async))/m.test(content)) return "esm";
    return "cjs";
  }

  function labelBuild(dir) {
    const rel = path.relative(lab, dir); const [, bundler, minS, app, mode] = rel.split("/");
    const code = fs.readFileSync(dir + "/out.js", "utf8");
    const map = JSON.parse(fs.readFileSync(dir + "/out.js.map", "utf8"));
    // sources -> absolute (Bun writes them relative to its temp outdir, so anchor on /pkgs/)
    const srcInfo = map.sources.map((s, k) => {
      const rest = s.replace(/^\0/, "").replace(/^(\.\.\/)+/, "").replace(/^\.\//, "");
      let abs;
      if (rest.startsWith("pkgs/")) abs = lab + "/" + rest;
      else if (rest.includes("node_modules/") || rest.startsWith("entries/")) abs = SRCROOT + "/" + rest;
      else return { pkg: "<bundler-helper>", pkgVersion: null, file: s.replace(/^\0/, ""), fmt: "helper" };
      const info = pkgOf(abs);
      return { ...info, fmt: srcFormat(info.file, map.sourcesContent?.[k]) };
    });
    // line starts + segment offsets (UTF-16 offsets, like JS string indices)
    const lineStart = [0]; for (let k = 0; k < code.length; k++) if (code.charCodeAt(k) === 10) lineStart.push(k + 1);
    const dec = decode(map.mappings);
    let nSeg = 0; for (const l of dec) nSeg += l.length;
    const segOff = new Int32Array(nSeg), segSrc = new Int32Array(nSeg); let q = 0;
    dec.forEach((l, li) => { for (const s of l) { segOff[q] = lineStart[li] + s[0]; segSrc[q] = s.length >= 4 ? s[1] : -1; q++; } });
    // segments are emitted in increasing generated order per line; lines are increasing -> sorted.
    const lowerBound = (x) => { let lo = 0, hi = nSeg; while (lo < hi) { const m = (lo + hi) >> 1; if (segOff[m] < x) lo = m + 1; else hi = m; } return lo; };
    const tally = (s, e) => { const c = new Map(); for (let k = lowerBound(s); k < nSeg && segOff[k] < e; k++) if (segSrc[k] >= 0) c.set(segSrc[k], (c.get(segSrc[k]) || 0) + 1); return c; };
    const majority = (c) => { let best = -1, bn = 0, tot = 0; for (const [k, n] of c) { tot += n; if (n > bn) { bn = n; best = k; } } return { src: best, agree: bn, total: tot }; };

    let ast; try { ast = acorn.parse(code, { ecmaVersion: "latest", sourceType: "script", allowReturnOutsideFunction: true, allowHashBang: true }); }
    catch { ast = acorn.parse(code, { ecmaVersion: "latest", sourceType: "module", allowHashBang: true }); }
    let body = ast.body;
    // Bun --target bun wraps everything in one function; descend if so (not used by this lab's node target).
    if (body.length === 1 && body[0].type === "ExpressionStatement" && body[0].expression.type === "CallExpression" && /Function/.test(body[0].expression.callee.type)) body = body[0].expression.callee.body.body;

    // wrapper helpers by definition text
    const helperKind = new Map();
    for (const st of body) {
      const defs = st.type === "VariableDeclaration" ? st.declarations.filter((d) => d.init && d.id.type === "Identifier").map((d) => [d.id.name, d.init]) : st.type === "FunctionDeclaration" ? [[st.id.name, st]] : [];
      for (const [name, node] of defs) {
        if (node.end - node.start > 400) continue;
        const t = code.slice(node.start, node.end);
        if (/exports:\s*\{\s*\}/.test(t) && /=>|function/.test(t)) helperKind.set(name, "cjs");
        else if (/\(\s*[\w$]+\s*=\s*0\s*\)/.test(t) && /=>|function/.test(t)) helperKind.set(name, "lazy-esm");
      }
    }
    const declNames = (st) => st.type === "VariableDeclaration" ? st.declarations.filter((d) => d.id.type === "Identifier").map((d) => d.id.name)
      : (st.type === "FunctionDeclaration" || st.type === "ClassDeclaration") && st.id ? [st.id.name] : [];
    const wrapperOf = (decl) => {
      const init = decl.init; if (!init || init.type !== "CallExpression" || init.callee.type !== "Identifier") return null;
      const k = helperKind.get(init.callee.name); if (!k || init.arguments.length !== 1) return null;
      return /Function|ObjectExpression/.test(init.arguments[0].type) ? k : null;
    };
    // units
    const units = [];
    for (const st of body) {
      if (st.type === "VariableDeclaration" && st.declarations.length > 1) {
        const parts = st.declarations.map((d, k) => ({ s: k === 0 ? st.start : d.start, e: k === st.declarations.length - 1 ? st.end : d.end, d }));
        const srcs = new Set(parts.map((p) => majority(tally(p.s, p.e)).src).filter((x) => x >= 0));
        if (srcs.size > 1) { for (const p of parts) units.push({ s: p.s, e: p.e, wrap: wrapperOf(p.d), declarator: true, names: p.d.id.type === "Identifier" ? [p.d.id.name] : [] }); continue; }
      }
      const wrap = st.type === "VariableDeclaration" ? st.declarations.map(wrapperOf).find(Boolean) ?? null : null;
      units.push({ s: st.start, e: st.end, wrap, declarator: false, names: declNames(st) });
    }
    for (const u of units) { const m = majority(tally(u.s, u.e)); u.src = m.src; u.agree = m.agree; u.total = m.total; }
    // unmapped units sandwiched between two units of the same source join them
    for (let k = 1; k < units.length - 1; k++) if (units[k].src < 0) {
      let a = k - 1, b = k + 1; while (b < units.length && units[b].src < 0) b++;
      if (b < units.length && units[a].src >= 0 && units[a].src === units[b].src) for (let x = k; x < b; x++) units[x].src = units[a].src;
    }
    // bundler glue with no mapping (Bun's `var N={};H(N,{name:()=>local,...})` namespace blocks): attach it to the
    // NEXT run only if most getter targets are declared there; otherwise it stays <unmapped>.
    const isNsGlue = (u) => { const t = code.slice(u.s, u.e).trim();
      return /^var\s+[\w$]+\s*=\s*\{\s*\};?$/.test(t) || /^[\w$]+\(\s*[\w$]+\s*,\s*\{[\s\S]*\}\s*\);?$/.test(t); };
    for (let b = 1; b < units.length; b++) if (units[b].src >= 0 && units[b - 1].src < 0) {
      let k = b; while (k > 0 && units[k - 1].src < 0 && isNsGlue(units[k - 1])) k--;
      if (k === b) continue;
      const targets = []; for (let x = k; x < b; x++) for (const m of code.slice(units[x].s, units[x].e).matchAll(/:\s*\(\)\s*=>\s*([\w$]+)/g)) targets.push(m[1]);
      const declared = new Set(); for (let x = b; x < units.length && units[x].src === units[b].src; x++) for (const n of units[x].names || []) declared.add(n);
      if (targets.length && targets.filter((t) => declared.has(t)).length * 2 > targets.length) for (let x = k; x < b; x++) { units[x].src = units[b].src; units[x].attached = true; }
    }
    let lastWrapEnd = -1; for (const u of units) if (u.wrap) lastWrapEnd = u.e;
    // runs
    const runs = [];
    for (const u of units) {
      const r = runs[runs.length - 1];
      // a new wrapper always starts a new module unless the run is that file's hoisted prefix of a lazy ESM
      if (r && r.src === u.src && !(u.wrap === "cjs") && !(r.units.some((x) => x.wrap))) r.units.push(u);
      else runs.push({ src: u.src, units: [u] });
    }
    // byte offsets
    const byteAt = (() => { const enc = Buffer.from(code, "utf8"); if (enc.length === code.length) return (x) => x;
      const cache = new Map(); return (x) => { if (!cache.has(x)) cache.set(x, Buffer.byteLength(code.slice(0, x), "utf8")); return cache.get(x); }; })();
    fs.rmSync(dir + "/modules", { recursive: true, force: true }); fs.mkdirSync(dir + "/modules");
    const partsBySrc = new Map(); for (const r of runs) partsBySrc.set(r.src, (partsBySrc.get(r.src) || 0) + 1);
    const partIdx = new Map();
    const bundlerVersion = bundler.split("-").slice(1).join("-");
    const rows = []; let prevEnd = 0;
    runs.forEach((r, k) => {
      const first = r.units[0], last = r.units[r.units.length - 1];
      // span starts right after the previous span (leading whitespace skipped) so /*! banners and comments travel with the module
      let s = prevEnd; while (s < first.s && /\s/.test(code[s])) s++;
      if (/^[;,]/.test(code.slice(s, first.s).trim())) s = first.s; // never start on a separator
      const e = last.e; prevEnd = e;
      const info = r.src >= 0 ? srcInfo[r.src] : { pkg: k === 0 || runs.slice(0, k).every((x) => x.src < 0) ? "<runtime>" : "<unmapped>", pkgVersion: null, file: null, fmt: null };
      let kind;
      const wrap = r.units.find((u) => u.wrap)?.wrap;
      if (bundler.startsWith("rollup")) kind = info.fmt === "cjs" ? "cjs" : info.fmt === "esm" || info.fmt === "json" ? "eager-esm" : "other";
      else if (info.pkg === "<runtime>" || info.pkg === "<unmapped>" || info.pkg === "<bundler-helper>") kind = "other";
      else if (wrap) kind = wrap;
      else kind = e <= lastWrapEnd ? "lazy-esm" : "eager-esm";
      const marker = kind === "lazy-esm" ? !!wrap : null;
      const agree = r.units.reduce((a, u) => a + u.agree, 0), total = r.units.reduce((a, u) => a + u.total, 0);
      const pi = (partIdx.get(r.src) || 0); partIdx.set(r.src, pi + 1);
      const text = code.slice(s, e);
      const tp = `modules/${String(k).padStart(5, "0")}.js`;
      fs.writeFileSync(path.join(dir, tp), text);
      rows.push({ build: rel.replace(/^builds\//, ""), bundler: bundler.split("-")[0], bundlerVersion, minified: minS === "min", app, mode, idx: k,
        pkg: info.pkg, pkgVersion: info.pkgVersion, file: info.file, srcFormat: info.fmt, kind, marker,
        part: pi, parts: r.src >= 0 ? partsBySrc.get(r.src) : 1, declaratorUnits: r.units.filter((u) => u.declarator).length, attachedGlue: r.units.filter((u) => u.attached).length,
        start: byteAt(s), end: byteAt(e), startChar: s, endChar: e, bytes: Buffer.byteLength(text), segAgree: agree, segTotal: total,
        purity: total ? +(agree / total).toFixed(4) : null, sha256: crypto.createHash("sha256").update(text).digest("hex").slice(0, 16),
        textPath: path.join(rel, tp) });
    });
    fs.writeFileSync(dir + "/labels.jsonl", rows.map((r) => JSON.stringify(r)).join("\n") + "\n");
  }
}
