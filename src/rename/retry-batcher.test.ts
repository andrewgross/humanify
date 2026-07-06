import assert from "node:assert";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import { RetryBatcher } from "./retry-batcher.js";

function makeRequest(
  identifiers: string[],
  overrides: Partial<BatchRenameRequest> = {}
): BatchRenameRequest {
  return {
    code: `function f(${identifiers.join(", ")}) {}`,
    identifiers,
    usedNames: new Set(["taken"]),
    calleeSignatures: [],
    callsites: [],
    isRetry: true,
    promptBody: `retry context for ${identifiers.join(", ")}`,
    ...overrides
  };
}

/** LLM stub that renames every identifier `x` to `x_new` and records calls. */
function makeStubLLM(delayMs = 0): {
  llm: LLMProvider;
  calls: BatchRenameRequest[];
} {
  const calls: BatchRenameRequest[] = [];
  const llm: LLMProvider = {
    async suggestAllNames(request) {
      calls.push(request);
      if (delayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
      const renames: Record<string, string> = {};
      for (const id of request.identifiers) {
        renames[id] = `${id}_new`;
      }
      return { renames, finishReason: "stop" };
    }
  };
  return { llm, calls };
}

describe("RetryBatcher", () => {
  it("merges concurrent submissions from different groups into one call", async () => {
    const { llm, calls } = makeStubLLM();
    const batcher = new RetryBatcher(llm, undefined, { windowMs: 5 });

    const [a, b] = await Promise.all([
      batcher.submit(makeRequest(["e"])),
      batcher.submit(makeRequest(["t", "n"]))
    ]);

    assert.strictEqual(calls.length, 1, "Both groups share one LLM call");
    assert.deepStrictEqual(
      [...calls[0].identifiers].sort(),
      ["e", "n", "t"],
      "Merged call carries the union of identifiers"
    );
    assert.deepStrictEqual(a.renames, { e: "e_new" });
    assert.deepStrictEqual(b.renames, { t: "t_new", n: "n_new" });
  });

  it("includes every group's prompt body in the merged prompt", async () => {
    const { llm, calls } = makeStubLLM();
    const batcher = new RetryBatcher(llm, undefined, { windowMs: 5 });

    await Promise.all([
      batcher.submit(makeRequest(["e"], { promptBody: "BODY-ONE" })),
      batcher.submit(makeRequest(["t"], { promptBody: "BODY-TWO" }))
    ]);

    assert.strictEqual(calls.length, 1);
    const prompt = calls[0].userPrompt ?? "";
    assert.ok(prompt.includes("BODY-ONE"), "First group body present");
    assert.ok(prompt.includes("BODY-TWO"), "Second group body present");
  });

  it("sends a lone submission through unchanged", async () => {
    const { llm, calls } = makeStubLLM();
    const batcher = new RetryBatcher(llm, undefined, { windowMs: 5 });

    const request = makeRequest(["e"]);
    const result = await batcher.submit(request);

    assert.strictEqual(calls.length, 1);
    assert.strictEqual(
      calls[0],
      request,
      "Single entry passes its request through untouched"
    );
    assert.deepStrictEqual(result.renames, { e: "e_new" });
  });

  it("keeps groups with overlapping identifier names in separate calls", async () => {
    const { llm, calls } = makeStubLLM();
    const batcher = new RetryBatcher(llm, undefined, { windowMs: 5 });

    await Promise.all([
      batcher.submit(makeRequest(["e", "t"])),
      batcher.submit(makeRequest(["e"]))
    ]);

    assert.strictEqual(
      calls.length,
      2,
      "Same minified name in two scopes cannot share a flat JSON response"
    );
  });

  it("keeps different system-prompt pools in separate calls", async () => {
    const { llm, calls } = makeStubLLM();
    const batcher = new RetryBatcher(llm, undefined, { windowMs: 5 });

    await Promise.all([
      batcher.submit(makeRequest(["e"])),
      batcher.submit(makeRequest(["t"], { systemPrompt: "MODULE PROMPT" }))
    ]);

    assert.strictEqual(calls.length, 2, "Pools must not mix system prompts");
  });

  it("dispatches without merging when an entry has no promptBody", async () => {
    const { llm, calls } = makeStubLLM();
    const batcher = new RetryBatcher(llm, undefined, { windowMs: 5 });

    await Promise.all([
      batcher.submit(makeRequest(["e"], { promptBody: undefined })),
      batcher.submit(makeRequest(["t"], { promptBody: undefined }))
    ]);

    assert.strictEqual(
      calls.length,
      2,
      "Entries without a prompt body cannot merge"
    );
  });

  it("flushes immediately when the identifier budget fills", async () => {
    const { llm, calls } = makeStubLLM();
    // windowMs high enough that a timer-based flush would time out the test
    const batcher = new RetryBatcher(llm, undefined, {
      windowMs: 60_000,
      maxBatch: 2
    });

    const results = await Promise.all([
      batcher.submit(makeRequest(["e"])),
      batcher.submit(makeRequest(["t"]))
    ]);

    assert.strictEqual(calls.length, 1);
    assert.strictEqual(results.length, 2);
  });

  it("rejects all merged entries when the LLM call throws", async () => {
    const llm: LLMProvider = {
      async suggestAllNames() {
        throw new Error("boom");
      }
    };
    const batcher = new RetryBatcher(llm, undefined, { windowMs: 5 });

    const results = await Promise.allSettled([
      batcher.submit(makeRequest(["e"])),
      batcher.submit(makeRequest(["t"]))
    ]);

    assert.ok(results.every((r) => r.status === "rejected"));
  });

  it("records one metrics call per actual LLM call", async () => {
    const { llm } = makeStubLLM();
    let starts = 0;
    let dones = 0;
    let tokenRecords = 0;
    const metrics = {
      llmCallStart() {
        starts++;
        return () => {
          dones++;
        };
      },
      recordTokens() {
        tokenRecords++;
      }
    };
    const batcher = new RetryBatcher(llm, metrics, { windowMs: 5 });

    await Promise.all([
      batcher.submit(makeRequest(["e"])),
      batcher.submit(makeRequest(["t"]))
    ]);

    assert.strictEqual(starts, 1, "One merged call → one llmCallStart");
    assert.strictEqual(dones, 1, "Completion callback fired once");
    assert.strictEqual(tokenRecords, 1, "Tokens recorded once");
  });
});
