import type { LLMContext } from "../analysis/types.js";

/**
 * System prompt for identifier renaming.
 * Provides guidelines for the LLM on naming conventions.
 */
export const SYSTEM_PROMPT = `You are an expert JavaScript developer helping to deobfuscate minified code.

Your task is to suggest meaningful, descriptive names for minified identifiers based on their usage context.

Guidelines:
- Use camelCase for variables and functions
- Use PascalCase for classes and constructors
- Be specific but concise (e.g., "userId" not "theIdOfTheUser")
- Consider the function's callees - if it calls "fetchUser", it might be "loadUserProfile"
- Avoid generic names like "data", "result", "temp" unless truly appropriate
- Never use reserved words (if, for, class, etc.)

Respond with JSON: { "name": "suggestedName", "reasoning": "brief explanation" }`;

/**
 * System prompt specifically for function naming.
 */
export const FUNCTION_NAME_SYSTEM_PROMPT = `You are an expert JavaScript developer helping to deobfuscate minified code.

Your task is to suggest meaningful, descriptive names for minified functions based on their implementation and usage.

Guidelines:
- Use camelCase for regular functions
- Use PascalCase for classes and constructors (look for 'this' usage, prototype, new calls)
- Start with a verb for action functions (get, set, fetch, handle, process, etc.)
- Be specific: "fetchUserProfile" is better than "getData"
- Consider what the function does internally - if it calls "fetch", it likely fetches something
- Consider how it's called - call site names often hint at purpose
- Never use reserved words

Respond with JSON: { "name": "suggestedName", "reasoning": "brief explanation" }`;

/**
 * Builds a user prompt for the LLM based on context.
 */
export function buildUserPrompt(
  currentName: string,
  context: LLMContext
): string {
  let prompt = `Suggest a better name for the identifier "${currentName}" in this code:\n\n`;

  prompt += "```javascript\n" + context.functionCode + "\n```\n\n";

  if (context.calleeSignatures.length > 0) {
    prompt += "This function calls these (already named) functions:\n";
    for (const callee of context.calleeSignatures) {
      prompt += `- ${callee.name}(${callee.params.join(", ")})\n`;
    }
    prompt += "\n";
  }

  if (context.callsites.length > 0) {
    prompt += "This function is called like:\n";
    for (const site of context.callsites.slice(0, 3)) {
      prompt += `- ${site}\n`;
    }
    prompt += "\n";
  }

  const usedList = [...context.usedIdentifiers].slice(0, 50).join(", ");
  if (usedList) {
    prompt += `Names already in use (avoid these): ${usedList}\n`;
  }

  return prompt;
}

/**
 * Builds a user prompt specifically for function naming.
 */
export function buildFunctionNamePrompt(
  currentName: string,
  context: LLMContext
): string {
  let prompt = `Suggest a better name for the function "${currentName}":\n\n`;

  prompt += "```javascript\n" + context.functionCode + "\n```\n\n";

  if (context.calleeSignatures.length > 0) {
    prompt += "This function calls:\n";
    for (const callee of context.calleeSignatures) {
      prompt += `- ${callee.name}(${callee.params.join(", ")})\n`;
      if (callee.snippet) {
        prompt += `  // ${callee.snippet.split("\n")[0]}\n`;
      }
    }
    prompt += "\n";
  }

  if (context.callsites.length > 0) {
    prompt += "This function is called as:\n";
    for (const site of context.callsites.slice(0, 5)) {
      prompt += `- ${site}\n`;
    }
    prompt += "\n";
  }

  const usedList = [...context.usedIdentifiers].slice(0, 50).join(", ");
  if (usedList) {
    prompt += `Names already in use (avoid these): ${usedList}\n`;
  }

  return prompt;
}

/**
 * Builds a retry prompt when the LLM's previous suggestion was rejected.
 */
export function buildRetryPrompt(
  currentName: string,
  rejectedName: string,
  context: LLMContext,
  reason: string
): string {
  let prompt = `Your previous suggestion "${rejectedName}" cannot be used: ${reason}\n\n`;
  prompt += `Please suggest a DIFFERENT name for the identifier "${currentName}" in this code:\n\n`;
  prompt += "```javascript\n" + context.functionCode + "\n```\n\n";

  const usedList = [...context.usedIdentifiers].slice(0, 50).join(", ");
  prompt += `Names already in use (you MUST avoid ALL of these): ${usedList}\n`;

  return prompt;
}

/**
 * Builds a retry prompt for function naming when the previous suggestion was rejected.
 */
export function buildFunctionRetryPrompt(
  currentName: string,
  rejectedName: string,
  context: LLMContext,
  reason: string
): string {
  let prompt = `Your previous suggestion "${rejectedName}" cannot be used: ${reason}\n\n`;
  prompt += `Please suggest a DIFFERENT name for the function "${currentName}":\n\n`;
  prompt += "```javascript\n" + context.functionCode + "\n```\n\n";

  const usedList = [...context.usedIdentifiers].slice(0, 50).join(", ");
  prompt += `Names already in use (you MUST avoid ALL of these): ${usedList}\n`;

  return prompt;
}

/**
 * GBNF grammar to constrain output to valid JavaScript identifiers.
 * Used with local llama.cpp models.
 */
export const IDENTIFIER_GRAMMAR = `root ::= [a-zA-Z_$] [a-zA-Z0-9_$]{1,30}`;

/**
 * GBNF grammar for JSON response with name field.
 */
export const JSON_NAME_GRAMMAR = `
root ::= "{" ws "\"name\":" ws "\"" identifier "\"" ws "}"
ws ::= [ \t\n]*
identifier ::= [a-zA-Z_$] [a-zA-Z0-9_$]{0,30}
`;

/**
 * GBNF grammar for JSON response with name and reasoning fields.
 */
export const JSON_NAME_REASONING_GRAMMAR = `
root ::= "{" ws "\"name\":" ws "\"" identifier "\"" ws "," ws "\"reasoning\":" ws "\"" [^"]* "\"" ws "}"
ws ::= [ \t\n]*
identifier ::= [a-zA-Z_$] [a-zA-Z0-9_$]{0,30}
`;
