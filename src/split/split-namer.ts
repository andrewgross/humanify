/**
 * LLM namer for NEW split files/folders (exp024). Wraps an LLMProvider's
 * suggestAllNames — reusing its JSON parsing, retry, and logging — behind
 * the SplitNamer callback stableSplitFromCode injects on the fresh path.
 *
 * Naming-only by construction: the namer sees a summary (dominant
 * bindings, siblings, member files) and returns a single basename; it
 * never sees or moves code. stableSplitFromCode validates the proposal
 * (identifier-safe, specific, not minted/placeholder) and keeps the
 * mechanical stem on any miss. Only the fresh-grouping path calls it, so
 * inherited names never change — a rename is cross-version churn.
 */

import { debug } from "../debug.js";
import type { LLMProvider } from "../llm/types.js";
import type { SplitNamer, SplitNameRequest } from "./stable-split.js";

const SYSTEM_PROMPT =
  "You name source files and folders in a decompiled JavaScript CLI tool, " +
  "the way an experienced engineer would organize a real repository. Choose " +
  "a specific, descriptive name from the code's dominant responsibility. " +
  "Avoid generic names (utils, helpers, core, common, misc, index). Return " +
  "a single camelCase or kebab-case basename, no extension, no path.";

/** Human-readable brief the model names from. */
function buildPrompt(request: SplitNameRequest): string {
  const lines: string[] = [];
  lines.push(`Name this ${request.kind} in a decompiled CLI tool repository.`);
  lines.push("");
  lines.push("Its most-referenced declarations:");
  for (const binding of request.bindings) lines.push(`  - ${binding}`);
  if (request.members && request.members.length > 0) {
    lines.push("");
    lines.push(`Files it contains: ${request.members.join(", ")}`);
  }
  if (request.siblings.length > 0) {
    lines.push("");
    lines.push(
      `Sibling ${request.kind}s (pick a DISTINCT name): ` +
        request.siblings.join(", ")
    );
  }
  lines.push("");
  lines.push(
    `Reply with JSON {"${request.mechanicalStem}": "<name>"} — a single ` +
      `specific ${request.kind} name for "${request.mechanicalStem}".`
  );
  return lines.join("\n");
}

/**
 * Build a SplitNamer over an LLMProvider. Best-effort: a decline, an echo
 * of the stem, or a provider throw all resolve to null (keep the
 * mechanical stem). Validation of the returned string is the caller's
 * (stableSplitFromCode.acceptProposedName).
 */
export function createSplitNamer(provider: LLMProvider): SplitNamer {
  return async (request: SplitNameRequest): Promise<string | null> => {
    const prompt = buildPrompt(request);
    try {
      const response = await provider.suggestAllNames({
        code: prompt,
        identifiers: [request.mechanicalStem],
        usedNames: new Set(request.siblings),
        calleeSignatures: [],
        callsites: [],
        systemPrompt: SYSTEM_PROMPT,
        userPrompt: prompt
      });
      const proposed = response.renames[request.mechanicalStem];
      if (!proposed || proposed === request.mechanicalStem) return null;
      return proposed;
    } catch (err) {
      debug.log(
        "split-namer",
        `naming ${request.kind} "${request.mechanicalStem}" failed: ` +
          `${err instanceof Error ? err.message : String(err)}`
      );
      return null;
    }
  };
}
