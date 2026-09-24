// WP3.1 bundle-scale probe of the validated-rename RULES: a deterministic
// rename SEQUENCE over every binding of a whole text (an oracle dump's
// text/fresh.js), run on the real attemptValidatedRename. Each step renames
// one binding (through `binding.scope`, from its CURRENT name) to a
// candidate chosen to exercise a different rule:
//
//   i % 6 == 0  the next binding's crawl name        (target-in-scope / shadowing)
//   i % 6 == 1  a pseudo-random binding's crawl name (target-visible / shadows-child)
//   i % 6 == 2  a program global                     (target-free-name / invalid-target)
//   i % 6 == 3  a fresh descriptive name `r<i>`      (applies)
//   i % 6 == 4  the first name in the binding's own scope map (target-in-scope)
//   i % 6 == 5  a below-floor mint `q<i % 97>`        (applies / collides)
//
// Applied renames change later verdicts, so the sequence tests the rules
// AND the overlay's bookkeeping at scale. One line per step; the Rust side
// is `humanify rename-probe <text> <out>` and the files must be identical.
//
// Run: npx tsx test/parity/wp31-rename-bundle-probe.mjs <text.js> <out.txt>
import fs from "node:fs";
import { parseSync } from "@babel/core";

const { traverse } = await import("../../src/babel-utils.js");
const { attemptValidatedRename } = await import(
  "../../src/rename/validated-rename.js"
);

const [, , textPath, outPath] = process.argv;
const code = fs.readFileSync(textPath, "utf8");
const ast = parseSync(code, {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
});

const bindings = [];
const seen = new Set();
let program;
traverse(ast, {
  enter(path) {
    if (!program) program = path.scope.getProgramParent();
    for (const b of Object.values(path.scope.bindings)) {
      if (seen.has(b)) continue;
      seen.add(b);
      bindings.push(b);
    }
  }
});
bindings.sort((a, b) => a.identifier.start - b.identifier.start);
const crawl = bindings.map((b) => b.identifier.name);
const globals = Object.keys(program.globals).sort();
const n = bindings.length;

function candidate(i, b) {
  switch (i % 6) {
    case 0:
      return crawl[(i + 1) % n];
    case 1:
      return crawl[(i * 7919) % n];
    case 2:
      return globals.length ? globals[i % globals.length] : `g${i}`;
    case 3:
      return `r${i}`;
    case 4:
      return Object.keys(b.scope.bindings)[0];
    default:
      return `q${i % 97}`;
  }
}

const lines = [];
for (let i = 0; i < n; i++) {
  const b = bindings[i];
  const old = b.identifier.name;
  const to = candidate(i, b);
  const r = attemptValidatedRename(b.scope, old, to);
  lines.push(`${i} ${old} ${to} ${r.applied ? "applied" : r.reason}`);
}
fs.writeFileSync(outPath, `${lines.join("\n")}\n`);
console.log(`${lines.length} steps -> ${outPath}`);
