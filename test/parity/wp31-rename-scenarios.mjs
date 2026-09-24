// WP3.1 validated-rename scenarios, shared by the TS probe
// (wp31-rename-probe.mjs) and — through the frozen wp31-rename.json — the
// Rust port's replay test. Labels name the TS test each scenario ports
// ("vr:" = validated-rename.test.ts, "era:" = scope-era.test.ts); "x:" are
// extra probes for predicates whose behavior is subtle (lesson 3).
const P = "program";
const at = (scope, old, nu, extra = {}) => ({
  op: "attempt",
  scope,
  old,
  new: nu,
  ...extra
});
const rej = (scope, old, nu) => ({ op: "rejection", scope, old, new: nu });

const ERA_CODE = `
function getFileWriter() {
  let outerDir = null;
  register({ writeFn: (task) => {
    let innerDir = dirname(getPath());
    let changed = outerDir !== innerDir;
    outerDir = innerDir;
    return changed;
  }});
}`;

export const SCENARIOS = [
  // --- validated-rename.test.ts: attemptValidatedRename -------------------
  {
    label: "vr: applies a valid rename and rewrites references",
    code: "var a = 1; console.log(a);",
    ops: [at(P, "a", "fetchCount")]
  },
  {
    label: "vr: rejects a reserved word target",
    code: "var a = 1;",
    ops: [at(P, "a", "delete")]
  },
  {
    label: "vr: rejects a global builtin target",
    code: "var a = 1;",
    ops: [at(P, "a", "Map")]
  },
  {
    label: "vr: rejects an invalid identifier target",
    code: "var a = 1;",
    ops: [at(P, "a", "foo-bar")]
  },
  {
    label: "vr: rejects when the old name is not bound in the scope",
    code: "var a = 1;",
    ops: [at(P, "missing", "found")]
  },
  {
    label: "vr: rejects when the target is already bound in the same scope",
    code: "var a = 1; var b = 2;",
    ops: [at(P, "a", "b")]
  },
  {
    label:
      "vr: rejects when the binding under the old name is not the expected one",
    code: "var a = 1; var b = 2;",
    ops: [
      { op: "capture", as: "ev", scope: P, name: "a" },
      at(P, "a", "loaded"),
      at(P, "b", "a"),
      at(P, "a", "fromEvidence", { expected: { captured: "ev" } }),
      { op: "capture", as: "cur", scope: P, name: "a" },
      at(P, "a", "fromEvidence", { expected: { captured: "cur" } })
    ]
  },
  {
    label: "vr: rejects two sequential renames to the same target",
    code: "var a = 1; var b = 2;",
    ops: [at(P, "a", "shared"), at(P, "b", "shared")]
  },
  {
    label:
      "vr: rejects when an ancestor binding of the target is referenced in scope",
    code: "var helper = 1; function f(a) { return a + helper; }",
    ops: [at({ fn: 0 }, "a", "helper")]
  },
  {
    label: "vr: rejects when a child scope binds the target around a reference",
    code: "function f(a) { { let helper = 1; console.log(a, helper); } }",
    ops: [at({ fn: 0 }, "a", "helper")]
  },
  {
    label: "vr: allows a target bound only in an unrelated sibling scope",
    code: "function f(a) { return a; } function g(helper) { return helper; }",
    ops: [at({ fn: 0 }, "a", "helper")]
  },
  {
    label: "vr: renames writes and destructuring violations, not just reads",
    code: "var a = 1; a = 2; a += 3; [a] = [4]; console.log(a);",
    ops: [at(P, "a", "counter")]
  },
  {
    label: "vr: renames a function declaration name and its call sites",
    code: "function a() { return 1; } a(); var r = a;",
    ops: [at(P, "a", "getOne")]
  },
  {
    label: "vr: renames a duplicate var declaration's second declarator",
    code: "var a = 1; console.log(a); var a = 2; console.log(a);",
    ops: [at(P, "a", "counter")]
  },
  {
    label: "vr: renames a duplicate function declaration's name",
    code: "function a() { return 1; } console.log(a()); function a() { return 2; }",
    sourceType: "script",
    ops: [at(P, "a", "getValue")]
  },
  {
    label: "vr: renames a duplicate var re-declared in a for-of head",
    code: "var a = 1; for (var a of [2]) { console.log(a); }",
    ops: [at(P, "a", "item")]
  },
  {
    label: "vr: renames a duplicate destructuring declarator target",
    code: "var source = { x: 2 }; var a = 1; var { x: a } = source; console.log(a);",
    ops: [at(P, "a", "count")]
  },
  {
    label:
      "vr: rejects renaming to a browser global the file reads (review C1 executed case)",
    code: "var d = 1; console.log(document.title, d);",
    ops: [at(P, "d", "document")]
  },
  {
    label: "vr: rejects renaming to a custom name the file uses as a global",
    code: "var d = 1; console.log(myAppGlobal.title, d);",
    ops: [at(P, "d", "myAppGlobal")]
  },
  {
    label: "vr: rejects when the free reference lives inside a nested function",
    code: "var d = 1; function f() { return myAppGlobal.title + d; }",
    ops: [at(P, "d", "myAppGlobal")]
  },
  {
    label: "vr: allows a target that appears nowhere as a free identifier",
    code: "var d = 1; console.log(myAppGlobal.title, d);",
    ops: [at(P, "d", "userCount")]
  },
  {
    label: "vr: preserves the external name when renaming an exported binding",
    code: "export const a = 1; console.log(a);",
    ops: [
      { op: "exportFlags", binding: { binding: "a" } },
      at(P, "a", "counter")
    ]
  },
  // --- validated-rename.test.ts: outer-capture precision -----------------
  {
    label:
      "vr: rejects capturing an outer binding written inside the renamed binding's scope",
    code: "\n      function connect(cfg) {\n        let transport;\n        if (cfg) {\n          let env = { a: 1 };\n          transport = { env: env };\n        }\n        return transport;\n      }",
    ops: [rej({ owner: "env" }, "env", "transport")]
  },
  {
    label:
      "vr: rejects capturing an outer binding read inside the renamed binding's scope",
    code: "\n      function connect(cfg) {\n        let transport = mk();\n        if (cfg) {\n          let env = { a: 1 };\n          console.log(transport, env);\n        }\n        return transport;\n      }",
    ops: [rej({ owner: "env" }, "env", "transport")]
  },
  {
    label: "vr: rejects capture across a nested function boundary",
    code: "\n      function connect(cfg) {\n        let transport;\n        const setup = () => {\n          let env = { a: 1 };\n          transport = { env: env };\n        };\n        return [setup, transport];\n      }",
    ops: [rej({ owner: "env" }, "env", "transport")]
  },
  {
    label:
      "vr: allows shadowing an outer binding with no references inside the renamed binding's scope",
    code: "\n      function process(cfg) {\n        let helperCount = 1;\n        if (cfg) {\n          let env = { a: 1 };\n          console.log(env);\n        }\n        return helperCount;\n      }",
    ops: [rej({ owner: "env" }, "env", "helperCount")]
  },
  {
    label:
      "vr: rejects renaming a for-of loop variable to the iterated binding's name",
    code: "\n      function build(list) {\n        let validationErrorList = list.filter(Boolean);\n        for (let entry of validationErrorList) {\n          console.log(entry);\n        }\n        return validationErrorList;\n      }",
    ops: [rej({ owner: "entry" }, "entry", "validationErrorList")]
  },
  {
    label:
      "vr: rejects renaming the iterated binding to the for-of loop variable's name",
    code: "\n      function build(list) {\n        let allEntries = list.filter(Boolean);\n        for (let validationErrorList of allEntries) {\n          console.log(validationErrorList);\n        }\n        return allEntries;\n      }",
    ops: [rej({ owner: "allEntries" }, "allEntries", "validationErrorList")]
  },
  {
    label: "vr: rejects the for-in variant of the loop-head collision",
    code: "\n      function walk(obj) {\n        let keyMap = obj.entries;\n        for (const propKey in keyMap) {\n          console.log(propKey, keyMap[propKey]);\n        }\n      }",
    ops: [
      rej({ owner: "propKey" }, "propKey", "keyMap"),
      rej({ owner: "keyMap" }, "keyMap", "propKey")
    ]
  },
  {
    label: "vr: still rejects a target bound in the same scope",
    code: "\n      function f() {\n        let alpha = 1;\n        let beta = 2;\n        return alpha + beta;\n      }",
    ops: [rej({ owner: "beta" }, "beta", "alpha")]
  },
  // --- validated-rename.test.ts: export-default preservation -------------
  {
    label: "vr: keeps the export default form when renaming the declaration id",
    code: "export default function mitt(e) { return e; }\n",
    ops: [
      { op: "exportFlags", binding: { binding: "mitt" } },
      at(P, "mitt", "createEventEmitter")
    ]
  },
  {
    label: "vr: keeps the named export declaration form when renaming its id",
    code: "export function mitt(e) { return e; }\nconst use = mitt;\nexport { use };\n",
    ops: [
      { op: "exportFlags", binding: { binding: "mitt" } },
      { op: "exportFlags", binding: { binding: "use" } },
      at(P, "mitt", "createEventEmitter")
    ]
  },
  {
    label: "vr: renames the default export's references in place too",
    code: "export default function mitt(e) { return e; }\nconst use = mitt;\nexport { use };\n",
    ops: [at(P, "mitt", "createEventEmitter")]
  },
  // --- scope-era.test.ts, single era (the Rust model has ONE scope table) --
  {
    label:
      "era: a rename applied AFTER a re-crawl, through a RETAINED old scope, is invisible (single era)",
    code: ERA_CODE,
    sourceType: "unambiguous",
    ops: [
      at({ owner: "innerDir" }, "innerDir", "dirPath"),
      at({ owner: "outerDir" }, "outerDir", "dirPath")
    ]
  },
  {
    label:
      "era: catches a SAME-SCOPE collision the other era's map has gone stale on (single era)",
    code: "function f() { let aa = 1, bb = 2; use(aa, bb); }",
    sourceType: "unambiguous",
    ops: [
      at({ owner: "aa" }, "aa", "dirPath"),
      at({ owner: "bb" }, "bb", "dirPath")
    ]
  },
  {
    label:
      "era: catches an ANCESTOR rename whose references sit inside the inner block (single era)",
    code: ERA_CODE,
    sourceType: "unambiguous",
    ops: [
      at({ owner: "outerDir" }, "outerDir", "dirPath"),
      at({ owner: "innerDir" }, "innerDir", "dirPath")
    ]
  },
  {
    label: "era: stays at zero for ordinary renames — no eras, nothing to flip",
    code: "function f() { let aa = 1; use(aa); }",
    sourceType: "unambiguous",
    ops: [at({ owner: "aa" }, "aa", "dirPath")]
  },
  {
    label:
      "era: does not count a ledger-sourced resolve that ends in NO capture",
    code: "function outer() { let aa = 1; use(aa); function inner() { let bb = 2; return bb; } }",
    sourceType: "unambiguous",
    ops: [
      at({ owner: "aa" }, "aa", "dirPath"),
      at({ owner: "bb" }, "bb", "dirPath")
    ]
  },
  // --- extra probes: the subtle predicates --------------------------------
  {
    label: "x: shadowing rename — class expression id takes the owner's name",
    code: "var X; X = class q { m() { return q; } }; var Y = class r {};",
    sourceType: "script",
    ops: [
      {
        op: "shadow",
        inner: { binding: "q" },
        owner: { binding: "X" },
        new: "X"
      },
      {
        op: "shadow",
        inner: { binding: "r" },
        owner: { binding: "Y" },
        new: "Y"
      }
    ]
  },
  {
    label:
      "x: shadowing rename — capture-in-subtree when the owner is referenced inside",
    code: "var X; X = class q extends X {};",
    sourceType: "script",
    ops: [
      {
        op: "shadow",
        inner: { binding: "q" },
        owner: { binding: "X" },
        new: "X"
      }
    ]
  },
  {
    label:
      "x: shadowing rename — owner name mismatch is no-binding; plain rename is target-visible",
    code: "var X = function f() { return X; };",
    sourceType: "script",
    ops: [
      {
        op: "shadow",
        inner: { binding: "f" },
        owner: { binding: "X" },
        new: "Z"
      },
      at({ owner: "f" }, "f", "X"),
      {
        op: "shadow",
        inner: { binding: "f" },
        owner: { binding: "X" },
        new: "X"
      }
    ]
  },
  {
    label:
      "x: shadowing rename — child scope binds the target around a reference",
    code: "var X = function f() { { let X = 1; return [f, X]; } };",
    sourceType: "script",
    ops: [
      {
        op: "shadow",
        inner: { binding: "f" },
        owner: { binding: "X" },
        new: "X"
      }
    ]
  },
  {
    label:
      "x: class declaration alias — the class scope keeps the crawl-time name",
    code: "class C { m(p) { return [C, p]; } } new C(); var v = 1; class D { m() { return v; } }",
    sourceType: "script",
    ops: [
      at(P, "C", "Klass"),
      at({ owner: "p" }, "p", "C"),
      at({ owner: "p" }, "p", "Klass"),
      at(P, "v", "C"),
      at(P, "v", "D"),
      rej(P, "Klass", "C")
    ]
  },
  {
    label: "x: arguments — a reserved target, and a free name for resolution",
    code: "function f(a) { return arguments[0] + a; } var arguments2 = 1;",
    sourceType: "script",
    ops: [at({ fn: 0 }, "a", "arguments"), at(P, "arguments2", "args")]
  },
  {
    label: "x: catch params — own clause scope, shadowing checks see the body",
    code: "var e = 0; try { f(); } catch (err) { var x = err; e = err; } finally { e; }",
    sourceType: "script",
    ops: [
      at({ owner: "err" }, "err", "e"),
      at({ owner: "err" }, "err", "x"),
      at({ owner: "err" }, "err", "error"),
      at(P, "x", "error"),
      at(P, "e", "err")
    ]
  },
  {
    label:
      "x: function expression name binding — local scope, visible to its body",
    code: "var g = function f(n) { return n ? f(n - 1) : 0; }; var n = 2;",
    sourceType: "script",
    ops: [
      at({ owner: "f" }, "f", "n"),
      at({ owner: "f" }, "f", "recurse"),
      at({ owner: "n" }, "n", "recurse"),
      at({ owner: "n" }, "n", "g")
    ]
  },
  {
    label:
      "x: Annex-B block function — Babel binds it in the block; the call after is a global",
    code: "function g() { if (a) { function h() {} h(); } h(); var k = 1; }",
    sourceType: "script",
    ops: [
      at({ owner: "k" }, "k", "h"),
      at({ owner: "h" }, "h", "helper"),
      at({ owner: "k" }, "k", "helper")
    ]
  },
  {
    label:
      "x: export var restructure — the Babel renamer path, then the fast path",
    code: "export const a = 1, b = 2; console.log(a, b);",
    ops: [
      { op: "exportFlags", binding: { binding: "a" } },
      at(P, "a", "first"),
      { op: "exportFlags", binding: { binding: "a" } },
      { op: "exportFlags", binding: { binding: "b" } },
      at(P, "first", "renamedAgain"),
      at(P, "b", "second")
    ]
  },
  {
    label:
      "x: export specifier — the local stays export-involved across renames",
    code: "const a = 1; export { a as b }; export { a };",
    ops: [
      { op: "exportFlags", binding: { binding: "a" } },
      at(P, "a", "first"),
      at(P, "first", "second")
    ]
  },
  {
    label: "x: params of an exported function are export-involved",
    code: "export function f(p) { return p; } export class K { m(q) { return q; } }",
    ops: [
      { op: "exportFlags", binding: { binding: "p" } },
      { op: "exportFlags", binding: { binding: "q" } },
      { op: "exportFlags", binding: { binding: "K" } },
      at({ owner: "p" }, "p", "value"),
      at({ owner: "q" }, "q", "value")
    ]
  },
  {
    label: "x: pattern param default — ref scope and the target-visible walk",
    code: "var z = 1; function q({ a = z }, b = z) { var y; return a + b; }",
    sourceType: "script",
    ops: [
      at({ owner: "a" }, "a", "z"),
      at({ owner: "y" }, "y", "z"),
      at(P, "z", "a"),
      at(P, "z", "y"),
      at(P, "z", "zz")
    ]
  },
  {
    label: "x: switch discriminant resolves outside the switch scope",
    code: "var sv = 1; function sw(p) { switch (sv) { case p: let w = 2; return w; } }",
    sourceType: "script",
    ops: [
      at({ owner: "w" }, "w", "sv"),
      at(P, "sv", "w"),
      at({ owner: "p" }, "p", "w")
    ]
  },
  {
    label:
      "x: computed method key resolves in the class scope (block span includes the key)",
    code: "var k = 1; class K { [k](m) { return m; } }",
    sourceType: "script",
    ops: [at({ owner: "m" }, "m", "k"), at(P, "k", "m")]
  },
  {
    label: "x: loop-body var carries its own declarator as a violation",
    code: "function f(H) { var q = 0; while (q < 3) { var $ = H[q]; q++; } { let $2; } }",
    sourceType: "script",
    ops: [
      at({ owner: "$" }, "$", "item"),
      at({ owner: "q" }, "q", "$2"),
      at({ owner: "q" }, "q", "idx")
    ]
  },
  {
    label: "x: repeated rename moves the name to the end of the scope map",
    code: "var a = 1, b = 2, c = 3;",
    ops: [at(P, "a", "x"), at(P, "x", "a"), at(P, "b", "a"), at(P, "b", "x")]
  },
  {
    label: "x: rename to the same name is target-in-scope",
    code: "var a = 1;",
    ops: [at(P, "a", "a")]
  },
  {
    label: "x: an undeclared assignment target is a free name",
    code: "var d = 1; later = d;",
    sourceType: "script",
    ops: [at(P, "d", "later")]
  },
  {
    label: "x: a destructuring for-of target undeclared is NOT a free name",
    code: "var d = 1; for ([u] of xs) {} for (w of xs) {}",
    sourceType: "script",
    ops: [at(P, "d", "u"), at(P, "d", "w")]
  },
  {
    label: "x: below-floor names (carried) and a descriptive target",
    code: "var a = 1, b = 2, c = 3;",
    ops: [at(P, "a", "q7"), at(P, "b", "fsPromises_"), at(P, "c", "count")]
  }
];
