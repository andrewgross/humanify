import {
  getLlama,
  LlamaChatSession,
  LlamaGrammar,
  type LlamaModel,
  type LlamaContext
} from "node-llama-cpp";
import type { LLMContext } from "../analysis/types.js";
import type { LLMProvider, NameSuggestion } from "./types.js";
import { SYSTEM_PROMPT, buildUserPrompt, IDENTIFIER_GRAMMAR } from "./prompts.js";
import { sanitizeIdentifier } from "./validation.js";
import { getModelWrapper } from "../local-models.js";

/**
 * Configuration for the local llama provider.
 */
export interface LocalLlamaConfig {
  /** Path to the GGUF model file */
  modelPath: string;

  /** Model name for wrapper selection (e.g., "2b", "8b") */
  modelName?: string;

  /** Random seed for reproducibility */
  seed?: number;

  /** Disable GPU acceleration */
  disableGpu?: boolean;

  /** Temperature for generation (0-1) */
  temperature?: number;
}

/**
 * LLM provider using local llama.cpp models via node-llama-cpp.
 *
 * Uses grammar constraints to ensure output is a valid identifier.
 */
export class LocalLlamaProvider implements LLMProvider {
  private model: LlamaModel;
  private context: LlamaContext;
  private llama: Awaited<ReturnType<typeof getLlama>>;
  private config: LocalLlamaConfig;

  private constructor(
    llama: Awaited<ReturnType<typeof getLlama>>,
    model: LlamaModel,
    context: LlamaContext,
    config: LocalLlamaConfig
  ) {
    this.llama = llama;
    this.model = model;
    this.context = context;
    this.config = config;
  }

  /**
   * Creates a new LocalLlamaProvider. Must be called as a factory
   * since model loading is async.
   */
  static async create(config: LocalLlamaConfig): Promise<LocalLlamaProvider> {
    const disableGpu = config.disableGpu ?? (process.env["CI"] === "true");
    const llama = await getLlama({ gpu: disableGpu ? false : "auto" });

    const model = await llama.loadModel({
      modelPath: config.modelPath,
      gpuLayers: disableGpu ? 0 : undefined
    });

    const context = await model.createContext();

    return new LocalLlamaProvider(llama, model, context, config);
  }

  async suggestName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    const prompt = buildUserPrompt(currentName, context);

    // Create a new session for each request to avoid context pollution
    const session = new LlamaChatSession({
      contextSequence: this.context.getSequence(),
      autoDisposeSequence: true,
      systemPrompt: SYSTEM_PROMPT,
      chatWrapper: this.config.modelName
        ? getModelWrapper(this.config.modelName)
        : undefined
    });

    try {
      // Use grammar to constrain output to valid identifiers
      const grammar = new LlamaGrammar(this.llama, {
        grammar: IDENTIFIER_GRAMMAR
      });

      const response = await session.promptWithMeta(prompt, {
        temperature: this.config.temperature ?? 0.5,
        grammar,
        stopOnAbortSignal: true
      });

      const name = sanitizeIdentifier(response.responseText.trim());
      return { name };
    } finally {
      session.dispose();
    }
  }

  async suggestFunctionName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    // For local models, use the same method - grammar constrains output
    return this.suggestName(currentName, context);
  }

  /**
   * Disposes of the model resources.
   */
  dispose(): void {
    this.context.dispose();
    this.model.dispose();
  }
}

/**
 * Creates a local llama provider from a model path.
 */
export async function createLocalProvider(
  modelPath: string,
  options: Partial<LocalLlamaConfig> = {}
): Promise<LocalLlamaProvider> {
  return LocalLlamaProvider.create({
    modelPath,
    ...options
  });
}
