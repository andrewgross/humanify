import assert from "node:assert";
import { describe, it } from "node:test";
import { OpenAICompatibleProvider } from "./openai-compatible.js";
import type { BatchRenameRequest } from "./types.js";

const makeRequest = (identifiers: string[]): BatchRenameRequest => ({
  code: "function a(b, c) { return b + c; }",
  identifiers,
  usedNames: new Set(),
  calleeSignatures: [],
  callsites: []
});

/** Replace the provider's private OpenAI client with a canned-response stub. */
function stubClient(
  provider: OpenAICompatibleProvider,
  content: string | null
): void {
  const client = (provider as unknown as { client: Record<string, unknown> })
    .client;
  client.chat = {
    completions: {
      create: async () => ({
        choices: [{ message: { content } }]
      })
    }
  };
}

/** Stub the client and capture the request body passed to completions.create. */
function stubClientCapturing(
  provider: OpenAICompatibleProvider,
  content: string
): { body: Record<string, unknown> | undefined } {
  const captured: { body: Record<string, unknown> | undefined } = {
    body: undefined
  };
  const client = (provider as unknown as { client: Record<string, unknown> })
    .client;
  client.chat = {
    completions: {
      create: async (body: Record<string, unknown>) => {
        captured.body = body;
        return { choices: [{ message: { content } }] };
      }
    }
  };
  return captured;
}

function makeProvider(): OpenAICompatibleProvider {
  return new OpenAICompatibleProvider({
    endpoint: "https://test.api/v1",
    apiKey: "test-key",
    model: "test-model"
  });
}

describe("OpenAICompatibleProvider", () => {
  describe("suggestAllNames", () => {
    it("parses valid JSON response", async () => {
      const provider = makeProvider();
      stubClient(
        provider,
        JSON.stringify({ a: "addNumbers", b: "firstValue" })
      );

      const result = await provider.suggestAllNames(makeRequest(["a", "b"]));

      assert.strictEqual(result.renames.a, "addNumbers");
      assert.strictEqual(result.renames.b, "firstValue");
    });

    it("returns empty renames when no response", async () => {
      const provider = makeProvider();
      stubClient(provider, null);

      const result = await provider.suggestAllNames(makeRequest(["a"]));

      assert.deepStrictEqual(result.renames, {});
    });

    it("extracts renames from malformed JSON", async () => {
      const provider = makeProvider();
      stubClient(
        provider,
        'Here are the names: "a": "calculateSum", "b": "inputValue" — done!'
      );

      const result = await provider.suggestAllNames(makeRequest(["a", "b"]));

      // Falls back to regex extraction of "key": "value" pairs
      assert.strictEqual(result.renames.a, "calculateSum");
      assert.strictEqual(result.renames.b, "inputValue");
    });

    it("passes a raw invalid identifier through unchanged (no longer sanitized)", async () => {
      const provider = makeProvider();
      stubClient(provider, JSON.stringify({ a: "123invalid" }));

      const result = await provider.suggestAllNames(makeRequest(["a"]));

      // The adapter no longer force-sanitizes. The raw name flows through so
      // the batch validator can classify it as invalid and drive a retry.
      assert.strictEqual(result.renames.a, "123invalid");
    });

    it("passes a raw reserved word through unchanged (JSON path)", async () => {
      const provider = makeProvider();
      stubClient(provider, JSON.stringify({ a: "delete" }));

      const result = await provider.suggestAllNames(makeRequest(["a"]));

      // Raw, not "delete_" — sanitization is the processor's last resort, not
      // the adapter's silent default.
      assert.strictEqual(result.renames.a, "delete");
    });

    it("passes a raw reserved/builtin word through unchanged (regex-fallback path)", async () => {
      const provider = makeProvider();
      // Malformed JSON forces the regex extraction fallback.
      stubClient(provider, 'names: "a": "Map", "b": "delete" done');

      const result = await provider.suggestAllNames(makeRequest(["a", "b"]));

      assert.strictEqual(result.renames.a, "Map");
      assert.strictEqual(result.renames.b, "delete");
    });
  });

  describe("configuration", () => {
    it("uses default maxTokens", () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      assert.strictEqual(
        (
          provider as unknown as {
            client: Record<string, unknown>;
            maxTokens: number;
            temperature: number;
          }
        ).maxTokens,
        2000
      );
    });

    it("uses custom maxTokens", () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model",
        maxTokens: 200
      });

      assert.strictEqual(
        (
          provider as unknown as {
            client: Record<string, unknown>;
            maxTokens: number;
            temperature: number;
          }
        ).maxTokens,
        200
      );
    });

    it("defaults to temperature 0 for deterministic naming", () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      assert.strictEqual(
        (
          provider as unknown as {
            client: Record<string, unknown>;
            maxTokens: number;
            temperature: number;
          }
        ).temperature,
        0
      );
    });

    it("uses custom temperature", () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model",
        temperature: 0.7
      });

      assert.strictEqual(
        (
          provider as unknown as {
            client: Record<string, unknown>;
            maxTokens: number;
            temperature: number;
          }
        ).temperature,
        0.7
      );
    });

    it("omits reasoning_effort from the request body by default", async () => {
      const provider = makeProvider();
      const captured = stubClientCapturing(
        provider,
        JSON.stringify({ a: "value" })
      );

      await provider.suggestAllNames(makeRequest(["a"]));

      assert.ok(captured.body, "request body should be captured");
      assert.ok(
        !("reasoning_effort" in captured.body),
        "reasoning_effort must not be sent unless configured"
      );
    });

    it("sends reasoning_effort when configured", async () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model",
        reasoningEffort: "low"
      });
      const captured = stubClientCapturing(
        provider,
        JSON.stringify({ a: "value" })
      );

      await provider.suggestAllNames(makeRequest(["a"]));

      assert.ok(captured.body, "request body should be captured");
      assert.strictEqual(captured.body.reasoning_effort, "low");
    });

    it("sends temperature 0 in the request body by default", async () => {
      const provider = makeProvider();
      const captured = stubClientCapturing(
        provider,
        JSON.stringify({ a: "value" })
      );

      await provider.suggestAllNames(makeRequest(["a"]));

      assert.ok(captured.body, "request body should be captured");
      assert.strictEqual(captured.body.temperature, 0);
    });
  });
});
