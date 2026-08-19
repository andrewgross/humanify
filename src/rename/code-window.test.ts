import assert from "node:assert";
import { describe, it } from "node:test";
import {
  capContextCode,
  MAX_CODE_LINES,
  selectFunctionCode
} from "./code-window.js";

function makeLines(n: number): string {
  return Array.from({ length: n }, (_, i) => `  line(${i + 1});`).join("\n");
}

describe("selectFunctionCode", () => {
  it("returns code unchanged when at or under the cap", () => {
    const code = makeLines(MAX_CODE_LINES);
    const result = selectFunctionCode({ code, sessionId: "t" });
    assert.strictEqual(result, code);
  });

  it("falls back to flat truncation when locs are missing", () => {
    const code = makeLines(600);
    const result = selectFunctionCode({ code, sessionId: "t" });
    const lines = result.split("\n");
    assert.strictEqual(lines.length, MAX_CODE_LINES + 2);
    assert.match(result, /\[truncated\]/);
    assert.ok(result.includes("line(500);"));
    assert.ok(!result.includes("line(501);"));
  });

  it("falls back to flat truncation when the loc span disagrees with the line count", () => {
    const code = makeLines(600);
    const result = selectFunctionCode({
      code,
      sessionId: "t",
      fnStartLine: 100,
      fnEndLine: 650, // span 551 != 600 lines
      anchorStartLines: [630]
    });
    assert.match(result, /\[truncated\]/);
    assert.ok(!result.includes("line(531);"));
  });

  it("windows around a past-cap anchor and keeps the header and closing line", () => {
    const code = makeLines(1000);
    const result = selectFunctionCode({
      code,
      sessionId: "t",
      fnStartLine: 2000,
      fnEndLine: 2999, // span 1000 == line count
      anchorStartLines: [2800] // relative line 801
    });
    assert.ok(result.includes("line(801);"), "anchor line must be visible");
    assert.ok(result.includes("line(1);"), "header must be included");
    assert.ok(result.includes("line(1000);"), "closing line must be included");
    assert.match(result, /omitted/);
    assert.ok(
      result.split("\n").length <= MAX_CODE_LINES + 4,
      "selection must respect the budget"
    );
    // A far-away line stays hidden.
    assert.ok(!result.includes("line(400);"));
  });

  it("merges overlapping anchor windows", () => {
    const code = makeLines(1000);
    const result = selectFunctionCode({
      code,
      sessionId: "t",
      fnStartLine: 1,
      fnEndLine: 1000,
      anchorStartLines: [700, 710, 715]
    });
    // One contiguous block: exactly one elision between header and block,
    // one before the closing line.
    const markers = result
      .split("\n")
      .filter((l) => l.includes("omitted")).length;
    assert.strictEqual(markers, 2);
    assert.ok(result.includes("line(700);"));
    assert.ok(result.includes("line(715);"));
  });

  it("folds anchors outside the function range into the header", () => {
    const code = makeLines(800);
    const result = selectFunctionCode({
      code,
      sessionId: "t",
      fnStartLine: 100,
      fnEndLine: 899,
      anchorStartLines: [50, undefined] // before the function; unknown
    });
    assert.ok(result.includes("line(1);"));
    assert.ok(result.includes("line(800);"), "closing line still present");
    assert.ok(result.split("\n").length < 100, "no spurious windows");
  });

  it("caps prior-version context at the code budget", () => {
    // Close-matched megafunctions used to embed the FULL prior function
    // in every batch prompt — 3,500-line priors overflowed the model's
    // 32K context and 400-failed whole batches (exp015 baseline).
    const code = makeLines(3000);
    const capped = capContextCode(code, "t");
    assert.ok(capped.split("\n").length <= MAX_CODE_LINES + 2);
    assert.ok(capped.includes("line(500);"));
    assert.ok(!capped.includes("line(501);"));
    // Under the cap: unchanged.
    const small = makeLines(100);
    assert.strictEqual(capContextCode(small, "t"), small);
  });

  it("shrinks padding to fit many spread anchors within the budget", () => {
    const code = makeLines(3000);
    const anchors = Array.from({ length: 10 }, (_, i) => 200 + i * 280);
    const result = selectFunctionCode({
      code,
      sessionId: "t",
      fnStartLine: 1,
      fnEndLine: 3000,
      anchorStartLines: anchors
    });
    for (const a of anchors) {
      assert.ok(result.includes(`line(${a});`), `anchor ${a} must be visible`);
    }
    assert.ok(
      result.split("\n").length <= MAX_CODE_LINES + 12,
      `selection must respect the budget, got ${result.split("\n").length} lines`
    );
  });
});

/**
 * `selectRequestCode` promises "every requested identifier is visible". It is
 * not kept when an identifier's declaration loc is missing or falls outside the
 * function's own loc range: `resolveAnchors` drops those silently, so the
 * binding gets no window and the model is asked to name a symbol it cannot see.
 *
 * Measured at 4 identifiers in 171,756 on a cold run — rare, but each one is a
 * name invented blind that then churns against the previous release's blind
 * invention. `cr_2` -> `unusedParameterPlaceholder` is one of them.
 */
describe("selectFunctionCode — identifiers with no usable anchor", () => {
  const oversized = (declLine: number, name: string): string => {
    const lines = Array.from({ length: 900 }, (_, i) =>
      i + 1 === declLine
        ? `  let ${name} = compute(${i});`
        : `  line(${i + 1});`
    );
    return `function big() {\n${lines.join("\n")}\n}`;
  };

  it("shows an identifier whose declaration loc is UNKNOWN", () => {
    const name = "cr_2";
    const selected = selectFunctionCode({
      code: oversized(700, name),
      sessionId: "t.js:1:0",
      fnStartLine: 1,
      fnEndLine: 902,
      // undefined = the binding carried no loc. Today this anchor is dropped.
      anchorStartLines: [undefined],
      identifierNames: [name]
    });
    assert.ok(
      selected.includes(name),
      `requested identifier ${name} is not in the code shown to the model`
    );
  });

  it("shows an identifier whose declaration loc is OUTSIDE the function range", () => {
    const name = "qt";
    const selected = selectFunctionCode({
      code: oversized(800, name),
      sessionId: "t.js:1:0",
      fnStartLine: 1,
      fnEndLine: 902,
      // A loc past the function's own end — dropped by the range check.
      anchorStartLines: [5000],
      identifierNames: [name]
    });
    assert.ok(
      selected.includes(name),
      `requested identifier ${name} is not in the code shown to the model`
    );
  });
});
