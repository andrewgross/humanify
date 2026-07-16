/**
 * LLM namer for NEW split files/folders (exp024, batched for the
 * human-layout work). Wraps an LLMProvider's suggestAllNames — reusing its
 * JSON parsing, retry, and logging — behind the SplitNamer callback
 * stableSplitFromCode injects on the fresh path.
 *
 * A whole sibling scope arrives as ONE batch → one provider call, so the
 * model names siblings against each other (and the repo's top level is
 * named jointly, the way a human surveys the whole set before naming).
 *
 * Naming-only by construction: the namer sees per-entry summaries
 * (dominant bindings, siblings, member files) and returns basenames; it
 * never sees or moves code. The caller validates every proposal
 * (identifier-safe, specific, not minted/placeholder, not a member echo)
 * and keeps the mechanical stem on any miss. Only the fresh-grouping path
 * calls it, so inherited names never change — a rename is cross-version
 * churn.
 */

import { debug } from "../debug.js";
import type { LLMProvider } from "../llm/types.js";
import { uniqueCaseInsensitiveName } from "../shared/unique-name.js";
import type { SplitNameRequest, SplitNamer } from "./stable-split.js";

export const SPLIT_NAMER_SYSTEM_PROMPT =
  "You name source files and folders in a decompiled JavaScript CLI tool, " +
  "the way an experienced engineer would organize a real repository.\n" +
  "Name the CONCEPT — what the code is about — from the evidence (the " +
  "strings it uses, the APIs it calls, its declarations). Do NOT just echo " +
  "the loudest function name.\n" +
  "Rules:\n" +
  "- Use a NOUN or noun phrase, 1-3 words. Good: retry-scheduler, " +
  "hostname-resolver, token-bucket, message-queue, diff-view, auth-flow.\n" +
  "- A FOLDER is a domain bucket: a plain noun (auth, transcript, tools, " +
  "permissions). Never a verb phrase (get-display-name), never a " +
  "conjunction (foo-and-bar — that means it should be two folders), never " +
  "a decoration suffix (Manager, Suite, Engine, Group, Handler).\n" +
  "- Never start a name with a conjunction, article, or preposition " +
  "(and, or, the, a, with, for). Never a bare verb.\n" +
  "- Avoid generic names (utils, helpers, core, common, misc, index) and " +
  "numeric suffixes (initializer17).\n" +
  "- Siblings must be DISTINCT; a folder name must not repeat one member.\n" +
  "Return a single kebab-case basename per entry, no extension, no path.";
const SYSTEM_PROMPT = SPLIT_NAMER_SYSTEM_PROMPT;

/** One entry's brief within the batch prompt. */
function renderEntry(key: string, request: SplitNameRequest): string[] {
  const lines = [`### ${key} (${request.kind})`];
  if (request.evidence && request.evidence.length > 0) {
    lines.push(`What it does (from its code): ${request.evidence}`);
  }
  lines.push("Most-referenced declarations:");
  for (const binding of request.bindings) lines.push(`  - ${binding}`);
  if (request.members && request.members.length > 0) {
    lines.push(`Files it contains: ${request.members.join(", ")}`);
    lines.push("Name the whole group, not one member.");
  }
  if (request.level === "top") {
    lines.push(
      "This is a TOP-LEVEL source folder: prefer a short plain domain " +
        "noun (like auth, permissions, transcript, tools) — no decorated " +
        "suffixes such as Suite, Engine, Hub, or Manager."
    );
  }
  if (request.siblings.length > 0) {
    lines.push(
      `Sibling ${request.kind}s (pick a DISTINCT name): ` +
        request.siblings.join(", ")
    );
  }
  return lines;
}

/** Human-readable brief the model names from — all entries of one batch. */
function buildPrompt(requests: SplitNameRequest[], keys: string[]): string {
  const lines: string[] = [
    `Name ${requests.length} entries in a decompiled CLI tool repository.`,
    ""
  ];
  for (let i = 0; i < requests.length; i++) {
    lines.push(...renderEntry(keys[i], requests[i]));
    lines.push("");
  }
  lines.push(
    `Reply with JSON {${keys.map((k) => `"${k}": "<name>"`).join(", ")}} — ` +
      "one specific name per entry."
  );
  return lines.join("\n");
}

/**
 * Build a SplitNamer over an LLMProvider. Best-effort: a decline, an echo
 * of the stem, or a provider throw all resolve to null (keep the
 * mechanical stem). Duplicate stems within a batch are uniquified into
 * distinct prompt keys so every brief maps to exactly one answer.
 * Validation of the returned strings is the caller's job.
 */
export function createSplitNamer(provider: LLMProvider): SplitNamer {
  return async (requests) => {
    if (requests.length === 0) return [];
    const used = new Set<string>();
    const keys = requests.map((r) =>
      uniqueCaseInsensitiveName(r.mechanicalStem, used, "")
    );
    const prompt = buildPrompt(requests, keys);
    try {
      const response = await provider.suggestAllNames({
        code: prompt,
        identifiers: keys,
        usedNames: new Set(requests.flatMap((r) => r.siblings)),
        calleeSignatures: [],
        callsites: [],
        systemPrompt: SYSTEM_PROMPT,
        userPrompt: prompt
      });
      return requests.map((request, i) => {
        const proposed = response.renames[keys[i]];
        if (
          !proposed ||
          proposed === request.mechanicalStem ||
          proposed === keys[i]
        ) {
          return null;
        }
        return proposed;
      });
    } catch (err) {
      debug.log(
        "split-namer",
        `naming batch of ${requests.length} failed: ` +
          `${err instanceof Error ? err.message : String(err)}`
      );
      return requests.map(() => null);
    }
  };
}
