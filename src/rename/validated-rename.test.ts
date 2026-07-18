import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import { generate, traverse } from "../babel-utils.js";
import {
  attemptValidatedRename,
  fastRenameBinding,
  getRenameRejection
} from "./validated-rename.js";

function parseWithScopes(
  code: string,
  sourceType: "module" | "script" = "module"
): {
  ast: t.File;
  programScope: Scope;
  functionScopes: Scope[];
} {
  const ast = parseSync(code, { sourceType });
  if (!ast) throw new Error("Failed to parse test fixture");
  let programScope: Scope | undefined;
  const functionScopes: Scope[] = [];
  traverse(ast, {
    Program(path) {
      programScope = path.scope;
    },
    Function(path) {
      functionScopes.push(path.scope);
    }
  });
  if (!programScope) throw new Error("No program scope");
  return { ast: ast as t.File, programScope, functionScopes };
}

describe("fastRenameBinding input guard", () => {
  it("throws when handed a builtin target — callers must validate first", () => {
    // fastRenameBinding is the mutation primitive; a caller that skips
    // getRenameRejection could otherwise bind `document` and capture
    // every document.* read in scope.
    const { programScope } = parseWithScopes("var a = 1;");
    assert.throws(
      () => fastRenameBinding(programScope, "a", "document"),
      /invalid rename target/i
    );
  });

  it("throws when handed a reserved word target", () => {
    const { programScope } = parseWithScopes("var a = 1;");
    assert.throws(
      () => fastRenameBinding(programScope, "a", "class"),
      /invalid rename target/i
    );
  });
});

