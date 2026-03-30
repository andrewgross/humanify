import fs from "node:fs/promises";
import path from "node:path";
import { identifyBunCjsFactory, identifyBunRequire } from "../bun-helpers.js";
import type {
  BundlerAdapter,
  DetectionResult,
  UnpackResult
} from "../types.js";

export class BunUnpackAdapter implements BundlerAdapter {
  name = "bun";

  supports(detection: DetectionResult): boolean {
    return detection.bundler?.type === "bun";
  }

  async unpack(code: string, outputDir: string): Promise<UnpackResult> {
    await fs.mkdir(outputDir, { recursive: true });

    const factory = identifyBunCjsFactory(code);
    const requireVar = identifyBunRequire(code);

    if (!factory) {
      // No factory helper found — treat as single file
      const outputPath = path.join(outputDir, "index.js");
      await fs.writeFile(outputPath, code);
      return { files: [{ path: outputPath }] };
    }

    const modules = extractFactoryBodies(code, factory.name);

    if (modules.length === 0) {
      const outputPath = path.join(outputDir, "index.js");
      await fs.writeFile(outputPath, code);
      return { files: [{ path: outputPath }] };
    }

    const files: Array<{ path: string }> = [];

    // Collect covered character ranges to extract runtime
    const coveredRanges: Array<{ start: number; end: number }> = [];

    for (let i = 0; i < modules.length; i++) {
      let body = modules[i].body;
      coveredRanges.push({
        start: modules[i].declStart,
        end: modules[i].declEnd
      });

      // Rewrite require calls if we identified the require variable
      if (requireVar) {
        body = rewriteRequireCalls(body, requireVar);
      }

      const fileName = `${modules[i].name}.js`;
      const outputPath = path.join(outputDir, fileName);
      await fs.writeFile(outputPath, body);
      files.push({ path: outputPath });
    }

    // Extract runtime (code not inside any factory declaration)
    const runtime = extractRuntime(code, coveredRanges);
    if (runtime.trim()) {
      const runtimePath = path.join(outputDir, "runtime.js");
      await fs.writeFile(runtimePath, runtime);
      files.push({ path: runtimePath });
    }

    return { files };
  }
}

interface ExtractedModule {
  name: string;
  body: string;
  declStart: number;
  declEnd: number;
}

/**
 * Find all `var NAME = FACTORY_HELPER(...)` declarations and extract
 * the factory body (between the outermost parens).
 */
function extractFactoryBodies(
  code: string,
  factoryName: string
): ExtractedModule[] {
  const modules: ExtractedModule[] = [];
  const pattern = new RegExp(
    `(?:var|let|const)\\s+([$\\w]+)\\s*=\\s*${escapeRegExp(factoryName)}\\s*\\(`,
    "g"
  );

  for (
    let match = pattern.exec(code);
    match !== null;
    match = pattern.exec(code)
  ) {
    const varName = match[1];
    const declStart = match.index;
    // Find the opening paren of the factory call
    const parenStart = match.index + match[0].length - 1;
    const parenEnd = findMatchingParen(code, parenStart);
    if (parenEnd === -1) continue;

    // Body is between the outer parens (exclusive)
    const body = code.slice(parenStart + 1, parenEnd);

    // Declaration ends after closing paren + optional semicolon
    let declEnd = parenEnd + 1;
    if (declEnd < code.length && code[declEnd] === ";") declEnd++;

    modules.push({ name: varName, body, declStart, declEnd });
  }

  return modules;
}

/** Find the matching closing paren by tracking depth. */
function findMatchingParen(code: string, openPos: number): number {
  let depth = 0;
  for (let i = openPos; i < code.length; i++) {
    if (code[i] === "(") depth++;
    else if (code[i] === ")") {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/**
 * Rewrite `REQUIRE_VAR("...")` calls to `require("...")`.
 */
function rewriteRequireCalls(body: string, requireVar: string): string {
  const re = new RegExp(`\\b${escapeRegExp(requireVar)}\\(`, "g");
  return body.replace(re, "require(");
}

/** Extract code not covered by any factory declaration. */
function extractRuntime(
  code: string,
  coveredRanges: Array<{ start: number; end: number }>
): string {
  const sorted = [...coveredRanges].sort((a, b) => a.start - b.start);
  const parts: string[] = [];
  let cursor = 0;

  for (const range of sorted) {
    if (cursor < range.start) {
      parts.push(code.slice(cursor, range.start));
    }
    cursor = Math.max(cursor, range.end);
  }

  if (cursor < code.length) {
    parts.push(code.slice(cursor));
  }

  return parts.join("");
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
