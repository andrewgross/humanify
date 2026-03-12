import type { LLMContext } from "../analysis/types.js";
import type { LooksMinifiedFn } from "../rename/minified-heuristic.js";
import { looksMinified as defaultLooksMinified } from "../rename/minified-heuristic.js";

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

  prompt += `\`\`\`javascript\n${context.functionCode}\n\`\`\`\n\n`;

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

  prompt += `\`\`\`javascript\n${context.functionCode}\n\`\`\`\n\n`;

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
  prompt += `\`\`\`javascript\n${context.functionCode}\n\`\`\`\n\n`;

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
  prompt += `\`\`\`javascript\n${context.functionCode}\n\`\`\`\n\n`;

  const usedList = [...context.usedIdentifiers].slice(0, 50).join(", ");
  prompt += `Names already in use (you MUST avoid ALL of these): ${usedList}\n`;

  return prompt;
}

/**
 * System prompt for batch renaming all identifiers in a function at once.
 */
export const BATCH_RENAME_SYSTEM_PROMPT = `You are an expert JavaScript developer helping to deobfuscate minified code.

Your task is to analyze a minified function and suggest meaningful, descriptive names for ALL identifiers at once.

CRITICAL RULES:
- You MUST provide a mapping for EVERY identifier listed. Do not skip any.
- All suggested names MUST be unique — no two identifiers can map to the same name.
- Respond with ONLY a JSON object. No explanation, no markdown, just the JSON.

Naming Guidelines:
- First understand what the function DOES semantically
- Name the function based on its PURPOSE (e.g., "splitStringIntoChunks" not "processData")
- Name variables based on what they REPRESENT (e.g., "chunkSize" not "tVal")
- Use camelCase for variables and functions
- Use PascalCase for classes/constructors (look for 'this' usage, 'new' calls)
- Start function names with verbs (get, set, fetch, create, handle, process, etc.)
- Name loop counters meaningfully when possible (index, i, j are OK for simple loops)`;

/**
 * Builds the user prompt for batch renaming.
 */
export function buildBatchRenamePrompt(
  code: string,
  identifiers: string[],
  usedNames: Set<string>,
  calleeSignatures: Array<{ name: string; params: string[] }>,
  callsites: string[],
  contextVars?: string[]
): string {
  let prompt = `Analyze this function and suggest descriptive names for ALL listed identifiers:\n\n`;

  prompt += `\`\`\`javascript\n${code}\n\`\`\`\n\n`;

  prompt += `Identifiers to rename: ${identifiers.join(", ")}\n\n`;

  if (contextVars && contextVars.length > 0) {
    prompt +=
      "Surrounding scope variables (for context only, do NOT rename these):\n";
    for (const v of contextVars) {
      prompt += `  ${v}\n`;
    }
    prompt += "\n";
  }

  if (calleeSignatures.length > 0) {
    prompt += "This function calls:\n";
    for (const callee of calleeSignatures) {
      prompt += `- ${callee.name}(${callee.params.join(", ")})\n`;
    }
    prompt += "\n";
  }

  if (callsites.length > 0) {
    prompt += "This function is called as:\n";
    for (const site of callsites.slice(0, 3)) {
      prompt += `- ${site}\n`;
    }
    prompt += "\n";
  }

  const usedList = [...usedNames].slice(0, 50);
  if (usedList.length > 0) {
    prompt += `Names already in use (MUST avoid these): ${usedList.join(", ")}\n\n`;
  }

  prompt += `You MUST respond with a JSON object containing exactly ${identifiers.length} mappings — one for each identifier listed above:\n`;
  prompt += `{ ${identifiers.map((id) => `"${id}": "descriptiveName"`).join(", ")} }`;

  return prompt;
}

/**
 * Builds a retry prompt for batch renaming when some identifiers failed.
 */
