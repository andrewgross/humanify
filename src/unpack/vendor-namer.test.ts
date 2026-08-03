import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { CjsFactoryRecord } from "../analysis/bun-module-classification.js";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import {
  type VendorNamer,
  acceptVendorName,
  buildVendorEvidence,
  createVendorNamer,
  nameFallbackFactoriesWithLlm
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

describe("nameFallbackFactoriesWithLlm", () => {
  function record(hash: string): CjsFactoryRecord {
    return {
      factoryVar: hash,
      byteRange: [0, 0],
      structuralHash: hash,
      name: `lib_${hash.slice(0, 8)}`,
      nameSource: "fallback"
    } as unknown as CjsFactoryRecord;
  }

  it("reverts a name the model over-applied to too many modules (hallucination guard)", async () => {
    // The real 2.1.89 run named 100 distinct modules "is-plain-object" —
    // a default the model reaches for on tiny utility modules. A name
    // applied to more than the cap is unreliable; those keep lib_<hash>.
    const many = Array.from({ length: 40 }, (_, i) =>
      record(`${i.toString(16).padStart(16, "0")}`)
    );
    const namer: VendorNamer = async (reqs) =>
      reqs.map(() => "is-plain-object");
    const renamed = await nameFallbackFactoriesWithLlm(
      many,
      "x".repeat(100),
      namer,
      100,
      10 // cap: names applied to >10 factories are hallucinations
    );
    assert.strictEqual(renamed, 0, "an over-applied name is fully reverted");
    for (const r of many) {
      assert.strictEqual(r.nameSource, "fallback");
      assert.match(r.name ?? "", /^lib_/);
    }
  });

  it("keeps a name applied within the cap (a real package with many modules)", async () => {
    const some = Array.from({ length: 8 }, (_, i) =>
      record(`${i.toString(16).padStart(16, "0")}`)
    );
    const namer: VendorNamer = async (reqs) => reqs.map(() => "protobufjs");
    const renamed = await nameFallbackFactoriesWithLlm(
      some,
      "x".repeat(100),
      namer,
      100,
      10
    );
    assert.strictEqual(renamed, 8);
    for (const r of some) {
      assert.strictEqual(r.nameSource, "llm");
      assert.strictEqual(r.name, "protobufjs");
    }
  });
});

/**
 * Vendor naming has THREE distinct failure modes and they were all
 * indistinguishable — every one returned `null`:
 *
 *   1. the model returned nothing for that key (declined);
 *   2. it echoed the key back unchanged, which is a non-answer;
 *   3. the provider threw, killing the whole batch.
 *
 * Only (3) was logged, and only as a batch count. So no run could answer "how
 * many vendor files went unnamed, and why?" — on the surface that went
 * UNSCORED for thirteen experiments and carries 2.4x the entire measured src/
 * noise (measurement-pitfalls rule 8). "Vendor names look bad" and "the model
 * declined half of them" are different problems with different fixes, and
 * nothing on disk could tell them apart.
 */
describe("vendor naming records WHY a name was not produced", () => {
  it("separates named, declined, echoed and thrown", async () => {
    const stats = { named: 0, declined: 0, echoed: 0, batchesFailed: 0 };
    const namer = createVendorNamer(
      providerReturning((req) => {
        const out: Record<string, string> = {};
        for (const id of req.identifiers) {
          if (id === "a")
            out[id] = "lodashClone"; // named
          else if (id === "b") out[id] = id; // echoed the key back
          // "c" omitted entirely -> declined
        }
        return out;
      }),
      stats
    );
    const result = await namer([
      { key: "a", evidence: "" },
      { key: "b", evidence: "" },
      { key: "c", evidence: "" }
    ]);
    assert.deepEqual(result, ["lodashClone", null, null]);
    assert.equal(stats.named, 1);
    assert.equal(stats.echoed, 1, "an echoed key is a non-answer, not a name");
    assert.equal(stats.declined, 1);
    assert.equal(stats.batchesFailed, 0);
  });

  it("counts a thrown batch as its own outcome, not as declines", async () => {
    // A provider outage and a model that will not name anything produce the
    // same nulls but mean opposite things: one is an infrastructure problem,
    // the other is a prompt problem.
    const stats = { named: 0, declined: 0, echoed: 0, batchesFailed: 0 };
    const namer = createVendorNamer(
      {
        async suggestAllNames() {
          throw new Error("provider down");
        }
      } as LLMProvider,
      stats
    );
    const result = await namer([
      { key: "a", evidence: "" },
      { key: "b", evidence: "" }
    ]);
    assert.deepEqual(result, [null, null]);
    assert.equal(stats.batchesFailed, 1);
    assert.equal(stats.declined, 0, "a thrown batch is not a decline");
    assert.equal(stats.named, 0);
  });

  it("works without a stats sink — instrumentation must stay optional", async () => {
    const namer: VendorNamer = createVendorNamer(
      providerReturning(() => ({ a: "named" }))
    );
    assert.deepEqual(await namer([{ key: "a", evidence: "" }]), ["named"]);
  });
});
