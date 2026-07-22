/**
 * Disk response cache for LLM rename calls (measurement determinism).
 *
 * Motivation (measured 2026-07-22): identical pipeline runs agree to ~±115
 * noise lines within a serving session, but the SAME code drifts by up to
 * ±2.7k lines across sessions — the local vLLM server's serving state, not
 * our batch order, is the nondeterminism. Caching responses by prompt makes
 * repeated prompts deterministic across sessions, makes eval reruns nearly
 * free, and sharpens A/B probes (shared prompts short-circuit; only
 * genuinely-changed prompts reach the server).
 */
import assert from "node:assert";
import { describe, it } from "node:test";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import type {
  BatchRenameRequest,
  BatchRenameResponse,
  LLMProvider
} from "./types.js";
import { CachedLLMProvider } from "./cached-provider.js";

function tmpDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "llm-cache-test-"));
}

class FakeProvider implements LLMProvider {
  calls = 0;
  response: BatchRenameResponse = {
    renames: { a: "alpha" },
    finishReason: "stop",
    usage: { totalTokens: 15, inputTokens: 10, outputTokens: 5 }
  };
  fail = false;
  async suggestAllNames(): Promise<BatchRenameResponse> {
    this.calls++;
    if (this.fail) throw new Error("server down");
    return structuredClone(this.response);
  }
}

function request(code: string): BatchRenameRequest {
  return {
    code,
    identifiers: ["a"],
    usedNames: new Set(["taken"]),
    calleeSignatures: [],
    callsites: []
  };
}

describe("CachedLLMProvider", () => {
  it("serves identical requests from disk after one inner call", async () => {
    const dir = tmpDir();
    const inner = new FakeProvider();
    const provider = new CachedLLMProvider(inner, dir, {
      model: "m1",
      temperature: 0
    });
    const first = await provider.suggestAllNames(request("function f() {}"));
    const second = await provider.suggestAllNames(request("function f() {}"));
    assert.strictEqual(inner.calls, 1);
    assert.deepStrictEqual(first.renames, { a: "alpha" });
    assert.deepStrictEqual(second.renames, { a: "alpha" });
  });

  it("misses on different prompts and different model params", async () => {
    const dir = tmpDir();
    const inner = new FakeProvider();
    const provider = new CachedLLMProvider(inner, dir, {
      model: "m1",
      temperature: 0
    });
    await provider.suggestAllNames(request("function f() {}"));
    await provider.suggestAllNames(request("function g() {}"));
    assert.strictEqual(inner.calls, 2);
    const other = new CachedLLMProvider(inner, dir, {
      model: "m2",
      temperature: 0
    });
    await other.suggestAllNames(request("function f() {}"));
    assert.strictEqual(inner.calls, 3);
  });

  it("persists across provider instances (cross-session determinism)", async () => {
    const dir = tmpDir();
    const inner = new FakeProvider();
    const first = new CachedLLMProvider(inner, dir, {
      model: "m1",
      temperature: 0
    });
    await first.suggestAllNames(request("function f() {}"));
    const inner2 = new FakeProvider();
    inner2.response = {
      renames: { a: "DIFFERENT" },
      finishReason: "stop"
    };
    const second = new CachedLLMProvider(inner2, dir, {
      model: "m1",
      temperature: 0
    });
    const result = await second.suggestAllNames(request("function f() {}"));
    assert.strictEqual(inner2.calls, 0, "must not reach the server");
    assert.deepStrictEqual(result.renames, { a: "alpha" });
  });

  it("zeroes usage on hits and reports hit/miss counters", async () => {
    const dir = tmpDir();
    const inner = new FakeProvider();
    const provider = new CachedLLMProvider(inner, dir, {
      model: "m1",
      temperature: 0
    });
    await provider.suggestAllNames(request("function f() {}"));
    const hit = await provider.suggestAllNames(request("function f() {}"));
    assert.strictEqual(hit.usage?.totalTokens ?? 0, 0);
    assert.strictEqual(provider.stats.hits, 1);
    assert.strictEqual(provider.stats.misses, 1);
  });

  it("does not cache empty responses or errors", async () => {
    const dir = tmpDir();
    const inner = new FakeProvider();
    inner.response = { renames: {}, finishReason: "length" };
    const provider = new CachedLLMProvider(inner, dir, {
      model: "m1",
      temperature: 0
    });
    await provider.suggestAllNames(request("function f() {}"));
    await provider.suggestAllNames(request("function f() {}"));
    assert.strictEqual(inner.calls, 2, "empty responses are not cached");
    inner.fail = true;
    await assert.rejects(() =>
      provider.suggestAllNames(request("function h() {}"))
    );
    inner.fail = false;
    inner.response = {
      renames: { a: "beta" },
      finishReason: "stop"
    };
    const after = await provider.suggestAllNames(request("function h() {}"));
    assert.deepStrictEqual(after.renames, { a: "beta" });
  });
});
