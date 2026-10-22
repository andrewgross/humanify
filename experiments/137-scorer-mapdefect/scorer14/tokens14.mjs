// exp137 scorer 14: the NAME EVIDENCE read by a real JS tokenizer (acorn), for the map-defect layer
// (mapdefect14.py). Two modes, JSON on stdin, JSON on stdout. Run with NODE_PATH holding acorn.
//   {"mode": "units", "bundle": path, "spans": [[startByte, endByte], ...]}
//     -> per span the unit's evidence: string and template-chunk values, `.property` names and object-literal keys
//        of 4+ characters (`{key:` / `,key:`): what no minifier renames (exp136's evtokens.mjs, unchanged)
//   {"mode": "files", "files": [path, ...], "want": [token, ...], "cap": N}
//     -> {"hits": [[file index, ...] per wanted token (at most cap)], "many": [token index, ...]}: which files hold
//        each wanted token as a string / template value or a name of any position (a property, a key, a
//        shorthand, a method). A file that does not tokenize (TypeScript, flow) adds, from the error on, its quoted
//        strings and identifiers read by a regex. Exact values: the caller verifies finalists by substring.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const acorn = require("acorn");
const OPTS = { ecmaVersion: "latest", allowReturnOutsideFunction: true, allowHashBang: true,
  allowAwaitOutsideFunction: true, sourceType: "script" };

function tokens(text, all) {
  try {
    for (const t of acorn.tokenizer(text, OPTS)) all.push(t);
    return -1;
  } catch (e) {
    return typeof e.pos === "number" ? e.pos : 0;
  }
}

function unitEvidence(text) {
  const all = [];
  tokens(text, all);
  const toks = new Set();
  for (let k = 0; k < all.length; k++) {
    const t = all[k];
    const label = t.type.label;
    const prev = k > 0 ? all[k - 1].type.label : "";
    const next = k + 1 < all.length ? all[k + 1].type.label : "";
    if ((label === "string" || label === "template") && typeof t.value === "string") toks.add(t.value);
    else if (label === "name" && t.value.length >= 4 && (prev === "." || prev === "?." ||
             (next === ":" && (prev === "{" || prev === ",")))) toks.add(t.value);
  }
  return [...toks];
}

const STR = /"((?:[^"\\\n]|\\.){0,400})"|'((?:[^'\\\n]|\\.){0,400})'|([A-Za-z_$][\w$]{3,})/g;

function fileTokens(text) {
  const all = [];
  const err = tokens(text, all);
  const toks = new Set();
  for (const t of all) {
    const label = t.type.label;
    if ((label === "string" || label === "template") && typeof t.value === "string") toks.add(t.value);
    else if (label === "name" || (t.type.keyword && typeof t.value === "string")) toks.add(String(t.value));
  }
  if (err >= 0) {
    for (const m of text.slice(err).matchAll(STR)) toks.add(m[1] ?? m[2] ?? m[3]);
  }
  return toks;
}

const req = JSON.parse(readFileSync(0, "utf8"));
if (req.mode === "units") {
  const buf = readFileSync(req.bundle);
  process.stdout.write(JSON.stringify(req.spans.map(([s, e]) => unitEvidence(buf.subarray(s, e).toString("utf8")))));
} else if (req.mode === "files") {
  const want = new Map(req.want.map((t, i) => [t, i]));
  const hits = req.want.map(() => []);
  const many = new Set();
  req.files.forEach((p, fi) => {
    let text;
    try {
      text = readFileSync(p, "utf8");
    } catch {
      return;
    }
    for (const t of fileTokens(text)) {
      const i = want.get(t);
      if (i === undefined) continue;
      if (hits[i].length < req.cap) hits[i].push(fi);
      else many.add(i);
    }
  });
  process.stdout.write(JSON.stringify({ hits, many: [...many] }));
} else {
  throw new Error(`unknown mode ${req.mode}`);
}
