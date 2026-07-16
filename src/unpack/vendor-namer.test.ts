import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import {
  acceptVendorName,
  buildVendorEvidence,
  createVendorNamer
} from "./vendor-namer.js";

function providerReturning(
  fn: (req: BatchRenameRequest) => Record<string, string>
): LLMProvider {
  return {
    async suggestAllNames(req: BatchRenameRequest) {
      return { renames: fn(req) };
    }
  };
}

describe("buildVendorEvidence", () => {
  it("collects export names, urls, and distinctive strings", () => {
    const body = [
      `exports.parse = function () {};`,
      `exports.stringify = q;`,
      `var msg = "YAMLException: bad indent";`,
      `var site = "https://github.com/nodeca/js-yaml";`,
      `var x = "ab";`
    ].join("\n");
    const evidence = buildVendorEvidence(body);
    assert.match(evidence, /parse/);
    assert.match(evidence, /stringify/);
    assert.match(evidence, /YAMLException/);
    assert.match(evidence, /github\.com\/nodeca\/js-yaml/);
    assert.doesNotMatch(evidence, /"ab"/, "too-short strings are noise");
  });
});

describe("acceptVendorName", () => {
  it("accepts package-shaped names, normalized", () => {
    assert.equal(acceptVendorName("js-yaml"), "js-yaml");
    assert.equal(acceptVendorName("@aws-sdk/client-s3"), "@aws-sdk/client-s3");
    assert.equal(acceptVendorName("Zod"), "zod");
  });

  it("rejects generic, minified, or malformed proposals", () => {
    for (const bad of [
      "lib",
      "library",
      "utils",
      "unknown",
      "module",
      "index",
      "vendor",
      "H",
      "a b c",
      ""
    ]) {
      assert.equal(acceptVendorName(bad), null, `${bad} must be rejected`);
    }
  });
});

describe("createVendorNamer", () => {
  it("names a whole batch in one provider call, keyed by request keys", async () => {
    let calls = 0;
    const namer = createVendorNamer(
      providerReturning((req) => {
        calls++;
        assert.deepEqual(req.identifiers, ["lib_00112233", "lib_aabbccdd"]);
        assert.match(req.code, /YAMLException/);
        return { lib_00112233: "js-yaml", lib_aabbccdd: "cheerio" };
      })
    );
    const result = await namer([
      { key: "lib_00112233", evidence: 'strings: "YAMLException"' },
      { key: "lib_aabbccdd", evidence: "exports: load" }
    ]);
    assert.deepEqual(result, ["js-yaml", "cheerio"]);
    assert.equal(calls, 1);
  });

  it("returns null per entry on decline or echo", async () => {
    const namer = createVendorNamer(
      providerReturning(() => ({ lib_00112233: "lib_00112233" }))
    );
    assert.deepEqual(
      await namer([
        { key: "lib_00112233", evidence: "x" },
        { key: "lib_aabbccdd", evidence: "y" }
      ]),
      [null, null]
    );
  });

  it("contains a provider throw as all-null", async () => {
    const crashing: LLMProvider = {
      async suggestAllNames() {
        throw new Error("box down");
      }
    };
    const namer = createVendorNamer(crashing);
    assert.deepEqual(await namer([{ key: "lib_x", evidence: "e" }]), [null]);
  });
});
