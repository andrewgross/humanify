import assert from "node:assert";
import { describe, it } from "node:test";
import { registerNodeCacheReset } from "./analysis/node-caches.js";
import {
  BIG_SOURCE_BYTES,
  parseFileAst,
  parseSourceAst,
  transformWithPlugins
} from "./babel-utils.js";

// Delta-counting spy: the reset registry is append-only by design
// (node-caches.test.ts precedent) — assert deltas around calls, never totals.
let resetCount = 0;
registerNodeCacheReset(() => {
  resetCount++;
});

/** A >=BIG_SOURCE_BYTES source that still parses in milliseconds: the bulk
 * is one line comment. The funnel gates on code.length, not AST size. */
const BIG_SOURCE = `// ${"x".repeat(BIG_SOURCE_BYTES)}\nlet bigMarker = 1;`;
const SMALL_SOURCE = "let smallMarker = 1;";

describe("parseSourceAst cache-era funnel", () => {
  it("resets the registered analysis caches when a big source is parsed", () => {
    const before = resetCount;
    const ast = parseSourceAst(BIG_SOURCE);
    assert.ok(ast, "big source must parse");
    assert.strictEqual(resetCount, before + 1);
  });

  it("does not reset for small sources", () => {
    const before = resetCount;
    const ast = parseSourceAst(SMALL_SOURCE);
    assert.ok(ast);
    assert.strictEqual(resetCount, before);
  });

  it("preserveAstCaches suppresses the reset (prior-bundle parse)", () => {
    const before = resetCount;
    const ast = parseSourceAst(BIG_SOURCE, { preserveAstCaches: true });
    assert.ok(ast);
    assert.strictEqual(resetCount, before);
  });

  // NOTE: Babel's internal path-cache swap (clearBabelTraverseCache inside
  // the funnel) is not asserted here — @babel/traverse's cache object is not
  // reliably reachable across ESM interop shapes in the test runner. The
  // scale harness (experiments/031) is the behavioral test: its GREEN curve
  // is impossible unless the funnel clears that cache too.
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
});

describe("parseFileAst safety net", () => {
  it("inherits the big-source reset (covers the split parse and future sites)", () => {
    const before = resetCount;
    const ast = parseFileAst(BIG_SOURCE);
    assert.ok(ast);
    assert.strictEqual(resetCount, before + 1);
  });

  it("small sources stay reset-free (per-file split/relink parses)", () => {
    const before = resetCount;
    parseFileAst(SMALL_SOURCE);
    assert.strictEqual(resetCount, before);
  });
});

describe("transformWithPlugins (site J: pre-rename babel transform)", () => {
  it("starts a fresh cache era before transforming a big source", async () => {
    const before = resetCount;
    const out = await transformWithPlugins(BIG_SOURCE, []);
    assert.ok(out.includes("bigMarker"));
    assert.ok(resetCount >= before + 1, "big transform must reset caches");
  });

  it("does not reset for small sources", async () => {
    const before = resetCount;
    await transformWithPlugins(SMALL_SOURCE, []);
    assert.strictEqual(resetCount, before);
  });
});
