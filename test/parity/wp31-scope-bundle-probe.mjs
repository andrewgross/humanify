// WP3.1 bundle-scale probe of Babel's scope model: the same rows as
// wp31-scope-probe.mjs, one JSON line per scope (in `Object.keys` binding
// order) and one per binding, for a WHOLE text (an oracle dump's
// text/fresh.js). The Rust side is `humanify scope-view <text> <out>`;
// the two files must be byte-identical (spans are UTF-16 code units on
// both sides — the Rust converts). Lesson 13: slice fixtures are for
// structure; the pipeline walks the full bundle.
//
// Run: node test/parity/wp31-scope-bundle-probe.mjs <text.js> <out.jsonl>
import fs from "node:fs";
import { parseSync } from "@babel/core";
import traverseMod from "@babel/traverse";

const traverse = traverseMod.default ?? traverseMod;
const [, , textPath, outPath] = process.argv;
const code = fs.readFileSync(textPath, "utf8");
const ast = parseSync(code, {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
});

const key = (s) => `${s.block.type}@${s.block.start}:${s.block.end}`;
const site = (p) => [p.node.type, p.node.start, p.node.end, key(p.scope)];

const scopes = new Set();
let program;
traverse(ast, {
  enter(path) {
    if (!program) program = path.scope.getProgramParent();
    scopes.add(path.scope);
  }
});
const lines = [];
const seen = new Set();
for (const s of scopes) {
  lines.push(
    // Keys alphabetical: serde_json's map order on the Rust side.
    JSON.stringify({
      names: Object.keys(s.bindings),
      parent: s.parent ? key(s.parent) : null,
      scope: key(s)
    })
  );
  for (const name of Object.keys(s.bindings)) {
    const b = s.bindings[name];
    if (seen.has(b)) continue;
    seen.add(b);
    lines.push(
      JSON.stringify({
        id: [b.identifier.start, b.identifier.end],
        kind: b.kind,
        name,
        owner: key(b.scope),
        refs: b.referencePaths.map(site),
        viol: b.constantViolations.map(site)
      })
    );
  }
}
lines.sort();
lines.push(JSON.stringify({ globals: Object.keys(program.globals).sort() }));
fs.writeFileSync(outPath, `${lines.join("\n")}\n`);
console.log(`${lines.length} lines -> ${outPath}`);
