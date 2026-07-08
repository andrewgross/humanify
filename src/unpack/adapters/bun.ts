import fs from "node:fs/promises";
import path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  classifyBunModules,
  nameCjsFactories,
  type BunModuleClassification,
  type CjsFactoryRecord
} from "../../analysis/bun-module-classification.js";
import { findWrapperFunction } from "../../analysis/wrapper-detection.js";
import type * as babelTraverse from "@babel/traverse";
import {
  identifyBunCjsFactory,
  identifyBunRequire
} from "../../shared/bun-helpers.js";
import type { BundlerDetectionResult } from "../../detection/types.js";
import type { UnpackAdapter, UnpackResult } from "../types.js";

/** Sidecar metadata file written alongside the extracted factory files. */
export const BUN_MODULES_MANIFEST = "_bun-modules.json";

export interface BunModulesManifestEntry {
  /** Filename written into outputDir (relative). */
  fileName: string;
  /** Human-friendly name used to derive fileName. */
  name: string;
  /** How `name` was chosen by the cascade. */
  nameSource: "banner" | "url" | "carry-over" | "llm" | "fallback";
  /** Structural hash — stable across builds. The cross-version join key. */
  structuralHash: string;
  /** Original obfuscated factoryVar in the bundle (debug only). */
  factoryVar: string;
  /**
   * Content-derived identifier every reference to this factory was
   * rewritten to (in runtime.js AND other factories' bodies). The
   * declaration is stripped during extraction, so references become FREE
   * identifiers nothing downstream can rename — and Bun re-rolls the
   * minified token every build, making each reference a cross-version
   * diff line. Rewriting to the sanitized file name (itself a pure
   * function of module content) makes the references version-stable.
   * Absent when the rewrite was skipped (no classification record, a
   * write to the factory var, or no capture-free identifier).
   */
  runtimeIdentifier?: string;
  /** Banner package, if a bang-block comment identified the library. */
  bannerPackage?: string;
  /** Banner version, if present. */
  bannerVersion?: string;
}

export interface BunModulesManifest {
  /** Always "bun" — distinguishes from other adapters that might write JSON here. */
  adapter: "bun";
  /** Filename for the leftover runtime code, if any. */
  runtimeFile?: string;
  /** One entry per extracted CJS factory file. */
  factories: BunModulesManifestEntry[];
}

export class BunUnpackAdapter implements UnpackAdapter {
  name = "bun";

  supports(detection: BundlerDetectionResult): boolean {
    return detection.bundler?.type === "bun";
  }

  async unpack(code: string, outputDir: string): Promise<UnpackResult> {
    await fs.mkdir(outputDir, { recursive: true });

    const factory = identifyBunCjsFactory(code);
    const requireVar = identifyBunRequire(code);

    if (!factory) {
      const outputPath = path.join(outputDir, "index.js");
      await fs.writeFile(outputPath, code);
      return { files: [{ path: outputPath }] };
    }

    const classification = classifyWithAst(code);
    const helperName = classification?.cjsFactoryHelperVar ?? factory.name;

    // AST extraction is the source of truth — `findMatchingParen` on raw
    // source mishandles parens inside string/regex/template literals,
    // which corrupts the body slice on real-world bundles. Fall back to
    // regex only when the AST classifier didn't run (parse failure).
    const modules: ExtractedModule[] = classification
      ? extractFactoryBodiesFromAst(classification, code)
      : extractFactoryBodies(code, helperName);
    if (modules.length === 0) {
      const outputPath = path.join(outputDir, "index.js");
      await fs.writeFile(outputPath, code);
      return { files: [{ path: outputPath }] };
    }

    const byFactoryVar = buildNamingLookup(classification);
    const files: Array<{ path: string }> = [];
    const manifestEntries: BunModulesManifestEntry[] = [];
    const { plans, declEdits, refEdits } = planModules(modules, byFactoryVar);

    // Pass 2: write each factory body with cross-factory references
    // rewritten, then assemble the runtime the same way.
    for (const mod of modules) {
      const plan = plans.get(mod);
      if (!plan) continue;
      let body = sliceWithEdits(code, refEdits, mod.bodyStart, mod.bodyEnd);
      if (requireVar) body = rewriteRequireCalls(body, requireVar);

      const record = byFactoryVar.get(mod.name);
      const outputPath = path.join(outputDir, `${plan.naming.fileName}.js`);
      await fs.writeFile(outputPath, body);
      files.push({ path: outputPath });

      manifestEntries.push({
        fileName: `${plan.naming.fileName}.js`,
        name: plan.naming.name,
        nameSource: plan.naming.nameSource,
        structuralHash: plan.naming.structuralHash,
        factoryVar: mod.name,
        runtimeIdentifier: plan.identifier,
        bannerPackage: record?.bannerPackage,
        bannerVersion: record?.bannerVersion
      });
    }

    const runtime = sliceWithEdits(
      code,
      [...declEdits, ...refEdits],
      0,
      code.length
    );
    let runtimeFile: string | undefined;
    if (runtime.trim()) {
      runtimeFile = "runtime.js";
      const runtimePath = path.join(outputDir, runtimeFile);
      await fs.writeFile(runtimePath, runtime);
      files.push({ path: runtimePath });
    }

    const manifest: BunModulesManifest = {
      adapter: "bun",
      runtimeFile,
      factories: manifestEntries
    };
    await fs.writeFile(
      path.join(outputDir, BUN_MODULES_MANIFEST),
      `${JSON.stringify(manifest, null, 2)}\n`
    );

    return { files };
  }
}