describe("attemptValidatedRename", () => {
  it("applies a valid rename and rewrites references", () => {
    const { ast, programScope } = parseWithScopes("var a = 1; console.log(a);");
    const result = attemptValidatedRename(programScope, "a", "fetchCount");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    assert.match(code, /var fetchCount = 1/);
    assert.match(code, /console\.log\(fetchCount\)/);
  });

  it("rejects a reserved word target", () => {
    const { ast, programScope } = parseWithScopes("var a = 1;");
    const result = attemptValidatedRename(programScope, "a", "delete");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "invalid-target");
    assert.match(generate(ast).code, /var a = 1/);
  });

  it("rejects a global builtin target", () => {
    const { programScope } = parseWithScopes("var a = 1;");
    const result = attemptValidatedRename(programScope, "a", "Map");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "invalid-target");
  });

  it("rejects an invalid identifier target", () => {
    const { programScope } = parseWithScopes("var a = 1;");
    const result = attemptValidatedRename(programScope, "a", "foo-bar");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "invalid-target");
  });

  it("rejects when the old name is not bound in the scope", () => {
    const { programScope } = parseWithScopes("var a = 1;");
    const result = attemptValidatedRename(programScope, "missing", "found");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "no-binding");
  });

  it("rejects when the target is already bound in the same scope", () => {
    const { ast, programScope } = parseWithScopes("var a = 1; var b = 2;");
    const result = attemptValidatedRename(programScope, "a", "b");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "target-in-scope");
    assert.match(generate(ast).code, /var a = 1/);
  });

  it("rejects two sequential renames to the same target", () => {
    // The NH failure shape: two bindings in one scope transferred to one name
    const { ast, programScope } = parseWithScopes("var a = 1; var b = 2;");
    const first = attemptValidatedRename(programScope, "a", "shared");
    const second = attemptValidatedRename(programScope, "b", "shared");
    assert.strictEqual(first.applied, true);
    assert.strictEqual(second.applied, false);
    assert.strictEqual(second.reason, "target-in-scope");
    const code = generate(ast).code;
    assert.match(code, /var shared = 1/);
    assert.match(code, /var b = 2/);
  });

  it("rejects when an ancestor binding of the target is referenced in scope", () => {
    const { functionScopes } = parseWithScopes(
      "var helper = 1; function f(a) { return a + helper; }"
    );
    const result = attemptValidatedRename(functionScopes[0], "a", "helper");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "target-visible");
  });

  it("rejects when a child scope binds the target around a reference", () => {
    const { functionScopes } = parseWithScopes(
      "function f(a) { { let helper = 1; console.log(a, helper); } }"
    );
    const result = attemptValidatedRename(functionScopes[0], "a", "helper");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "shadows-child");
  });

  it("allows a target bound only in an unrelated sibling scope", () => {
    const { functionScopes } = parseWithScopes(
      "function f(a) { return a; } function g(helper) { return helper; }"
    );
    const result = attemptValidatedRename(functionScopes[0], "a", "helper");
    assert.strictEqual(result.applied, true);
  });

  it("renames writes and destructuring violations, not just reads", () => {
    const { ast, programScope } = parseWithScopes(
      "var a = 1; a = 2; a += 3; [a] = [4]; console.log(a);"
    );
    const result = attemptValidatedRename(programScope, "a", "counter");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    assert.ok(!/\ba\b/.test(code), `no occurrence of 'a' may remain:\n${code}`);
    assert.match(code, /var counter = 1/);
    assert.match(code, /counter = 2/);
    assert.match(code, /counter \+= 3/);
    assert.match(code, /\[counter\] = \[4\]/);
  });

  it("renames a function declaration name and its call sites", () => {
    const { ast, programScope } = parseWithScopes(
      "function a() { return 1; } a(); var r = a;"
    );
    const result = attemptValidatedRename(programScope, "a", "getOne");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    assert.match(code, /function getOne\(\)/);
    assert.match(code, /getOne\(\)/);
    assert.match(code, /var r = getOne/);
  });

  it("renames a duplicate var declaration's second declarator", () => {
    // Review C2: Babel records the second `var a` as a constantViolation
    // whose path is the VariableDeclarator. Leaving it unrenamed silently
    // splits the binding — the program prints 1,1 instead of 1,2.
    const { ast, programScope } = parseWithScopes(
      "var a = 1; console.log(a); var a = 2; console.log(a);"
    );
    const result = attemptValidatedRename(programScope, "a", "counter");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    assert.ok(!/\ba\b/.test(code), `no occurrence of 'a' may remain:\n${code}`);
    assert.match(code, /var counter = 1/);
    assert.match(code, /var counter = 2/);
  });

  it("renames a duplicate function declaration's name", () => {
    // Legal only in sloppy scripts — which is what bundler output is.
    const { ast, programScope } = parseWithScopes(
      "function a() { return 1; } console.log(a()); function a() { return 2; }",
      "script"
    );
    const result = attemptValidatedRename(programScope, "a", "getValue");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    assert.ok(!/\ba\b/.test(code), `no occurrence of 'a' may remain:\n${code}`);
    assert.match(code, /console\.log\(getValue\(\)\)/);
    const declCount = code.match(/function getValue\(\)/g)?.length ?? 0;
    assert.strictEqual(declCount, 2, `both declarations renamed:\n${code}`);
  });

  it("renames a duplicate var re-declared in a for-of head", () => {
    const { ast, programScope } = parseWithScopes(
      "var a = 1; for (var a of [2]) { console.log(a); }"
    );
    const result = attemptValidatedRename(programScope, "a", "item");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    assert.ok(!/\ba\b/.test(code), `no occurrence of 'a' may remain:\n${code}`);
    assert.match(code, /for \(var item of/);
  });

  it("renames a duplicate destructuring declarator target", () => {
    const { ast, programScope } = parseWithScopes(
      "var source = { x: 2 }; var a = 1; var { x: a } = source; console.log(a);"
    );
    const result = attemptValidatedRename(programScope, "a", "count");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    assert.ok(!/\ba\b/.test(code), `no occurrence of 'a' may remain:\n${code}`);
    assert.match(code, /\{\s*x: count\s*\}/);
  });

  it("rejects renaming to a browser global the file reads (review C1 executed case)", () => {
    // Was APPLIED before the fix: output parses, but document.title reads
    // a number at runtime. `document` is now in GLOBAL_BUILTINS.
    const { ast, programScope } = parseWithScopes(
      "var d = 1; console.log(document.title, d);"
    );
    const result = attemptValidatedRename(programScope, "d", "document");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "invalid-target");
    assert.match(generate(ast).code, /var d = 1/);
  });

  it("rejects renaming to a custom name the file uses as a global", () => {
    // myAppGlobal is in no builtin list — only the file's own observed
    // free names can catch it. Invariant: a rename may never bind a
    // previously-free name.
    const { ast, programScope } = parseWithScopes(
      "var d = 1; console.log(myAppGlobal.title, d);"
    );
    const result = attemptValidatedRename(programScope, "d", "myAppGlobal");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "target-free-name");
    assert.match(generate(ast).code, /var d = 1/);
  });

  it("rejects when the free reference lives inside a nested function", () => {
    const { programScope } = parseWithScopes(
      "var d = 1; function f() { return myAppGlobal.title + d; }"
    );
    const result = attemptValidatedRename(programScope, "d", "myAppGlobal");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "target-free-name");
  });

  it("allows a target that appears nowhere as a free identifier", () => {
    const { programScope } = parseWithScopes(
      "var d = 1; console.log(myAppGlobal.title, d);"
    );
    const result = attemptValidatedRename(programScope, "d", "userCount");
    assert.strictEqual(result.applied, true);
  });

  it("preserves the external name when renaming an exported binding", () => {
    const { ast, programScope } = parseWithScopes(
      "export const a = 1; console.log(a);"
    );
    const result = attemptValidatedRename(programScope, "a", "counter");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    // Whatever form Babel's renamer chooses, consumers must still see `a`.
    assert.ok(
      /export\s*\{[^}]*\bas a\b[^}]*\}/.test(code) ||
        /export const a\b/.test(code),
      `external export name 'a' must be preserved:\n${code}`
    );
    assert.strictEqual(
      validateOutputParsesForTest(code),
      null,
      `renamed export must parse:\n${code}`
    );
  });
});

