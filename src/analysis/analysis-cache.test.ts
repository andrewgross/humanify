import assert from "node:assert";
import { describe, it } from "node:test";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { parseSourceAst, traverse } from "../babel-utils.js";
import {
  analysisCacheForPath,
  analysisCacheForScope
} from "./analysis-cache.js";
import {
  computeStructuralHash,
  computeStructuralSignature
} from "./structural-hash.js";

/** Parse and return the Program path plus every Function path. */
function parseProgram(code: string): {
  ast: t.File;
  program: NodePath<t.Program>;
  functions: NodePath<t.Function>[];
} {
  const ast = parseSourceAst(code) as t.File;
  let program: NodePath<t.Program> | undefined;
  const functions: NodePath<t.Function>[] = [];
  traverse(ast, {
    Program(path) {
      program = path as NodePath<t.Program>;
    },
    Function(path) {
      functions.push(path as NodePath<t.Function>);
    }
  });
  if (!program) throw new Error("no Program");
  return { ast, program, functions };
}

const SOURCE = `
const shared = 1;
function alpha(x) { return shared + x; }
function beta(y) { return alpha(y) * shared; }
`;

describe("analysisCacheForPath", () => {
  it("resolves every path of one AST to the same cache instance", () => {
    const { program, functions } = parseProgram(SOURCE);
    const cache = analysisCacheForPath(program);
    for (const fn of functions) {
      assert.strictEqual(analysisCacheForPath(fn), cache);
    }
    assert.strictEqual(analysisCacheForScope(functions[0].scope), cache);
  });

  it("gives independent caches to independently parsed ASTs", () => {
    const a = parseProgram(SOURCE);
    const b = parseProgram(SOURCE);
    assert.notStrictEqual(
      analysisCacheForPath(a.program),
      analysisCacheForPath(b.program)
    );
  });
});

describe("per-AST cache isolation", () => {
  it("hashing one AST fills only that AST's cache", () => {
    const a = parseProgram(SOURCE);
    const b = parseProgram(SOURCE);

    computeStructuralHash(a.functions[0]);

    const cacheA = analysisCacheForPath(a.program);
    const cacheB = analysisCacheForPath(b.program);
    assert.ok(
      cacheA.bindingByIdentifier.size > 0,
      "hashed AST must have binding entries"
    );
    assert.strictEqual(
      cacheB.bindingByIdentifier.size,
      0,
      "un-hashed AST must stay empty"
    );
  });

  it("hashes are deterministic across separately parsed (and cached) ASTs", () => {
    const a = parseProgram(SOURCE);
    const b = parseProgram(SOURCE);
    assert.strictEqual(
      computeStructuralHash(a.functions[0]),
      computeStructuralHash(b.functions[0])
    );
    assert.strictEqual(
      computeStructuralSignature(a.program),
      computeStructuralSignature(b.program)
    );
  });
});

describe("cache-era robustness", () => {
  it("re-hashing after a rename reuses the same slots (rename invariance)", () => {
    const { program, functions } = parseProgram(SOURCE);
    const before = computeStructuralSignature(program);
    // Rename a binding the cache already resolved; a pure rename must not
    // change the signature even though cached entries now carry the old name.
    functions[0].scope.rename("x", "renamedParam");
    program.scope.rename("shared", "renamedShared");
    assert.strictEqual(computeStructuralSignature(program), before);
  });

  it("survives a scope re-crawl between partial hash walks (era mixing)", () => {
    // Hash ONE function first: its identifiers are now cached with Binding
    // objects from the first crawl era. Then force a full re-crawl (what a
    // Babel cache clear + fresh traverse does): resolving the REMAINING
    // identifiers yields brand-new Binding objects. A whole-program hash now
    // mixes both eras for the same logical binding (`shared` inside alpha is
    // cached, `shared` at top level resolves fresh) — slots must still unify,
    // or every Babel-cache boundary silently corrupts hashes.
    const { program, functions } = parseProgram(SOURCE);
    const cold = parseProgram(SOURCE);
    const expected = computeStructuralSignature(cold.program);

    computeStructuralHash(functions[0]);
    program.scope.crawl();

    assert.strictEqual(computeStructuralSignature(program), expected);
  });
});
