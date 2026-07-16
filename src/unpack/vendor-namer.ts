/**
 * LLM naming for vendored CJS factories that the deterministic cascade
 * (banner → URL → carry-over) left hash-named — 1,498 of 1,523 vendor
 * files on a real CC bundle. Runs AFTER the cascade and only over
 * `fallback`-named records, so trusted sources always win.
 *
 * Same shape philosophy as the split namer: one provider call per batch
 * (suggestAllNames does the JSON/retry work), evidence-only prompts (the
 * model never sees or moves code), every proposal validated and floored
 * back to lib_<hash> on a miss.
 */

import type { CjsFactoryRecord } from "../analysis/bun-module-classification.js";
import { debug } from "../debug.js";
import type { LLMProvider } from "../llm/types.js";

export interface VendorNameRequest {
  /** The record's current fallback name (lib_<hash>) — the batch key. */
  key: string;
  /** Code-derived evidence: export names, URLs, distinctive strings. */
  evidence: string;
}

/** Batch namer: one proposal or null per request, in request order. */
export type VendorNamer = (
  requests: VendorNameRequest[]
) => Promise<Array<string | null>>;

const SYSTEM_PROMPT =
  "You identify vendored third-party npm packages inside a decompiled " +
  "JavaScript bundle. For each entry, infer the package's npm name from " +
  "its code evidence (export names, URLs, distinctive string literals). " +
  "When the exact package is unclear, give a short descriptive kebab-case " +
  "module name instead. Never invent a scope. Reply with one name per key.";

/** Names too generic to identify anything. */
const GENERIC_VENDOR_NAMES = new Set([
  "lib",
  "library",
  "libs",
  "module",
  "modules",
  "package",
  "pkg",
  "vendor",
  "unknown",
  "utils",
  "util",
  "helpers",
  "helper",
  "index",
  "misc",
  "common",
  "core"
]);

function unique(values: string[]): string[] {
  return [...new Set(values)];
}

/** Compact per-factory evidence block for the naming prompt. */
export function buildVendorEvidence(body: string, capChars = 700): string {
  const urls = unique(body.match(/https?:\/\/[^\s"'`)]+/g) ?? []).slice(0, 3);
  const exportsProps = unique(
    [...body.matchAll(/\bexports\.([A-Za-z_$][\w$]*)\s*=/g)].map((m) => m[1])
  ).slice(0, 10);
  const strings = unique(
    [...body.matchAll(/["']([^"'\\\n]{4,60})["']/g)]
      .map((m) => m[1])
      .filter((s) => /[a-z]/i.test(s))
  ).slice(0, 10);

  const lines: string[] = [];
  if (exportsProps.length > 0)
    lines.push(`exports: ${exportsProps.join(", ")}`);
  if (urls.length > 0) lines.push(`urls: ${urls.join(" ")}`);
  if (strings.length > 0) {
    lines.push(`strings: ${strings.map((s) => JSON.stringify(s)).join(", ")}`);
  }
  lines.push(`size: ${body.length} bytes`);
  return lines.join("\n").slice(0, capChars);
}

/**
 * Validate one proposal into a package-shaped vendor name (lowercased,
 * optional @scope/, dots/dashes allowed), or null when it is generic,
 * minified-short, or malformed.
 */
export function acceptVendorName(proposal: string): string | null {
  const name = proposal.trim().toLowerCase();
  if (!/^(@[a-z0-9][a-z0-9._-]*\/)?[a-z0-9][a-z0-9._-]{2,39}$/.test(name)) {
    return null;
  }
  if (GENERIC_VENDOR_NAMES.has(name)) return null;
  return name;
}

/** Prompt for one batch of vendor-name requests. */
function buildPrompt(requests: VendorNameRequest[]): string {
  const lines: string[] = [
    `Identify ${requests.length} vendored modules extracted from a JavaScript bundle.`,
    ""
  ];
  for (const request of requests) {
    lines.push(`### ${request.key}`);
    lines.push(request.evidence);
    lines.push("");
  }
  lines.push(
    `Reply with JSON {${requests
      .map((r) => `"${r.key}": "<npm package or kebab-case name>"`)
      .join(", ")}}.`
  );
  return lines.join("\n");
}

/**
 * Build a VendorNamer over an LLMProvider. Best-effort: a decline, an
 * echo of the key, or a provider throw all resolve to null entries.
 */
export function createVendorNamer(provider: LLMProvider): VendorNamer {
  return async (requests) => {
    if (requests.length === 0) return [];
    const prompt = buildPrompt(requests);
    try {
      const response = await provider.suggestAllNames({
        code: prompt,
        identifiers: requests.map((r) => r.key),
        usedNames: new Set(),
        calleeSignatures: [],
        callsites: [],
        systemPrompt: SYSTEM_PROMPT,
        userPrompt: prompt
      });
      return requests.map((request) => {
        const proposed = response.renames[request.key];
        if (!proposed || proposed === request.key) return null;
        return proposed;
      });
    } catch (err) {
      debug.log(
        "vendor-namer",
        `naming batch of ${requests.length} failed: ` +
          `${err instanceof Error ? err.message : String(err)}`
      );
      return requests.map(() => null);
    }
  };
}

/**
 * Re-name every `fallback`-named factory record through the namer,
 * chunked. Accepted proposals become the record's name with nameSource
 * "llm"; everything else keeps the lib_<hash> fallback. Returns how many
 * records were renamed. Mutates the records, mirroring nameCjsFactories.
 */
export async function nameFallbackFactoriesWithLlm(
  factories: CjsFactoryRecord[],
  source: string,
  namer: VendorNamer,
  chunkSize = 24
): Promise<number> {
  const fallbacks = factories.filter(
    (f) => f.nameSource === "fallback" && f.name
  );
  const chunks: CjsFactoryRecord[][] = [];
  for (let i = 0; i < fallbacks.length; i += chunkSize) {
    chunks.push(fallbacks.slice(i, i + chunkSize));
  }
  let renamed = 0;
  await Promise.all(
    chunks.map(async (chunk) => {
      const requests = chunk.map((record) => ({
        key: record.name as string,
        evidence: buildVendorEvidence(
          source.slice(record.byteRange[0], record.byteRange[1])
        )
      }));
      const proposals = await namer(requests);
      chunk.forEach((record, i) => {
        const accepted = proposals?.[i]
          ? acceptVendorName(proposals[i] as string)
          : null;
        if (accepted) {
          record.name = accepted;
          record.nameSource = "llm";
          renamed++;
        }
      });
    })
  );
  return renamed;
}
