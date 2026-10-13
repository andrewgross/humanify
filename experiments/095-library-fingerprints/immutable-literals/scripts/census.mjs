// Part A census: SELF-IDENTIFYING declared data (a module saying who it is), per detector.
// usage: node scripts/census.mjs [--jobs 60]
//   input 1: every JS/JSON source file of the lab packages + pkgs-cc (owner package known -> precision measurable)
//   input 2: Claude Code raw bundles 2.1.198 / 2.1.216, per layout unit (CJS factory / lazy-ESM group)
// writes results/census-hits-sources.jsonl, results/census-hits-cc.jsonl (one row per hit: detector, value, where)
import fs from "node:fs"; import path from "node:path";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { createRequire } from "node:module";
const acorn = createRequire("/work/exp095-library-fingerprints/lab/tools/package.json")("acorn");
const LAB = "/work/exp095-library-fingerprints/lab/";
const HERE = "/work/exp095-library-fingerprints/immutable-literals/";
const CCIN = (v) => `/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-${v}/binary-decompiled/src/entrypoints/index.js`;
const OPTS = { ecmaVersion: "latest", allowHashBang: true, allowReturnOutsideFunction: true, allowAwaitOutsideFunction: true, allowImportExportEverywhere: true, allowReserved: true };

const SEMVER = /^v?\d+\.\d+\.\d+(?:[-+][\w.-]+)?$/;
const NPMNAME = /^(?:@[a-z0-9][\w.-]*\/)?[a-z0-9][\w.-]*$/;
const HLJS_KEYS = new Set(["aliases", "keywords", "contains", "case_insensitive", "disableAutodetect", "illegal", "unicodeRegex"]);
const REG_KEYS = ["name", "type", "id", "method", "code", "kind", "keyword", "displayName", "title", "serviceId", "sdkId", "language"];

function str(n) {
  if (!n) return null;
  if (n.type === "Literal" && typeof n.value === "string") return n.value;
  if (n.type === "TemplateLiteral" && n.expressions.length === 0) return n.quasis[0].value.cooked;
  return null;
}
const keyName = (p) => (p.type === "Property" && !p.computed ? (p.key.type === "Identifier" ? p.key.name : typeof p.key.value === "string" ? p.key.value : null) : null);

