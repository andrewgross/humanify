/**
 * LLM provider for Google Gemini API.
 */

import {
  GoogleGenerativeAI,
  SchemaType
} from "@google/generative-ai";
import type { LLMContext } from "../analysis/types.js";
import type { LLMProvider, NameSuggestion } from "./types.js";
import { SYSTEM_PROMPT, buildUserPrompt, buildRetryPrompt } from "./prompts.js";
import { sanitizeIdentifier } from "./validation.js";

/**
 * Configuration for the Gemini provider.
 */
export interface GeminiConfig {
  /** Google AI API key */
  apiKey: string;

  /** Model name (default: gemini-1.5-flash) */
  model?: string;

  /** Temperature for generation (0-1) */
  temperature?: number;
}

/**
 * LLM provider for Google Gemini API.
 */
export class GeminiProvider implements LLMProvider {
  private client: GoogleGenerativeAI;
  private modelName: string;
  private temperature: number;

  constructor(config: GeminiConfig) {
    this.client = new GoogleGenerativeAI(config.apiKey);
    this.modelName = config.model ?? "gemini-1.5-flash";
    this.temperature = config.temperature ?? 0.3;
  }

  async suggestName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    const model = this.client.getGenerativeModel({
      model: this.modelName,
      systemInstruction: SYSTEM_PROMPT,
      generationConfig: {
        temperature: this.temperature,
        responseMimeType: "application/json",
        responseSchema: {
          type: SchemaType.OBJECT,
          nullable: false,
          properties: {
            name: {
              type: SchemaType.STRING,
              nullable: false,
              description: "The suggested name for the identifier"
            },
            reasoning: {
              type: SchemaType.STRING,
              nullable: true,
              description: "Brief explanation of the naming choice"
            }
          },
          required: ["name"]
        }
      }
    });

    const prompt = buildUserPrompt(currentName, context);
    const result = await model.generateContent(prompt);
    const text = result.response.text();

    try {
      const parsed = JSON.parse(text);
      return {
        name: sanitizeIdentifier(parsed.name || currentName),
        reasoning: parsed.reasoning
      };
    } catch {
      // If JSON parsing fails, try to extract identifier from raw response
      const match = text.match(/[a-zA-Z_$][a-zA-Z0-9_$]*/);
      return {
        name: match ? sanitizeIdentifier(match[0]) : currentName,
        reasoning: "Failed to parse JSON response"
      };
    }
  }

  async suggestFunctionName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    // Use the same method - Gemini handles both well
    return this.suggestName(currentName, context);
  }

  async retrySuggestName(
    currentName: string,
    rejectedName: string,
    reason: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    const model = this.client.getGenerativeModel({
      model: this.modelName,
      systemInstruction: SYSTEM_PROMPT,
      generationConfig: {
        temperature: this.temperature,
        responseMimeType: "application/json",
        responseSchema: {
          type: SchemaType.OBJECT,
          nullable: false,
          properties: {
            name: {
              type: SchemaType.STRING,
              nullable: false,
              description: "The suggested name for the identifier"
            },
            reasoning: {
              type: SchemaType.STRING,
              nullable: true,
              description: "Brief explanation of the naming choice"
            }
          },
          required: ["name"]
        }
      }
    });

    const prompt = buildRetryPrompt(currentName, rejectedName, context, reason);
    const result = await model.generateContent(prompt);
    const text = result.response.text();

    try {
      const parsed = JSON.parse(text);
      return {
        name: sanitizeIdentifier(parsed.name || currentName),
        reasoning: parsed.reasoning
      };
    } catch {
      const match = text.match(/[a-zA-Z_$][a-zA-Z0-9_$]*/);
      return {
        name: match ? sanitizeIdentifier(match[0]) : currentName,
        reasoning: "Failed to parse JSON response on retry"
      };
    }
  }
}

/**
 * Creates a Gemini provider.
 */
export function createGeminiProvider(
  apiKey: string,
  model = "gemini-1.5-flash",
  options: Partial<GeminiConfig> = {}
): GeminiProvider {
  return new GeminiProvider({
    apiKey,
    model,
    ...options
  });
}
