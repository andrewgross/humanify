/**
 * Does `structuralHash` — the manifest's cross-version join key — cover
 * literal VALUES? Task C's option C1 (reuse the prior vendor body when the
 * structural hash matches) ships wrong code if it does not.
 *
 * Run: npx tsx experiments/046-vendor-noise/hash-probe.ts
 */
import { parseSourceAst, traverse } from "../../src/babel-utils.js";
import { computeStructuralHash } from "../../src/analysis/structural-hash.js";

function h(src: string): string {
  const ast = parseSourceAst(src);
  if (!ast) throw new Error(`probe source failed to parse: ${src}`);
  let out = "";
  traverse(ast, {
    FunctionExpression(p) {
      if (!out) out = computeStructuralHash(p);
      p.stop();
    }
  });
  return out;
}

// Same-length / same-magnitude pairs: the honest test. A pair that differs in
// literal LENGTH can pass for the wrong reason (`__STR_5__` vs `__STR_4__`).
const cases: Array<[string, string, string]> = [
  [
    "control: local rename only",
    "(function(a,b){var c=1;return a+b+c})",
    "(function(x,y){var z=1;return x+y+z})"
  ],
  [
    "string, SAME length",
    "(function(a){return a+'alpha'})",
    "(function(a){return a+'omega'})"
  ],
  [
    "string, different length",
    "(function(a){return a+'alpha'})",
    "(function(a){return a+'beta'})"
  ],
  [
    "url string, same length",
    "(function(){return fetch('https://a.example/v1')})",
    "(function(){return fetch('https://b.example/v2')})"
  ],
  [
    "number, same magnitude bucket",
    "(function(a){return a+1})",
    "(function(a){return a+2})"
  ],
  [
    "number, same bucket (large)",
    "(function(a){return a+1000})",
    "(function(a){return a+2000})"
  ],
  [
    "number, different bucket",
    "(function(a){return a+1})",
    "(function(a){return a+1000})"
  ],
  ["bigint", "(function(){return 1n})", "(function(){return 99999n})"],
  [
    "template text, same length",
    "(function(a){return `xx${a}`})",
    "(function(a){return `yy${a}`})"
  ],
  [
    "property name",
    "(function(a){return a.foo})",
    "(function(a){return a.bar})"
  ],
  [
    "free identifier",
    "(function(a){return globalFoo(a)})",
    "(function(a){return globalBar(a)})"
  ],
  [
    "regex pattern",
    "(function(a){return /ab/.test(a)})",
    "(function(a){return /cd/.test(a)})"
  ],
  ["operator", "(function(a,b){return a+b})", "(function(a,b){return a-b})"]
];

let unsafe = 0;
for (const [label, A, B] of cases) {
  const same = h(A) === h(B);
  const control = label.startsWith("control");
  if (same && !control) unsafe++;
  const verdict = control
    ? same
      ? "invariant (as designed)"
      : "BROKEN — control must be invariant"
    : same
      ? "COLLIDES  <-- change is INVISIBLE"
      : "distinct";
  console.log(`${verdict.padEnd(34)} ${label}`);
}
console.log(
  `\n${unsafe} of ${cases.length - 1} semantic differences are INVISIBLE to structuralHash.`
);