/** Local parse check helper (mirrors the pipeline's output validation). */
function validateOutputParsesForTest(code: string): string | null {
  const ast = parseSync(code, { sourceType: "module" });
  return ast ? null : "parse failed";
}

/** The scope OWNING the binding of `name` (block scopes included). */
function bindingScopeOf(code: string, name: string): Scope {
  const { ast } = (() => {
    const parsed = parseSync(code, { sourceType: "module" });
    if (!parsed) throw new Error("Failed to parse test fixture");
    return { ast: parsed as t.File };
  })();
  let found: Scope | undefined;
  traverse(ast, {
    Identifier(path) {
      if (found || path.node.name !== name) return;
      const binding = path.scope.getBinding(name);
      if (binding) found = binding.scope;
    }
  });
  if (!found) throw new Error(`no binding for ${name}`);
  return found;
}

describe("getRenameRejection outer-capture precision", () => {
  // The 2.1.166 transport bug: an inner env-object local was renamed to
  // `authRequestInstance`, the SAME name as an outer variable ASSIGNED
  // inside the same block — the assignment re-resolved to the inner
  // binding (capture), breaking semantics and permanently flipping the
  // function's binding-keyed structural hash on every later version hop.
  it("rejects capturing an outer binding written inside the renamed binding's scope", () => {
    const code = `
      function connect(cfg) {
        let transport;
        if (cfg) {
          let env = { a: 1 };
          transport = { env: env };
        }
        return transport;
      }`;
    const scope = bindingScopeOf(code, "env");
    const rejection = getRenameRejection(scope, "env", "transport");
    assert.notStrictEqual(
      rejection,
      null,
      "renaming env→transport captures the `transport = ...` write"
    );
  });

  it("rejects capturing an outer binding read inside the renamed binding's scope", () => {
    const code = `
      function connect(cfg) {
        let transport = mk();
        if (cfg) {
          let env = { a: 1 };
          console.log(transport, env);
        }
        return transport;
      }`;
    const scope = bindingScopeOf(code, "env");
    const rejection = getRenameRejection(scope, "env", "transport");
    assert.notStrictEqual(
      rejection,
      null,
      "renaming env→transport captures the `console.log(transport)` read"
    );
  });

  it("rejects capture across a nested function boundary", () => {
    const code = `
      function connect(cfg) {
        let transport;
        const setup = () => {
          let env = { a: 1 };
          transport = { env: env };
        };
        return [setup, transport];
      }`;
    const scope = bindingScopeOf(code, "env");
    const rejection = getRenameRejection(scope, "env", "transport");
    assert.notStrictEqual(
      rejection,
      null,
      "capture risk is the same when the write sits in a nested function"
    );
  });

  it("allows shadowing an outer binding with no references inside the renamed binding's scope", () => {
    // Cosmetic shadowing: the outer name exists but is never referenced
    // inside the block, so no reference can re-resolve. Rejecting these
    // (the old blanket ancestor-visibility check) starved close-match
    // transfers and LLM suggestions of perfectly safe names.
    const code = `
      function process(cfg) {
        let helperCount = 1;
        if (cfg) {
          let env = { a: 1 };
          console.log(env);
        }
        return helperCount;
      }`;
    const scope = bindingScopeOf(code, "env");
    const rejection = getRenameRejection(scope, "env", "helperCount");
    assert.strictEqual(
      rejection,
      null,
      "no reference of helperCount lies inside the block — shadowing is safe"
    );
  });

  // The 2.1.110 shipped collision (issue-runnable-trees-dont-run #1):
  // `for (let validationErrorList of validationErrorList)`. Whichever
  // binding is renamed second must be rejected — the loop head's iterable
  // sits inside the ForOfStatement scope, so the loop `let` capturing it
  // is a TDZ crash, not a cosmetic shadow.
  it("rejects renaming a for-of loop variable to the iterated binding's name", () => {
    const code = `
      function build(list) {
        let validationErrorList = list.filter(Boolean);
        for (let entry of validationErrorList) {
          console.log(entry);
        }
        return validationErrorList;
      }`;
    const scope = bindingScopeOf(code, "entry");
    assert.strictEqual(scope.block.type, "ForOfStatement");
    assert.strictEqual(
      getRenameRejection(scope, "entry", "validationErrorList"),
      "target-visible"
    );
  });

  it("rejects renaming the iterated binding to the for-of loop variable's name", () => {
    const code = `
      function build(list) {
        let allEntries = list.filter(Boolean);
        for (let validationErrorList of allEntries) {
          console.log(validationErrorList);
        }
        return allEntries;
      }`;
    const scope = bindingScopeOf(code, "allEntries");
    assert.strictEqual(
      getRenameRejection(scope, "allEntries", "validationErrorList"),
      "shadows-child"
    );
  });

  it("rejects the for-in variant of the loop-head collision", () => {
    const code = `
      function walk(obj) {
        let keyMap = obj.entries;
        for (const propKey in keyMap) {
          console.log(propKey, keyMap[propKey]);
        }
      }`;
    const loopScope = bindingScopeOf(code, "propKey");
    assert.strictEqual(
      getRenameRejection(loopScope, "propKey", "keyMap"),
      "target-visible"
    );
    const outerScope = bindingScopeOf(code, "keyMap");
    assert.strictEqual(
      getRenameRejection(outerScope, "keyMap", "propKey"),
      "shadows-child"
    );
  });

  it("still rejects a target bound in the same scope", () => {
    const code = `
      function f() {
        let alpha = 1;
        let beta = 2;
        return alpha + beta;
      }`;
    const scope = bindingScopeOf(code, "beta");
    assert.strictEqual(
      getRenameRejection(scope, "beta", "alpha"),
      "target-in-scope"
    );
  });
});
