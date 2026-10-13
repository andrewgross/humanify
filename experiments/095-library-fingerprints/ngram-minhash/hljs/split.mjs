// Split a Bun CJS bundle into its CommonJS factories; label each from the unminified twin's path comment.
// node split.mjs <dir>   -> <dir>/factories.jsonl + <dir>/f/<n>.js
import fs from "fs";
import { createRequire } from "module";
const require = createRequire("/work/exp095-library-fingerprints/lab/tools/package.json");
const acorn = require("acorn");
const dir = process.argv[2];
function factories(code) {
  const ast = acorn.parse(code, { ecmaVersion: "latest", sourceType: "script", allowHashBang: true, allowReturnOutsideFunction: true });
  const out = [];
  for (const st of ast.body) {
    if (st.type !== "VariableDeclaration") continue;
    for (const d of st.declarations) {
      const i = d.init;
      if (i && i.type === "CallExpression" && i.arguments.length === 1 && /Function/.test(i.arguments[0].type))
        out.push({ start: st.start, end: st.end });
    }
  }
  return out;
}
const min = fs.readFileSync(`${dir}/out.min.js`, "utf8");
const un = fs.readFileSync(`${dir}/out.unmin.js`, "utf8");
const fm = factories(min), fu = factories(un);
if (fm.length !== fu.length) throw new Error(`count ${fm.length} vs ${fu.length}`);
fs.mkdirSync(`${dir}/f`, { recursive: true });
const rows = [];
let prevEnd = 0;
fu.forEach((u, k) => {
  const before = un.slice(prevEnd, u.start);
  prevEnd = u.end;
  const m = [...before.matchAll(/\/\/ node_modules\/highlight\.js\/(\S+)/g)].pop();
  const text = min.slice(fm[k].start, fm[k].end);
  const p = `${dir}/f/${String(k).padStart(4, "0")}.js`;
  fs.writeFileSync(p, text);
  const nm = /name:"([^"]+)"/.exec(text);
  rows.push({ k, file: m ? m[1] : null, name: nm ? nm[1] : null, path: p, bytes: Buffer.byteLength(text) });
});
fs.writeFileSync(`${dir}/factories.jsonl`, rows.map((r) => JSON.stringify(r)).join("\n") + "\n");
console.log(dir, rows.length, rows.filter((r) => r.file).length);