export function buildBatchRenameRetryPrompt(
  code: string,
  identifiers: string[],
  usedNames: Set<string>,
  previousAttempt: Record<string, string>,
  failures: {
    duplicates: string[];
    invalid: string[];
    missing: string[];
    unchanged: string[];
  }
): string {
  let prompt = `Your previous rename suggestions had issues:\n`;

  for (const name of failures.duplicates) {
    const suggested = previousAttempt[name];
    if (suggested) {
      prompt += `- "${name}" was suggested as "${suggested}" but that conflicts with an existing name\n`;
    } else {
      prompt += `- "${name}" had a duplicate/conflicting name\n`;
    }
  }
  for (const name of failures.unchanged) {
    prompt += `- "${name}" was returned as itself — you MUST suggest a DIFFERENT name\n`;
  }
  for (const name of failures.invalid) {
    const suggested = previousAttempt[name];
    if (suggested) {
      prompt += `- "${name}" was suggested as "${suggested}" which is not a valid JavaScript identifier\n`;
    } else {
      prompt += `- "${name}" had an invalid suggested name\n`;
    }
  }
  if (failures.missing.length > 0) {
    prompt += `- These identifiers were MISSING from your response: ${failures.missing.join(", ")}\n`;
  }

  // Collect rejected names to explicitly forbid
  const rejectedNames = new Set<string>();
  for (const name of [
    ...failures.duplicates,
    ...failures.unchanged,
    ...failures.invalid
  ]) {
    const suggested = previousAttempt[name];
    if (suggested) rejectedNames.add(suggested);
  }
  if (rejectedNames.size > 0) {
    prompt += `\nDO NOT suggest these names: ${[...rejectedNames].join(", ")}\n`;
  }

  prompt += `\nPlease suggest DIFFERENT names for these remaining identifiers:\n\n`;

  prompt += `\`\`\`javascript\n${code}\n\`\`\`\n\n`;

  prompt += `Identifiers still needing names: ${identifiers.join(", ")}\n\n`;

  const usedList = [...usedNames].slice(0, 50);
  prompt += `Names already in use (MUST avoid ALL of these): ${usedList.join(", ")}\n\n`;

  prompt += `Respond with JSON mapping each identifier to a UNIQUE name:\n`;
  prompt += `{ ${identifiers.map((id) => `"${id}": "descriptiveName"`).join(", ")} }`;

  return prompt;
}

/**
 * System prompt for module-level identifier renaming.
 */
export const MODULE_LEVEL_RENAME_SYSTEM_PROMPT = `You are an expert JavaScript developer helping to deobfuscate minified code.

Your task is to analyze top-level module declarations and suggest meaningful, descriptive names for minified identifiers.

Guidelines:
- For imports: use context from the module path (e.g., import { webcrypto as a } → webcrypto is already a good hint)
- For constants: use UPPER_SNAKE_CASE if the value is a true constant (literal number/string), camelCase otherwise
- For variables (let): use camelCase based on how they're used
- Be specific but concise
- Every identifier in the list MUST have a mapping
- All suggested names MUST be unique (no duplicates)
- Never use reserved words (if, for, class, etc.)

Respond with ONLY a JSON object mapping each original name to a descriptive name.`;

function buildDeclarationLookup(
  declarations: string[],
  identifiers: string[]
): Map<string, string[]> {
  const declByIdentifier = new Map<string, string[]>();
  for (const decl of declarations) {
    for (const id of identifiers) {
      if (decl.includes(id)) {
        if (!declByIdentifier.has(id)) declByIdentifier.set(id, []);
        declByIdentifier.get(id)?.push(decl);
      }
    }
  }
  return declByIdentifier;
}

