import assert from "node:assert";
import { createIsEligible } from "./rename-eligibility.js";
import { describe, it } from "node:test";
import { getProximateUsedNames } from "./proximity.js";

describe("getProximateUsedNames", () => {
  function makeBinding(line: number, refLines: number[] = []) {
    return {
      identifier: { loc: { start: { line } } },
      referencePaths: refLines.map((l) => ({
        node: { loc: { start: { line: l } } }
      }))
    };
  }

  it("always includes well-known names", () => {
    const allNames = new Set(["exports", "require", "console", "a", "b"]);
    const scopeBindings: Record<string, ReturnType<typeof makeBinding>> = {
      exports: makeBinding(1),
      require: makeBinding(2),
      console: makeBinding(3),
      a: makeBinding(1000), // far away
      b: makeBinding(1001) // far away
    };

    const result = getProximateUsedNames(
      allNames,
      [50],
      scopeBindings,
      200,
      createIsEligible()
    );

    assert.ok(result.has("exports"), "should include well-known 'exports'");
    assert.ok(result.has("require"), "should include well-known 'require'");
    assert.ok(result.has("console"), "should include well-known 'console'");
  });

  it("excludes eligible names", () => {
    // With the default isEligible, single-char names and descriptive names
    // are all eligible (everything is a rename candidate). Use an override
    // that treats only single-char names as eligible.
    const isEligible = (name: string) => name.length === 1;
    const allNames = new Set(["a", "b", "c", "myVar"]);
    const scopeBindings: Record<string, ReturnType<typeof makeBinding>> = {
      a: makeBinding(50),
      b: makeBinding(50),
      c: makeBinding(50),
      myVar: makeBinding(50)
    };

    const result = getProximateUsedNames(
      allNames,
      [50],
      scopeBindings,
      200,
      isEligible
    );

    assert.ok(!result.has("a"), "should exclude eligible 'a'");
    assert.ok(!result.has("b"), "should exclude eligible 'b'");
    assert.ok(!result.has("c"), "should exclude eligible 'c'");
    assert.ok(result.has("myVar"), "should include non-eligible 'myVar'");
  });

  it("includes names within +-100 lines, excludes those outside", () => {
    // Use an override that treats only single-char names as eligible,
    // so nearVar/farVar are preserved and subject to windowing
    const isEligible = (name: string) => name.length === 1;
    const allNames = new Set(["nearVar", "farVar"]);
    const scopeBindings: Record<string, ReturnType<typeof makeBinding>> = {
      nearVar: makeBinding(55), // within +-100 of line 50
      farVar: makeBinding(500) // far away from line 50
    };

    const result = getProximateUsedNames(
      allNames,
      [50],
      scopeBindings,
      200,
      isEligible
    );

    assert.ok(result.has("nearVar"), "should include name within proximity");
    assert.ok(!result.has("farVar"), "should exclude name outside proximity");
  });

  it("includes name if any reference is within proximity", () => {
    const isEligible = (name: string) => name.length === 1;
    const allNames = new Set(["refVar"]);
    const scopeBindings: Record<string, ReturnType<typeof makeBinding>> = {
      refVar: makeBinding(500, [45]) // declaration far, but reference near line 50
    };

    const result = getProximateUsedNames(
      allNames,
      [50],
      scopeBindings,
      200,
      isEligible
    );

    assert.ok(
      result.has("refVar"),
      "should include name whose reference is within proximity"
    );
  });

  it("returns all preserved names when below threshold", () => {
    const isEligible = (name: string) => name.length === 1;
    const allNames = new Set(["nearVar", "farVar", "a"]);
    const scopeBindings: Record<string, ReturnType<typeof makeBinding>> = {
      nearVar: makeBinding(50),
      farVar: makeBinding(500),
      a: makeBinding(50)
    };

    // totalBindings < 100 -> no windowing
    const result = getProximateUsedNames(
      allNames,
      [50],
      scopeBindings,
      50,
      isEligible
    );

    assert.ok(result.has("nearVar"), "should include nearVar");
    assert.ok(
      result.has("farVar"),
      "should include farVar (no windowing below threshold)"
    );
    assert.ok(!result.has("a"), "should still exclude eligible names");
  });
});

/**
 * 16-findings-queue #22 (finding #12's class, found by the Rust port,
 * WP3.3): `scopeBindings[name]` fell through to Object.prototype, so a used
 * name like `toString` (the LLM assigns such names) that is NOT bound in
 * this scope read the built-in function — truthy, no loc, no references —
 * and was EXCLUDED from the proximate used names, where a genuinely absent
 * name is included ("include if binding not found, to be safe").
 */
describe("getProximateUsedNames reads only own scope bindings", () => {
  it("includes an unbound toString like any other unbound name", () => {
    const scopeBindings: Record<
      string,
      { identifier: { loc: { start: { line: number } } } }
    > = {};
    for (let i = 0; i < 200; i++) {
      scopeBindings[`far${i}`] = {
        identifier: { loc: { start: { line: 9000 + i } } }
      };
    }
    const result = getProximateUsedNames(
      new Set(["toString", "someUnboundName"]),
      [50],
      scopeBindings,
      200,
      (name: string) => name.length === 1
    );
    assert.ok(result.has("someUnboundName"), "an unbound name is included");
    assert.ok(
      result.has("toString"),
      "an unbound toString must be included too"
    );
  });
});
