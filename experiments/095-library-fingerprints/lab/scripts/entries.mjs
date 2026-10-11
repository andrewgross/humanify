// usage: node scripts/entries.mjs  -> pkgs/<d>/entries/{req.cjs,imp.mjs,some.mjs,dyn.mjs}
// Modes: req  = require() every spec (CJS entry)          -> Bun/esbuild: CJS factory or lazy ESM (__esm + __toCommonJS)
//        imp  = import * as ns (ESM entry, whole namespace) -> eager (scope-hoisted) ESM, or CJS factory for CJS pkgs
//        some = import {a,b,c} (3 named exports)            -> eager ESM, tree-shaken subset
//        dyn  = import() every spec                         -> lazy ESM (__esm) / CJS factory
// Each entry also holds a small piece of app code with a unique string so <app> spans exist.
import fs from "node:fs"; import path from "node:path";
const lab = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const cat = Object.fromEntries(JSON.parse(fs.readFileSync(lab + "/scripts/catalog.json")).packages.map((p) => [p.name, p]));
const app = (tag) => `function labAppMain_${tag}(x){ if (!x) throw new Error("lab-app ${tag}: no input given"); return Object.keys(x).length + ":${tag}"; }\n`;
const RESERVED = new Set("break case catch class const continue debugger default delete do else enum export extends false finally for function if import in instanceof new null return super switch this throw true try typeof var void while with yield let static implements interface package private protected public await arguments eval".split(" "));
let n = 0;
for (const p of JSON.parse(fs.readFileSync(lab + "/versions.json"))) for (const v of Object.values(p.versions)) {
  const d = `${p.name.replace("/", "__")}@${v}`; const dir = path.join(lab, "pkgs", d, "entries");
  fs.rmSync(dir, { recursive: true, force: true }); fs.mkdirSync(dir, { recursive: true });
  const specs = cat[p.name].specs || [p.name];
  const ex = JSON.parse(fs.readFileSync(path.join(lab, "pkgs", d, "exports.json")));
  fs.writeFileSync(path.join(dir, "req.cjs"), app("req") + specs.map((s, i) => `const m${i} = require(${JSON.stringify(s)});`).join("\n") + `\nmodule.exports = { run: labAppMain_req, ${specs.map((_, i) => "m" + i).join(", ")} };\n`);
  fs.writeFileSync(path.join(dir, "imp.mjs"), app("imp") + specs.map((s, i) => `import * as m${i} from ${JSON.stringify(s)};`).join("\n") + `\nexport { labAppMain_imp, ${specs.map((_, i) => "m" + i).join(", ")} };\n`);
  fs.writeFileSync(path.join(dir, "dyn.mjs"), app("dyn") + `export const loaders = [${specs.map((s) => `() => import(${JSON.stringify(s)})`).join(", ")}];\nexport { labAppMain_dyn };\n`);
  const names = specs.length === 1 && ex[specs[0]] ? ex[specs[0]].filter((k) => k !== "default" && k !== "__esModule" && /^[A-Za-z_$][\w$]*$/.test(k) && !RESERVED.has(k)).sort() : [];
  if (names.length >= 2) {
    const pick = [...new Set([Math.floor(names.length / 4), Math.floor(names.length / 2), Math.floor((3 * names.length) / 4)])].map((i) => names[i]);
    fs.writeFileSync(path.join(dir, "some.mjs"), app("some") + `import { ${pick.join(", ")} } from ${JSON.stringify(specs[0])};\nexport { labAppMain_some, ${pick.join(", ")} };\n`);
  }
  n++;
}
console.log("apps", n);
