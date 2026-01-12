import { showPercentage } from "../../progress.js";
import { defineFilename } from "./define-filename.js";
import { Prompt } from "./llama.js";
import { unminifyVariableName } from "./unminify-variable-name.js";
import { visitAllIdentifiers } from "./visit-all-identifiers.js";

const PADDING_CHARS = 200;

export const localReanme = (
  prompt: Prompt,
  contextWindowSize: number,
  _concurrency?: number // Reserved for future use with new processor
) => {
  return async (code: string): Promise<string> => {
    const filename = await defineFilename(
      prompt,
      code.slice(0, PADDING_CHARS * 2)
    );

    return await visitAllIdentifiers(
      code,
      (name, surroundingCode) =>
        unminifyVariableName(prompt, name, filename, surroundingCode),
      contextWindowSize,
      showPercentage
    );
  };
};
