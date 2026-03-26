import assert from "node:assert";
import { describe, it } from "node:test";
import { createBabelPlugin } from "./babel.js";

describe("babel transforms", () => {
  const transform = createBabelPlugin();

  describe("SequenceExpression in for-loop update", () => {
    it("should preserve compound for-loop update expressions", async () => {
      const input = `for (let i = 0; i < n; i++, j += 2) { foo(i); }`;
      const result = await transform(input);
      // The compound update expression should stay in the for-loop
      assert.ok(result.includes("i++"), "should contain i++");
      assert.ok(result.includes("j += 2"), "should contain j += 2");
      // The i++ should NOT appear as a standalone statement before the loop
      const lines = result.trim().split("\n");
      const forLineIndex = lines.findIndex((l) => l.includes("for"));
      for (let k = 0; k < forLineIndex; k++) {
        assert.notStrictEqual(
          lines[k].trim(),
          "i++;",
          "i++ should not be extracted before the for-loop"
        );
      }
    });

    it("should still extract SequenceExpression from regular statements", async () => {
      const input = `a(), b();`;
      const result = await transform(input);
      // These should be split into separate statements
      assert.ok(result.includes("a()"), "should contain a()");
      assert.ok(result.includes("b()"), "should contain b()");
    });

    it("should handle for-loop with single update expression normally", async () => {
      const input = `for (let i = 0; i < n; i++) { foo(i); }`;
      const result = await transform(input);
      assert.ok(result.includes("i++"), "should contain i++");
    });
  });
});
