import { describe, it, mock, beforeEach } from "node:test";
import assert from "node:assert";
import {
  OpenAICompatibleProvider
} from "./openai-compatible.js";
import type { LLMContext } from "../analysis/types.js";

const makeContext = (): LLMContext => ({
  functionCode: "function a(b, c) { return b + c; }",
  calleeSignatures: [],
  callsites: [],
  usedIdentifiers: new Set()
});

// Mock the OpenAI module
const mockCreate = mock.fn();

// We'll test the provider's behavior with mocked responses
describe("OpenAICompatibleProvider", () => {
  describe("suggestName", () => {
    it("parses valid JSON response", async () => {
      // Create a provider with a mock client
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      // Access private client to mock it
      const client = (provider as any).client;
      client.chat = {
        completions: {
          create: async () => ({
            choices: [
              {
                message: {
                  content: JSON.stringify({
                    name: "addNumbers",
                    reasoning: "Adds two numbers together"
                  })
                }
              }
            ]
          })
        }
      };

      const result = await provider.suggestName("a", makeContext());

      assert.strictEqual(result.name, "addNumbers");
      assert.strictEqual(result.reasoning, "Adds two numbers together");
    });

    it("returns original name when no response", async () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      const client = (provider as any).client;
      client.chat = {
        completions: {
          create: async () => ({
            choices: [{ message: { content: null } }]
          })
        }
      };

      const result = await provider.suggestName("originalName", makeContext());

      assert.strictEqual(result.name, "originalName");
      assert.ok(result.reasoning?.includes("No response"));
    });

    it("extracts identifier from malformed JSON", async () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      const client = (provider as any).client;
      client.chat = {
        completions: {
          create: async () => ({
            choices: [
              {
                message: {
                  content: "I think the name should be calculateSum"
                }
              }
            ]
          })
        }
      };

      const result = await provider.suggestName("a", makeContext());

      // Should extract first valid identifier from response
      assert.strictEqual(result.name, "I"); // "I" is extracted as first identifier
      assert.ok(result.reasoning?.includes("Failed to parse"));
    });

    it("sanitizes invalid identifiers in response", async () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      const client = (provider as any).client;
      client.chat = {
        completions: {
          create: async () => ({
            choices: [
              {
                message: {
                  content: JSON.stringify({
                    name: "123invalid",
                    reasoning: "Test"
                  })
                }
              }
            ]
          })
        }
      };

      const result = await provider.suggestName("a", makeContext());

      // sanitizeIdentifier should prefix with _
      assert.strictEqual(result.name, "_123invalid");
    });

    it("includes confidence when provided", async () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      const client = (provider as any).client;
      client.chat = {
        completions: {
          create: async () => ({
            choices: [
              {
                message: {
                  content: JSON.stringify({
                    name: "sum",
                    reasoning: "Adds values",
                    confidence: 0.95
                  })
                }
              }
            ]
          })
        }
      };

      const result = await provider.suggestName("a", makeContext());

      assert.strictEqual(result.confidence, 0.95);
    });
  });

  describe("suggestFunctionName", () => {
    it("uses function-specific prompt", async () => {
      let capturedMessages: any[] = [];

      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      const client = (provider as any).client;
      client.chat = {
        completions: {
          create: async (params: any) => {
            capturedMessages = params.messages;
            return {
              choices: [
                {
                  message: {
                    content: JSON.stringify({
                      name: "calculateTotal",
                      reasoning: "Calculates total from inputs"
                    })
                  }
                }
              ]
            };
          }
        }
      };

      await provider.suggestFunctionName("fn", makeContext());

      // System prompt should mention functions
      assert.ok(
        capturedMessages[0].content.includes("function"),
        "Should use function-specific system prompt"
      );
    });
  });

  describe("suggestNames (batch)", () => {
    it("processes requests sequentially", async () => {
      const callOrder: string[] = [];

      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      const client = (provider as any).client;
      client.chat = {
        completions: {
          create: async () => {
            // Track call
            callOrder.push("called");
            return {
              choices: [
                {
                  message: {
                    content: JSON.stringify({
                      name: "result",
                      reasoning: "Test"
                    })
                  }
                }
              ]
            };
          }
        }
      };

      const results = await provider.suggestNames([
        { name: "a", context: makeContext() },
        { name: "b", context: makeContext() },
        { name: "c", context: makeContext() }
      ]);

      assert.strictEqual(results.length, 3);
      assert.strictEqual(callOrder.length, 3);
    });
  });

  describe("configuration", () => {
    it("uses default maxTokens", () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      assert.strictEqual((provider as any).maxTokens, 2000);
    });

    it("uses custom maxTokens", () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model",
        maxTokens: 200
      });

      assert.strictEqual((provider as any).maxTokens, 200);
    });

    it("uses default temperature", () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model"
      });

      assert.strictEqual((provider as any).temperature, 0.3);
    });

    it("uses custom temperature", () => {
      const provider = new OpenAICompatibleProvider({
        endpoint: "https://test.api/v1",
        apiKey: "test-key",
        model: "test-model",
        temperature: 0.7
      });

      assert.strictEqual((provider as any).temperature, 0.7);
    });
  });
});

