import assert from "node:assert";
import { describe, it } from "node:test";
import { stageFingerprint } from "./stage-fingerprint.js";

/**
 * A null control of `neutrality.sh` diverged once in 34 runs — identical `src/`
 * on both legs, 15 files and 212 lines different. Two attributions (timing,
 * path dependence) were tested and refuted, and 33 subsequent controls did not
 * reproduce it, so re-rolling is low-yield.
 *
 * What IS known came from deduction, not from catching one: both legs wrote
 * ZERO cache entries, and the cache key is a sha256 over the whole request
 * including `alreadyRenamed`, so both legs asked byte-identical prompts. The
 * divergence therefore arose DOWNSTREAM of naming. That inference took an
 * evening; a recorded hash at the stage boundary makes it a lookup.
 *
 * This is deliberately a fingerprint of a STAGE BOUNDARY, not of the tree: the
 * tree diff already tells you THAT two runs differ. What no artifact currently
 * records is WHERE they first differed.
 */
describe("stageFingerprint", () => {
  it("is stable for identical input — a differing hash means real divergence", () => {
    assert.strictEqual(
      stageFingerprint("var a = 1;"),
      stageFingerprint("var a = 1;")
    );
  });

  it("separates inputs that differ at all, including only in whitespace", () => {
    assert.notStrictEqual(
      stageFingerprint("var a = 1;"),
      stageFingerprint("var a = 2;")
    );
    // Formatting is emitted output too — a stage that reflows code has
    // diverged, even if every name matches.
    assert.notStrictEqual(
      stageFingerprint("var a = 1;"),
      stageFingerprint("var a  = 1;")
    );
  });

  it("is short enough to eyeball in a log line", () => {
    const fp = stageFingerprint("x");
    assert.match(fp, /^[0-9a-f]{16}$/, "16 hex chars: comparable at a glance");
  });
});
