// WP3.1 finding — a REAL TS validated-rename capture (reported, NOT fixed:
// the port reproduces the TS verdict; `src/` is the read-only oracle).
//
// Renaming a catch parameter to the name of a `var` declared in its own
// catch body is APPLIED, and changes behavior. Annex B.3.5 lets
// `var x` redeclare a catch parameter; the declaration hoists to the
// function (here: program) scope, but its INITIALIZER runs inside the catch
// block, where `x` now resolves to the catch parameter. The outer `x` is
// never assigned. The guard misses it because wouldCaptureOuterReference
// looks only at the outer binding's referencePaths and constantViolations,
// and a binding's own first declaration (`var x = err`) is neither.
//
// Run: npx tsx test/parity/wp31-catch-var-capture-repro.mjs
import { parseSync } from "@babel/core";

const { generate, traverse } = await import("../../src/babel-utils.js");
const { attemptValidatedRename } = await import(
  "../../src/rename/validated-rename.js"
);

const code =
  "function f() { try { throw 5; } catch (err) { var x = err; } return x; }";
const ast = parseSync(code, {
  sourceType: "script",
  configFile: false,
  babelrc: false
});
let catchScope;
traverse(ast, {
  CatchClause(path) {
    catchScope = path.scope;
  }
});
const attempt = attemptValidatedRename(catchScope, "err", "x");
const out = generate(ast).code;
const run = (src) => new Function(`${src}; return f();`)();
console.log("attempt:", JSON.stringify(attempt));
console.log("before:", run(code), " after:", run(out));
console.log(out);
