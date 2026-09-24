// WP3.1 probe: the rename freeze's consumer — `isBindingEvalTaintFrozen`
// (src/analysis/soundness.ts) for EVERY binding of each snippet, on the
// real TS functions. The Rust `RenameState::is_eval_taint_frozen` must
// agree binding for binding.
//
// Run: npx tsx test/parity/wp31-soundness-probe.mjs > test/parity/wp31-soundness.json
import { parseSync } from "@babel/core";

const { traverse } = await import("../../src/babel-utils.js");
const { collectEvalWithTaint, isBindingEvalTaintFrozen } = await import(
  "../../src/analysis/soundness.js"
);

const SNIPPETS = [
  "var m = 1; function outer(a) { var b; function inner(c) { with (a) { c; } } function safe(d) { return d; } }",
  "var m = 1; function runner(code) { let x = 1; return eval(code); } function clean(y) { return y + 1; }",
  "function f() { var eval = (s) => s; return eval('1'); } var n = 2;",
  "var q = 1; class K { m(p) { eval(p); } static { let s = 1; eval('s'); } n(r) { return r; } }",
  "var w = 1; class L { static { let t = 1; } static { eval('x'); var u; } }",
  "var o = { m(p) { return eval(p); }, n(r) { return r; } }; const arrow = (z) => eval(z);",
  "var z = 1; function g(a) { return a; }"
];

const out = SNIPPETS.map((code) => {
  const ast = parseSync(code, {
    sourceType: "script",
    configFile: false,
    babelrc: false
  });
  const taint = collectEvalWithTaint(ast);
  const bindings = [];
  const seen = new Set();
  traverse(ast, {
    enter(path) {
      for (const b of Object.values(path.scope.bindings)) {
        if (seen.has(b)) continue;
        seen.add(b);
        bindings.push(b);
      }
    }
  });
  bindings.sort((a, b) => a.identifier.start - b.identifier.start);
  return {
    code,
    siteCount: taint.siteCount,
    frozen: bindings.map((b) => ({
      id: [b.identifier.start, b.identifier.end],
      name: b.identifier.name,
      frozen: isBindingEvalTaintFrozen(b, taint)
    }))
  };
});
console.log(JSON.stringify({ cases: out }, null, 1));