function buildIdentifierProfile(
  id: string,
  declByIdentifier: Map<string, string[]>,
  assignmentContext: Record<string, string[]>,
  usageExamples: Record<string, string[]>
): string {
  let section = `Identifier: ${id}\n`;

  const decls = declByIdentifier.get(id);
  if (decls && decls.length > 0) {
    section += `  Declaration: ${decls[0]}\n`;
  }

  const assignments = assignmentContext[id];
  if (assignments && assignments.length > 0) {
    section += `  Assignments:\n`;
    for (const a of assignments) {
      const indented = a
        .split("\n")
        .map((line) => `    ${line}`)
        .join("\n");
      section += `${indented}\n`;
    }
  }

  const usages = usageExamples[id];
  if (usages && usages.length > 0) {
    section += `  Usage:\n`;
    for (const u of usages) {
      const indented = u
        .split("\n")
        .map((line) => `    ${line}`)
        .join("\n");
      section += `${indented}\n`;
    }
  }

  return section;
}

/**
 * Builds the user prompt for module-level batch renaming.
 * Each identifier is presented as a mini profile with declaration, assignments, and usage.
 */
export function buildModuleLevelRenamePrompt(
  declarations: string[],
  assignmentContext: Record<string, string[]>,
  usageExamples: Record<string, string[]>,
  identifiers: string[],
  usedNames: Set<string>,
  looksMinified?: LooksMinifiedFn
): string {
  let prompt = `Analyze these top-level module identifiers and suggest descriptive names.\n\n`;

  const declByIdentifier = buildDeclarationLookup(declarations, identifiers);

  for (const id of identifiers) {
    prompt += buildIdentifierProfile(
      id,
      declByIdentifier,
      assignmentContext,
      usageExamples
    );
    prompt += "\n";
  }

  prompt += `Identifiers to rename: ${identifiers.join(", ")}\n\n`;

  // Include all non-minified used names — minified ones will be renamed and
  // aren't useful for collision avoidance
  const isMinified = looksMinified ?? defaultLooksMinified;
  const usedList = [...usedNames].filter((n) => !isMinified(n));
  if (usedList.length > 0) {
    prompt += `Names already in use (MUST avoid these): ${usedList.join(", ")}\n\n`;
  }

  prompt += `Respond with JSON mapping EVERY identifier to a new name:\n`;
  prompt += `{ ${identifiers.map((id) => `"${id}": "descriptiveName"`).join(", ")} }`;

  return prompt;
}

/**
 * Builds a retry prefix for module-level rename prompts.
 * Prepended to the regular module-level prompt to give the LLM context
 * about what names were tried and rejected.
 */
export function buildModuleLevelRetryPrefix(
  previousAttempt: Record<string, string>,
  failures: {
    duplicates: string[];
    invalid: string[];
    missing: string[];
    unchanged: string[];
  }
): string {
  let prefix = `Your previous rename suggestions had issues:\n`;

  for (const name of failures.duplicates) {
    const suggested = previousAttempt[name];
    if (suggested) {
      prefix += `- "${name}" was suggested as "${suggested}" but that conflicts with an existing name\n`;
    }
  }
  for (const name of failures.unchanged) {
    prefix += `- "${name}" was returned as itself — you MUST suggest a DIFFERENT name\n`;
  }
  for (const name of failures.invalid) {
    const suggested = previousAttempt[name];
    if (suggested) {
      prefix += `- "${name}" was suggested as "${suggested}" which is not a valid JavaScript identifier\n`;
    }
  }
  if (failures.missing.length > 0) {
    prefix += `- These identifiers were MISSING from your response: ${failures.missing.join(", ")}\n`;
  }

  const rejectedNames = new Set<string>();
  for (const name of [
    ...failures.duplicates,
    ...failures.unchanged,
    ...failures.invalid
  ]) {
    const suggested = previousAttempt[name];
    if (suggested) rejectedNames.add(suggested);
  }
  if (rejectedNames.size > 0) {
    prefix += `\nDO NOT suggest these names: ${[...rejectedNames].join(", ")}\n`;
  }

  prefix += `\nPlease suggest DIFFERENT names for the remaining identifiers below:\n`;
  return prefix;
}