interface ExtractedModule {
  name: string;
  /** Byte range of the factory body (the inner function expression). */
  bodyStart: number;
  bodyEnd: number;
  /** Byte range of the whole declaration (spliced out of the runtime). */
  declStart: number;
  declEnd: number;
}

/** A byte-precise source substitution ("" = splice the range out). */
interface TextEdit {
  start: number;
  end: number;
  replacement: string;
}

interface ModulePlan {
  naming: NameLookup;
  identifier?: string;
}

/**
 * Pass 1 of unpack: choose file names and plan the stable-identifier
 * rewrite for every factory-var reference (references survive extraction
 * as FREE identifiers, so this is the only point they can ever be
 * renamed).
 */
function planModules(
  modules: ExtractedModule[],
  byFactoryVar: Map<string, CjsFactoryRecord>
): {
  plans: Map<ExtractedModule, ModulePlan>;
  declEdits: TextEdit[];
  refEdits: TextEdit[];
} {
  const usedNames = new Set<string>();
  const usedIdentifiers = new Set<string>();
  const declEdits: TextEdit[] = [];
  const refEdits: TextEdit[] = [];
  const plans = new Map<ExtractedModule, ModulePlan>();

  for (const mod of modules) {
    declEdits.push({ start: mod.declStart, end: mod.declEnd, replacement: "" });

    const record = byFactoryVar.get(mod.name);
    const naming = chooseFileName(mod.name, record, usedNames);
    usedNames.add(naming.fileName);

    let identifier: string | undefined;
    if (record) {
      const rename = planFactoryRename(
        record,
        naming.fileName,
        usedIdentifiers
      );
      if (rename) {
        identifier = rename.identifier;
        usedIdentifiers.add(rename.identifier);
        refEdits.push(...rename.edits);
      }
    }
    plans.set(mod, { naming, identifier });
  }

  return { plans, declEdits, refEdits };
}

/**
 * Slice [sliceStart, sliceEnd) of `code` with every edit inside the range
 * applied. Edits contained in an already-consumed range (a reference
 * inside a spliced-out declaration) are dropped.
 */
function sliceWithEdits(
  code: string,
  edits: TextEdit[],
  sliceStart: number,
  sliceEnd: number
): string {
  const inRange = edits
    .filter((e) => e.start >= sliceStart && e.end <= sliceEnd)
    .sort((a, b) => a.start - b.start || b.end - a.end);
  const parts: string[] = [];
  let cursor = sliceStart;
  for (const edit of inRange) {
    if (edit.start < cursor) continue;
    parts.push(code.slice(cursor, edit.start), edit.replacement);
    cursor = edit.end;
  }
  parts.push(code.slice(cursor, sliceEnd));
  return parts.join("");
}

