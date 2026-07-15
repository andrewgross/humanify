/**
 * Shared I/O for the graph-clustering split experiment. Beautifies a
 * decompiled Bun bundle ONCE (babel-generator — the same pass the
 * production pipeline uses; prettier OOMs on 12MB) and caches it, so the
 * fast algorithm-iteration loop never re-beautifies. No LLM rename: the
 * clustering works on the reference graph, which is name-agnostic.
 */

import fs from "node:fs";
import path from "node:path";
import { createBabelPlugin } from "../../../src/plugins/babel/babel.js";

export const CACHE_DIR =
  process.env.EXP029_CACHE ??
  "/private/tmp/claude-501/-Users-andrewgross-Development-humanify/f76e1d62-0710-44c5-ac26-70073059540f/scratchpad/exp029";

export function inputPath(version: string): string {
  return `/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-${version}/binary-decompiled/src/entrypoints/index.js`;
}

/** Real (unbundled) source tree for a version, if present. */
export function srcTreePath(version: string): string {
  return `/Users/andrewgross/Development/claude-code-src-${version}`;
}

export async function loadBeautified(version: string): Promise<string> {
  fs.mkdirSync(CACHE_DIR, { recursive: true });
  const cached = path.join(CACHE_DIR, `beautified-${version}.js`);
  if (fs.existsSync(cached)) return fs.readFileSync(cached, "utf8");
  const raw = fs.readFileSync(inputPath(version), "utf8");
  const beautify = createBabelPlugin();
  const out = await beautify(raw);
  fs.writeFileSync(cached, out);
  return out;
}
