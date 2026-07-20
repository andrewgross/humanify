import assert from "node:assert";
import { describe, it } from "node:test";
import {
  clearBabelTraverseCache,
  parseSourceAst,
  traverse
} from "./babel-utils.js";

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

describe("clearBabelTraverseCache", () => {
  it("actually swaps Babel's module-level path/scope cache maps", () => {
    // The cache namespace hangs off the RESOLVED traverse function
    // (interop-dependent: `ns.default.default.cache` under tsx). A probe
    // chain that misses it turns every era boundary in the pipeline into
    // a silent no-op — one process-lifetime ephemeron table, the
    // nondeterministic split-phase Rehash pin. This test pins the clear
    // to observable behavior: the live `path` map binding must change.
    const cache = (
      traverse as unknown as {
        cache: { path: WeakMap<object, object> };
      }
    ).cache;
    assert.ok(cache?.path instanceof WeakMap, "cache namespace reachable");
    const before = cache.path;
    clearBabelTraverseCache();
    assert.notStrictEqual(
      cache.path,
      before,
      "clear() must replace the path cache map (silent no-op otherwise)"
    );
  });
});