/**
 * Plan the reference rewrite for one factory: resolve its binding on the
 * classification AST (declaration still present — the only moment the
 * references are resolvable), choose a capture-free identifier derived
 * from the file name, and emit one edit per reference. Returns null when
 * the rewrite is unsafe: unresolvable/shadowed binding, a WRITE to the
 * factory var (partial rewrite would corrupt scope), or no capture-free
 * identifier.
 */
function planFactoryRename(
  record: CjsFactoryRecord,
  fileName: string,
  usedIdentifiers: Set<string>
): { identifier: string; edits: TextEdit[] } | null {
  const declPath = record.factoryPath;
  const binding = declPath.scope.getBinding(record.factoryVar);
  if (!binding || binding.path.node !== declPath.node) return null;
  if (binding.constantViolations.length > 0) return null;

  const identifier = chooseCaptureFreeIdentifier(
    sanitizeIdentifier(fileName),
    binding,
    usedIdentifiers
  );
  if (!identifier) return null;

  const edits: TextEdit[] = [];
  for (const ref of binding.referencePaths) {
    const node = ref.node;
    if (!t.isIdentifier(node) || node.start == null || node.end == null) {
      return null;
    }
    edits.push({ start: node.start, end: node.end, replacement: identifier });
  }
  return { identifier, edits };
}

/** File names allow `@ . -`; identifiers don't. */
function sanitizeIdentifier(fileName: string): string {
  const base = fileName.replace(/[^A-Za-z0-9_$]/g, "_");
  return /^[0-9]/.test(base) ? `_${base}` : base;
}

/**
 * The candidate (or a `_2`, `_3`, ... variant) that no reference site can
 * capture: not already chosen for another factory, not an existing free
 * name in the bundle (rewriting to it would conflate two different free
 * identifiers), and not bound in any scope visible from a reference.
 */
function chooseCaptureFreeIdentifier(
  base: string,
  binding: NonNullable<ReturnType<babelTraverse.Scope["getBinding"]>>,
  usedIdentifiers: Set<string>
): string | null {
  const programScope = binding.scope.getProgramParent();
  for (let i = 1; i <= 1000; i++) {
    const candidate = i === 1 ? base : `${base}_${i}`;
    if (usedIdentifiers.has(candidate)) continue;
    if (Object.hasOwn(programScope.globals, candidate)) continue;
    const captured = binding.referencePaths.some((ref) =>
      ref.scope.hasBinding(candidate)
    );
    if (!captured) return candidate;
  }
  return null;
}

interface NameLookup {
  fileName: string;
  name: string;
  nameSource: "banner" | "url" | "carry-over" | "llm" | "fallback";
  structuralHash: string;
}

function classifyWithAst(code: string) {
  try {
    const ast = parseSync(code, {
      sourceType: "unambiguous",
      parserOpts: { errorRecovery: true }
    });
    if (!ast || ast.type !== "File") return null;
    const wrapper = findWrapperFunction(ast as t.File);
    const classification = classifyBunModules(ast as t.File, code, wrapper);
    if (classification) nameCjsFactories(classification, code);
    return classification;
  } catch {
    return null;
  }
}

function buildNamingLookup(
  classification: ReturnType<typeof classifyWithAst>
): Map<string, CjsFactoryRecord> {
  const map = new Map<string, CjsFactoryRecord>();
  if (!classification) return map;
  for (const factory of classification.factories) {
    map.set(factory.factoryVar, factory);
  }
  return map;
}

/**
 * Sanitize a cascade name into something safe to use as a filename.
 * Keeps alphanumerics, `@`, `-`, `_`, `.`; replaces everything else.
 */
