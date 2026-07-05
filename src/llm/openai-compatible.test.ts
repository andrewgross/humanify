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

    it("sanitizes invalid identifiers in response", async () => {
      const provider = makeProvider();
      stubClient(provider, JSON.stringify({ a: "123invalid" }));

      const result = await provider.suggestAllNames(makeRequest(["a"]));

      // sanitizeIdentifier should prefix with _
      assert.strictEqual(result.renames.a, "_123invalid");
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

    it("uses default temperature", () => {
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
        0.3
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
  });
});
