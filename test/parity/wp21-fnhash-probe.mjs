// WP2.1 debug probe: structural-hash one function (or binding) per side and
// print hash + slot mapping, for diffing TS-vs-Rust cross-side equality.
// Usage: npx tsx test/parity/wp21-fnhash-probe.mjs <text.js> <start> <end> [--bindings]
//   --bindings: hash the binding fingerprint at the span instead
//     (computeBindingFingerprint on the binding's init path).
import fs from "node:fs";
import { parseSync, traverse } from "@babel/core";
import {
  hashPathWithMapping,
  computeBindingFingerprint
} from "../../src/analysis/structural-hash.ts";

const [file, startArg, endArg, flag] = process.argv.slice(2);
const start = Number(startArg);
const end = Number(endArg);
const text = fs.readFileSync(file, "utf8");

const ast = parseSync(text, {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
});
let hit = null;
traverse(ast, {
  "Function|VariableDeclarator|AssignmentExpression"(p) {
    const _s = p.node.start ?? p.node.loc?.start?.index ?? -1;
    // babel: node.start is the offset
    if (p.node.start === start && p.node.end === end) hit = p;
  }
});
if (!hit) {
  console.error(`no node at [${start},${end})`);
  process.exit(1);
}
if (flag === "--bindings") {
  const fp = computeBindingFingerprint(hit);
  console.log(JSON.stringify(fp));
} else if (hit.isFunction()) {
  const { hash, mapping } = hashPathWithMapping(hit);
  console.log(
    JSON.stringify({
      hash,
      type: hit.node.type,
      mapping: [...mapping.entries()]
    })
  );
} else {
  console.error(`node at span is ${hit.node.type}; use --bindings`);
  process.exit(1);
}
