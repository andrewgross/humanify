/**
 * Integration test for prior-version matching using real preact fixtures.
 *
 * This validates the full pipeline: parse → graph build → fingerprint → match → translate names.
 * Uses the actual preact v10.25.0 and v10.26.0 minified bundles.
 *
 * The prior version must be "humanified" (identifiers renamed to descriptive names).
 * We simulate this by renaming all single-char identifiers in v10.25.0 to
 * prefixed names — this mimics what the LLM pass produces.
 */
import assert from "node:assert";
import fs from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as babelTraverse from "@babel/traverse";
import { generate, traverse } from "../babel-utils.js";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import type { FunctionNode } from "../analysis/types.js";
import { matchPriorVersion } from "./prior-version.js";

const FIXTURES_DIR = path.resolve("test/e2e/fixtures/preact/minified");

function readFixture(version: string, minifier: string): string {
  const filePath = path.join(FIXTURES_DIR, version, `${minifier}.js`);
  return fs.readFileSync(filePath, "utf-8");
}

/**
 * Simulate humanification by renaming all minified-looking bindings.
 *
 * The real pipeline (RenameProcessor) collects bindings from BOTH the function
 * scope AND nested block scopes (let/const in try/for/if blocks). We replicate
 * that by visiting every Scope node so block-scoped vars in closures get
 * humanified names — matching what happens when the LLM renames a parent function
 * and the renames propagate into its inner closures.
 */
function simulateHumanify(code: string): string {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("Failed to parse");

  traverse(ast, {
    Scope(scopePath: babelTraverse.NodePath) {
      for (const [name, binding] of Object.entries(scopePath.scope.bindings)) {
        // Only rename bindings owned by this scope (not inherited)
        if (binding.scope !== scopePath.scope) continue;
        // Rename short identifiers that look minified
        if (name.length <= 4 && /^[a-z_]/.test(name)) {
          const line = binding.identifier.loc?.start.line ?? 0;
          const col = binding.identifier.loc?.start.column ?? 0;
          const newName = `humanified_${name}_${line}_${col}`;
          scopePath.scope.rename(name, newName);
        }
      }
    }
  });

  return generate(ast).code;
}

function buildFunctions(code: string): Map<string, FunctionNode> {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("Failed to parse");
  const functions = buildFunctionGraph(ast, "test.js");
  return new Map(functions.map((f) => [f.sessionId, f]));
}

describe("prior-version matching with real preact fixtures", () => {
  it("matches functions between humanified v10.25.0 and minified v10.26.0", () => {
    const v25Code = readFixture("v10.25.0", "terser-default");
    const v26Code = readFixture("v10.26.0", "terser-default");

    // Simulate humanification of v10.25.0
    const humanifiedV25 = simulateHumanify(v25Code);

    // Build v10.26.0 functions as the "new" minified version
    const newFunctions = buildFunctions(v26Code);
    const totalNew = newFunctions.size;

    // Match humanified v25 against minified v26
    const result = matchPriorVersion(humanifiedV25, newFunctions);

    const totalMatched = result.functionsMatched + result.functionsAlreadyNamed;
    const totalMatchRate = totalNew > 0 ? totalMatched / totalNew : 0;

    console.log(
      `  Preact terser: ${totalMatched}/${totalNew} total matched (${(totalMatchRate * 100).toFixed(1)}%), ` +
        `${result.functionsMatched} renamed, ${result.functionsAlreadyNamed} already named`
    );
    console.log(
      `  Resolution: unique=${result.matchResult.resolutionStats.structuralHashUnique}, ` +
        `cascade=${result.matchResult.resolutionStats.memberKeyResolved + result.matchResult.resolutionStats.calleeShapesResolved + result.matchResult.resolutionStats.callerShapesResolved}, ` +
        `propagation=${result.matchResult.resolutionStats.propagationResolved}, ` +
        `ambiguous=${result.matchResult.resolutionStats.stillAmbiguous}, ` +
        `unmatched=${result.matchResult.resolutionStats.unmatched}`
    );

    // Should match a significant portion for a patch version
    assert.ok(totalMatched > 0, "Should match at least some functions");
    // No duplicate renames (each new function matched at most once)
    const matchedNewIds = new Set(result.matchResult.matches.values());
    assert.strictEqual(
      matchedNewIds.size,
      result.matchResult.matches.size,
      "Each new function should be matched at most once"
    );
  });

  it("transferred names contain humanified identifiers", () => {
    const v25Code = readFixture("v10.25.0", "terser-default");
    const v26Code = readFixture("v10.26.0", "terser-default");

    const humanifiedV25 = simulateHumanify(v25Code);
    const newFunctions = buildFunctions(v26Code);
    const result = matchPriorVersion(humanifiedV25, newFunctions);

    let withRenames = 0;
    let withEmptyMapping = 0;
    let totalRenames = 0;
    for (const fn of newFunctions.values()) {
      if (!fn.renameMapping) continue;
      const names = fn.renameMapping.names;
      if (Object.keys(names).length === 0) {
        withEmptyMapping++;
        continue;
      }
      withRenames++;
      for (const [oldName, newName] of Object.entries(names)) {
        assert.notStrictEqual(oldName, newName);
        assert.ok(oldName.length > 0);
        assert.ok(newName.length > 0);
        // New names should be our simulated humanified names
        assert.ok(
          newName.startsWith("humanified_"),
          `Expected humanified name but got: ${newName}`
        );
        totalRenames++;
      }
    }

    console.log(
      `  ${withRenames} renamed, ${withEmptyMapping} already named, ${totalRenames} total identifier mappings`
    );
    assert.ok(withRenames > 0, "Should transfer names to matched functions");
    assert.strictEqual(withRenames, result.functionsMatched);
    assert.strictEqual(withEmptyMapping, result.functionsAlreadyNamed);
  });

  it("matching is deterministic", () => {
    const v25Code = readFixture("v10.25.0", "terser-default");
    const v26Code = readFixture("v10.26.0", "terser-default");
    const humanifiedV25 = simulateHumanify(v25Code);

    const fns1 = buildFunctions(v26Code);
    const result1 = matchPriorVersion(humanifiedV25, fns1);

    const fns2 = buildFunctions(v26Code);
    const result2 = matchPriorVersion(humanifiedV25, fns2);

    assert.strictEqual(
      result1.functionsMatched,
      result2.functionsMatched,
      "Match count should be deterministic"
    );
  });
});
