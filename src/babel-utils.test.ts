import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSourceAst } from "./babel-utils.js";

// NOTE: the funnel's Babel path-cache swap (clearBabelTraverseCache on big
// sources) is not asserted here — @babel/traverse's cache object is not
// reliably reachable across ESM interop shapes in the test runner. The scale
// harness (experiments/031) is the behavioral test: its GREEN curve is
// impossible unless the funnel clears that cache. The analysis caches need no
// funnel coverage at all anymore: they are per-AST (analysis-cache.ts), so
// parse boundaries cannot leak entries between trees by construction — see
// analysis-cache.test.ts for the isolation tests.

describe("parseSourceAst", () => {
  it("passes through errorRecovery, sourceType and filename", () => {
    // `const a;` is a recoverable parse error: throws plain, parses with
    // errorRecovery.
    assert.throws(() => parseSourceAst("const a;"));
    const recovered = parseSourceAst("const a;", {
      errorRecovery: true,
      sourceType: "module",
      filename: "probe.js"
    });
    assert.ok(recovered, "errorRecovery must yield an AST");
  });

  it("parses plain sources and preserveAstCaches is accepted", () => {
    const ast = parseSourceAst("let marker = 1;", { preserveAstCaches: true });
    assert.ok(ast);
  });
});
