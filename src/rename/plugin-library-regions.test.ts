/**
 * Library regions must classify functions in the SAME coordinate space the
 * regions were found in (finding #32). The banners are found in the RAW file
 * text; the rename pass sees the BEAUTIFIED text, where a minified file has
 * expanded (app functions before a banner land past it) and every comment is
 * gone (library functions after a banner slide before it). These tests drive
 * the real two-stage chain — beautify, then rename — with the per-file
 * context built exactly as `unminify.processFile` builds it.
 */
import assert from "node:assert";
import { describe, it } from "node:test";
import { findCommentRegions } from "../library-detection/comment-regions.js";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import type { FileContext } from "../pipeline/types.js";
import { createBabelPlugin } from "../plugins/babel/babel.js";
import { createRenamePlugin } from "./plugin.js";

const mockProvider: LLMProvider = {
  async suggestAllNames(request: BatchRenameRequest) {
    const renames: Record<string, string> = {};
    for (const id of request.identifiers) renames[id] = `${id}Renamed`;
    return { renames };
  }
};

/** Beautify then rename, sharing one FileContext — the unified.ts chain. */
async function runChain(raw: string): Promise<string> {
  const context: FileContext = {
    filePath: "index.js",
    commentRegions: findCommentRegions(raw)
  };
  assert.ok(context.commentRegions?.length, "the input must carry a banner");
  const beautified = await createBabelPlugin()(raw, context);
  const result = await createRenamePlugin({ provider: mockProvider })(
    beautified,
    context
  );
  assert.strictEqual(result.parseFailure, undefined);
  assert.strictEqual(result.semanticFailure, undefined);
  return result.code;
}

describe("library regions classify in the raw text's coordinates (#32)", () => {
  it("MINIFIED input: the app function before the banner is not library code", async () => {
    // One line; the banner sits past 1,024 chars so the header scan does not
    // claim the whole file. Beautify expands `var a0=0;` etc. by ~4 chars
    // each, so appFn's beautified start lands PAST the raw banner offset.
    const decls = Array.from({ length: 20 }, (_, i) => `var a${i}=${i};`);
    const raw = `var t="${"q".repeat(1100)}";console.log(t);${decls.join("")}var appFn=function(n){return n+1};console.log(appFn(2));/*! tinylib v1.2.3 */var libFn=function(z){return z*2};console.log(libFn(3));`;
    const out = await runChain(raw);
    assert.doesNotMatch(
      out,
      /tinylib_n/,
      "app param must not carry the prefix"
    );
    assert.match(out, /nRenamed/, "app function must be named normally");
    assert.match(out, /tinylib_z/, "library param must carry the prefix");
    assert.doesNotMatch(out, /zRenamed/, "library function must be frozen");
  });

  it("COMMENTED input: every library function after the banner is library code", async () => {
    // Beautify drops comments: the 1,100-char header comment disappears, so
    // every function after the banner slides BEFORE the raw banner offset.
    const raw = [
      `// ${"padding line for the header scan window. ".repeat(30)}`,
      "var appCounter = 5;",
      "console.log(appCounter);",
      "/*! tinylib v1.2.3 */",
      "var x1 = (r1) => {",
      "  var e1 = 111;",
      "  return e1 + r1;",
      "};",
      "var x2 = (r2) => {",
      "  var e2 = 222;",
      "  return e2 + r2;",
      "};",
      "console.log(x1, x2);"
    ].join("\n");
    const out = await runChain(raw);
    assert.match(out, /tinylib_r1/, "first library function must be frozen");
    assert.match(out, /tinylib_r2/, "second library function must be frozen");
    assert.doesNotMatch(out, /r1Renamed|r2Renamed/);
  });

  it("an eval-using library function keeps its names (finding #43)", async () => {
    // Direct eval resolves `z` and `k` by their ORIGINAL names at runtime;
    // the prefix pass renamed them anyway and the shipped code threw
    // ReferenceError while the run exited 0.
    const raw = `var t="${"q".repeat(1100)}";console.log(t.length);/*! tinylib v1.2.3 */var b=function(z){var k=1;return eval("z+k")};console.log(b(2));`;
    const out = await runChain(raw);
    assert.doesNotMatch(out, /tinylib_z|tinylib_k/);
    assert.match(out, /function \(z\)/);
    assert.match(out, /var k = 1/);
  });
});