export function census(code, emit) {
  const comments = [];
  let ast;
  for (const st of ["module", "script"]) { try { ast = acorn.parse(code, { ...OPTS, sourceType: st, onComment: (block, text) => { if (block) comments.push(text); } }); break; } catch { comments.length = 0; } }
  for (const c of comments) {
    if (c.startsWith("!")) emit("banner!", c.slice(1, 200).trim());
    else if (/@license|@preserve/.test(c)) emit("banner@license", c.slice(0, 200).replace(/\s+/g, " ").trim());
  }
  if (!ast) return;
  const stack = [ast];
  while (stack.length) {
    const n = stack.pop();
    if (!n || typeof n.type !== "string") continue;
    switch (n.type) {
      case "ObjectExpression": {
        const keys = new Map();
        for (const p of n.properties) { const k = keyName(p); if (k != null) keys.set(k, p.value); }
        const nm = str(keys.get("name")), ver = str(keys.get("version"));
        if (nm && ver && NPMNAME.test(nm) && SEMVER.test(ver)) emit(keys.has("dependencies") || keys.has("main") || keys.has("license") || keys.has("repository") ? "package.json" : "name+version", `${nm}@${ver}`);
        if (nm && [...keys.keys()].some((k) => HLJS_KEYS.has(k))) emit("hljs-grammar-shape", nm);
        if (nm && (keys.has("keywords") || keys.has("contains"))) emit("hljs-grammar-strict", nm); // aliases alone also matches app command tables
        if (keys.has("version") && ver && SEMVER.test(ver) && !nm) emit("version-field", ver);
        for (const rk of REG_KEYS) {
          const v = str(keys.get(rk));
          if (v != null && keys.size >= 3) emit("registry:" + rk, JSON.stringify([...keys.keys()].filter((k) => k !== rk).sort().slice(0, 6)) + " " + v.slice(0, 60));
        }
        if (str(keys.get("serviceId"))) emit("aws-serviceId", str(keys.get("serviceId")));
        break;
      }
      case "AssignmentExpression": case "VariableDeclarator": case "PropertyDefinition": {
        const lhs = n.type === "AssignmentExpression" ? n.left : n.type === "VariableDeclarator" ? n.id : n.key;
        const rhs = n.type === "AssignmentExpression" ? n.right : n.type === "VariableDeclarator" ? n.init : n.value;
        const ln = lhs && (lhs.type === "Identifier" ? lhs.name : lhs.type === "MemberExpression" && !lhs.computed ? lhs.property.name : null);
        const v = str(rhs);
        if (ln && v != null) {
          if (/^(VERSION|version|PACKAGE_VERSION|SDK_VERSION|LIB_VERSION|libVersion|sdkVersion)$/.test(ln) && SEMVER.test(v)) emit("VERSION=", v);
          if (ln === "name" && /^[A-Z]\w*(Error|Exception)$/.test(v)) emit("error-class-name", v);
          if (ln === "displayName") emit("displayName", v);
          if (/^(PACKAGE_NAME|packageName|LIB_NAME|libName|SDK_NAME|sdkName|pkgName)$/.test(ln)) emit("PACKAGE_NAME=", v);
        }
        break;
      }
      case "CallExpression": {
        const c = n.callee;
        const a0 = str(n.arguments[0]);
        if (c.type === "MemberExpression" && !c.computed && c.object.type === "Identifier" && c.object.name === "Symbol" && c.property.name === "for" && a0) emit("Symbol.for", a0);
        else if (c.type === "Identifier" && c.name === "Symbol" && a0) emit("Symbol()", a0);
        if (a0 && n.arguments.length === 1 && /^[a-z][\w.-]*(:[\w*.-]+)+$/.test(a0) && !/^(https?|file|node|data|mailto|urn):/.test(a0)) emit("debug-namespace?", a0);
        // smithy commands: .n("XxxClient","YyyCommand") / .s("Service","Op",{})
        const a1 = str(n.arguments[1]);
        if (a0 && a1 && /^[A-Z]\w*Client$/.test(a0) && /^[A-Z]\w*Command$/.test(a1)) emit("aws-client+command", `${a0} ${a1}`);
        break;
      }
      case "Literal": case "TemplateLiteral": {
        const v = n.type === "Literal" ? (typeof n.value === "string" ? n.value : null) : n.quasis.map((q) => q.value.cooked ?? "").join("${}");
        if (v == null) break;
        let m;
        if ((m = /^\[?(@[a-z0-9][\w.-]*\/[\w.-]+|[a-z][\w.-]{2,})\]?: \S/.exec(v)) && !/^(error|warning|note|usage|info|debug|http|https|example|default|options|expected|received|value|type|key|path|file|name|code|line|column|error code)$/i.test(m[1])) emit("error-prefix", m[1]);
        if ((m = /^(@?[a-z][\w.-]*(?:\/[\w.-]+)?)\/(\$\{\}|v?\d+\.\d+\.\d+)/.exec(v))) emit("user-agent", m[1]);
        if ((m = /node_modules\/((?:@[\w.-]+\/)?[\w.-]+)/.exec(v))) emit("node_modules-path", m[1]);
        if ((m = /github\.com\/([\w.-]+\/[\w.-]+)/.exec(v))) emit("github-repo", m[1].replace(/\.git$/, ""));
        if ((m = /^x-([a-z0-9]+)-[a-z0-9-]+$/i.exec(v))) emit("x-vendor-header", m[1].toLowerCase());
        if (/^[A-Z]\w+Command$/.test(v) && v.length > 10) emit("aws-command-string", v);
        break;
      }
      case "BinaryExpression": {
        const l = str(n.left);
        if (n.operator === "+" && l && /^@?[a-z][\w.-]*(\/[\w.-]+)?\/$/.test(l)) emit("user-agent", l.slice(0, -1));
        break;
      }
      case "MemberExpression": {
        if (!n.computed && n.object.type === "MemberExpression" && !n.object.computed && n.object.property.name === "env" && /^[A-Z][A-Z0-9_]{3,}$/.test(n.property.name)) emit("env-var", n.property.name);
        break;
      }
    }
    for (const k in n) {
      if (k === "type" || k === "start" || k === "end") continue;
      const v = n[k];
      if (Array.isArray(v)) { for (const c of v) if (c && typeof c.type === "string") stack.push(c); }
      else if (v && typeof v.type === "string") stack.push(v);
    }
  }
}