function sanitizeFsName(name: string): string {
  // `/` is common in scoped packages — convert to `__`.
  const normalized = name.replace(/\//g, "__");
  return normalized.replace(/[^@A-Za-z0-9._-]/g, "_");
}

/**
 * Resolve the on-disk filename for a factory. Falls back to the factoryVar
 * when classification produced nothing (e.g., a body the regex saw but the
 * AST classifier missed). Disambiguates collisions deterministically with
 * a `-2`, `-3`, ... suffix in source order.
 */
function chooseFileName(
  factoryVar: string,
  record: CjsFactoryRecord | undefined,
  used: Set<string>
): NameLookup {
  if (record?.name && record.nameSource) {
    return {
      fileName: disambiguate(sanitizeFsName(record.name), used),
      name: record.name,
      nameSource: record.nameSource,
      structuralHash: record.structuralHash
    };
  }
  return {
    fileName: disambiguate(factoryVar, used),
    name: factoryVar,
    nameSource: "fallback",
    structuralHash: ""
  };
}

/**
 * Append `-2`, `-3`, ... until the candidate name is unused. Source-order
 * stable so the same input bundle always produces the same filenames.
 */
function disambiguate(base: string, used: Set<string>): string {
  if (!used.has(base)) return base;
  for (let i = 2; i < 1_000_000; i++) {
    const candidate = `${base}-${i}`;
    if (!used.has(candidate)) return candidate;
  }
  // Should never reach here; degenerate fallback.
  return `${base}-overflow`;
}

/**
 * AST-precise factory extraction. Uses the byte ranges Babel records on
 * each `var X = HELPER(...)` VariableDeclarator and on the inner factory
 * function expression. Robust to string/regex/template literals containing
 * parens — the failure mode of the regex extractor below.
 */
function extractFactoryBodiesFromAst(
  classification: BunModuleClassification,
  code: string
): ExtractedModule[] {
  const modules: ExtractedModule[] = [];
  for (const factory of classification.factories) {
    const m = factoryToModule(factory, code);
    if (m) modules.push(m);
  }
  return modules;
}

/**
 * Build a single ExtractedModule from a classified factory. Returns null
 * if AST positions are missing or the call shape doesn't match
 * `HELPER(arrowOrFunction)`.
 */
function factoryToModule(
  factory: CjsFactoryRecord,
  code: string
): ExtractedModule | null {
  const init = factory.factoryPath.node.init;
  if (!t.isCallExpression(init) || init.arguments.length === 0) return null;
  const arg0 = init.arguments[0];
  if (!t.isArrowFunctionExpression(arg0) && !t.isFunctionExpression(arg0)) {
    return null;
  }
  const bodyStart = arg0.start;
  const bodyEnd = arg0.end;
  if (bodyStart == null || bodyEnd == null) return null;

  // factoryPath is the VariableDeclarator; its parent is the
  // VariableDeclaration whose source range starts at the `var` keyword.
  // We cover the keyword too — otherwise the runtime extractor leaves
  // stray `var ` tokens behind.
  const declParent = factory.factoryPath.parentPath?.node;
  if (!declParent || declParent.start == null || declParent.end == null) {
    return null;
  }
  let declEnd = declParent.end;
  if (declEnd < code.length && code[declEnd] === ";") declEnd++;

  return {
    name: factory.factoryVar,
    bodyStart,
    bodyEnd,
    declStart: declParent.start,
    declEnd
  };
}

/**
 * Find all `var NAME = FACTORY_HELPER(...)` declarations and extract
 * the factory body (between the outermost parens).
 *
 * Regex fallback used only when the AST classifier fails to parse the
 * bundle. Mishandles parens inside string/regex/template literals.
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
    const parenStart = match.index + match[0].length - 1;
    const parenEnd = findMatchingParen(code, parenStart);
    if (parenEnd === -1) continue;

    let declEnd = parenEnd + 1;
    if (declEnd < code.length && code[declEnd] === ";") declEnd++;

    modules.push({
      name: varName,
      bodyStart: parenStart + 1,
      bodyEnd: parenEnd,
      declStart,
      declEnd
    });
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

/** Rewrite `REQUIRE_VAR("...")` calls to `require("...")`. */
function rewriteRequireCalls(body: string, requireVar: string): string {
  const re = new RegExp(`\\b${escapeRegExp(requireVar)}\\(`, "g");
  return body.replace(re, "require(");
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