if (!isMainThread) {
  const { tasks, out } = workerData;
  const fd = fs.openSync(out, "w");
  let cache = null, cachePath = null;
  for (const t of tasks) {
    let code;
    if (t.slice) { if (cachePath !== t.path) { cache = fs.readFileSync(t.path, "utf8"); cachePath = t.path; } code = cache.slice(t.slice[0], t.slice[1]); }
    else code = fs.readFileSync(t.path, "utf8");
    const hits = new Map();
    const emit = (det, val) => { const k = det + "\u0000" + val; hits.set(k, (hits.get(k) || 0) + 1); };
    if (t.json) { try { const j = JSON.parse(code); if (j && typeof j.name === "string" && typeof j.version === "string") emit("package.json-file", `${j.name}@${j.version}`); } catch {} }
    else census(code, emit);
    const buf = [];
    for (const [k, c] of hits) { const [det, val] = k.split("\u0000"); buf.push(JSON.stringify({ ...t.meta, det, val, count: c })); }
    if (buf.length) fs.writeSync(fd, buf.join("\n") + "\n");
  }
  fs.closeSync(fd);
  parentPort.postMessage("done");
} else {
  const jobs = Number(process.argv[process.argv.indexOf("--jobs") + 1]) || 60;
  const sources = new Map();
  const walk = (dir, owner) => {
    let ents; try { ents = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
    let own = owner;
    if (ents.some((e) => e.name === "package.json" && e.isFile())) { try { const pj = JSON.parse(fs.readFileSync(dir + "/package.json", "utf8")); if (pj.name && pj.version) own = { pkg: pj.name, ver: pj.version, root: dir }; } catch {} }
    for (const e of ents) {
      const p = dir + "/" + e.name;
      if (e.isDirectory()) { if (e.name !== ".bin") walk(p, own); }
      else if (e.isFile() && own && /\.(m?js|cjs)$/.test(e.name)) {
        const rel = path.relative(own.root, p); if (rel.startsWith("node_modules/")) continue;
        const key = `${own.pkg}@${own.ver}/${rel}`; if (sources.has(key) || fs.statSync(p).size > 4e6) continue;
        sources.set(key, { path: p, meta: { pkg: own.pkg, ver: own.ver, file: rel } });
      }
    }
  };
  for (const d of fs.readdirSync(LAB + "pkgs")) walk(LAB + "pkgs/" + d + "/node_modules", null);
  walk(HERE + "pkgs-cc/node_modules", null);
  const cc = [];
  for (const v of ["2.1.198", "2.1.216"]) {
    const lay = JSON.parse(fs.readFileSync(`${LAB}realworld/layout-${v}.json`, "utf8"));
    lay.modules.forEach((m, i) => cc.push({ path: CCIN(v), slice: [m.startChar, m.endChar], meta: { cc: v, i, kind: m.kind, startChar: m.startChar, bytes: m.endChar - m.startChar } }));
  }
  for (const [name, tasks] of [["sources", [...sources.values()]], ["cc", cc]]) {
    const n = Math.min(jobs, tasks.length), parts = Array.from({ length: n }, () => []);
    if (name === "cc") tasks.forEach((t, i) => parts[Math.floor((i * n) / tasks.length)].push(t)); else tasks.forEach((t, i) => parts[i % n].push(t));
    await Promise.all(parts.map((p, k) => new Promise((res, rej) => { const w = new Worker(new URL(import.meta.url), { workerData: { tasks: p, out: `${HERE}results/census-${name}.part${k}` } }); w.on("message", res); w.on("error", rej); })));
    const out = fs.openSync(`${HERE}results/census-hits-${name}.jsonl`, "w");
    for (let k = 0; k < n; k++) { fs.writeSync(out, fs.readFileSync(`${HERE}results/census-${name}.part${k}`)); fs.unlinkSync(`${HERE}results/census-${name}.part${k}`); }
    fs.closeSync(out);
    console.log(name, tasks.length, "inputs");
  }
}
