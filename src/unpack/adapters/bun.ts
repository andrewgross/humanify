import fs from "node:fs/promises";
import fsSync from "node:fs";
import path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  classifyBunModules,
  hashFallbackName,
  isHashFallbackName,
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
import { stripJsExtension, vendorStemFor } from "../../shared/cjs-factory.js";
import { uniqueCaseInsensitiveName } from "../../shared/unique-name.js";
import { VENDOR_DIR } from "../../split/layout.js";
import type { UnpackAdapter, UnpackOptions, UnpackResult } from "../types.js";
import { nameFallbackFactoriesWithLlm } from "../vendor-namer.js";
import { verbose } from "../../verbose.js";

/** One-line note when the LLM pass upgraded hash-named vendor files. */
function verboseLogVendorNaming(renamed: number, total: number): void {
  verbose.log(`Vendor naming: LLM named ${renamed}/${total} factories`);
}

/** Sidecar metadata filename, written INSIDE the vendor/ folder alongside
 * the extracted factory files (vendor/ stays self-describing). */
export const BUN_MODULES_MANIFEST = "_bun-modules.json";

/** The Bun unpack manifest's path within an output tree — the single source
 * of truth for callers that resolve it from the output directory (the writer
 * here and the runnable-relink reader). The library detector resolves it from
 * the extracted files' own directory instead, since it never sees outputDir. */
export function bunManifestPath(outputDir: string): string {
  return path.join(outputDir, VENDOR_DIR, BUN_MODULES_MANIFEST);
}

/**
 * Cross-release vendor names to carry over, read from the tree
 * `--prior-version` points into: structuralHash → the names its factories
 * carried, IN BUNDLE ORDER.
 *
 * Vendor names are LLM-derived and NOT reproducible run-to-run, so an
 * unchanged library is renamed every release. src/ imports vendor by path,
 * so that drift rewrites require() lines throughout app code — the churn
 * dominates a cross-version diff even though vendor/ itself is excluded
 * from the history. Feeding these into the naming cascade pins unchanged
 * libraries to the name the lineage already used.
 *
 * A LIST per hash, not one name: re-export shims are structurally identical
 * but proxy different libraries, so one hash covers several distinct names.
 * Collapsing them would misname every member of the group. The manifest is
 * written in bundle order, which is the order the cascade walks factories
 * in, so position is the tie-break (see priorNameFor).
 *
 * Mirrors findSplitLedgerIn: --prior-version normally points at a prior
 * release's .humanify/humanified.js, so try that file's own directory as
 * the tree root first, then its parent (the .humanify/ case).
 */
export function loadPriorVendorNames(
  priorFile: string
): Map<string, string[]> | undefined {
  const dir = path.dirname(priorFile);
  const manifestPath = [
    bunManifestPath(dir),
    bunManifestPath(path.dirname(dir))
  ].find((candidate) => fsSync.existsSync(candidate));
  if (!manifestPath) return undefined;
  try {
    const manifest = JSON.parse(
      fsSync.readFileSync(manifestPath, "utf-8")
    ) as BunModulesManifest;
    const names = new Map<string, string[]>();
    for (const entry of manifest.factories) {
      if (!entry.structuralHash || !entry.name) continue;
      const group = names.get(entry.structuralHash) ?? [];
      group.push(entry.name);
      names.set(entry.structuralHash, group);
    }
    return names.size > 0 ? names : undefined;
  } catch {
    return undefined;
  }
}

export interface BunModulesManifestEntry {
  /** Path of the extracted factory file, relative to the output root
   * (`vendor/<name>.js`). */
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

  async unpack(
    code: string,
    outputDir: string,
    options?: UnpackOptions
  ): Promise<UnpackResult> {
    await fs.mkdir(outputDir, { recursive: true });

    const factory = identifyBunCjsFactory(code);
    const requireVar = identifyBunRequire(code);

    if (!factory) {
      const outputPath = path.join(outputDir, "index.js");
      await fs.writeFile(outputPath, code);
      return { files: [{ path: outputPath }] };
    }

    const classification = classifyWithAst(code, options?.priorVendorNames);
    if (classification && options?.vendorNamer) {
      // Post-cascade LLM pass: only hash-named (fallback) factories are
      // re-named, so banner/URL/carry-over names always win.
      const renamed = await nameFallbackFactoriesWithLlm(
        classification.factories,
        code,
        options.vendorNamer
      );
      if (renamed > 0) {
        verboseLogVendorNaming(renamed, classification.factories.length);
      }
    }
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
    const { plans, declEdits, refEdits } = planModules(
      modules,
      byFactoryVar,
      code
    );

    // Pass 2: write each factory body (vendored library code, set aside
    // under vendor/) with cross-factory references rewritten, then
    // assemble the runtime the same way.
    await fs.mkdir(path.join(outputDir, VENDOR_DIR), { recursive: true });
    for (const mod of modules) {
      const plan = plans.get(mod);
      if (!plan) continue;
      let body = sliceWithEdits(code, refEdits, mod.bodyStart, mod.bodyEnd);
      if (requireVar) body = rewriteRequireCalls(body, requireVar);
      const record = byFactoryVar.get(mod.name);
      const relPath = `${VENDOR_DIR}/${plan.naming.fileName}.js`;
      files.push({ path: await writeVendorFile(outputDir, relPath, body) });
      manifestEntries.push({
        fileName: relPath,
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
      bunManifestPath(outputDir),
      `${JSON.stringify(manifest, null, 2)}\n`
    );

    return { files };
  }
}

/** Write one vendored factory body, creating a nested package folder
 * (vendor/@scope/name/…) when the grouped name has one. Returns the path. */
async function writeVendorFile(
  outputDir: string,
  relPath: string,
  body: string
): Promise<string> {
  const outputPath = path.join(outputDir, relPath);
  if (relPath.slice(`${VENDOR_DIR}/`.length).includes("/")) {
    await fs.mkdir(path.dirname(outputPath), { recursive: true });
  }
  await fs.writeFile(outputPath, body);
  return outputPath;
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
  byFactoryVar: Map<string, CjsFactoryRecord>,
  code: string
): {
  plans: Map<ExtractedModule, ModulePlan>;
  declEdits: TextEdit[];
  refEdits: TextEdit[];
} {
  // Per-folder lowercased used stems — uniquify folds case (a
  // case-insensitive FS collapses Foo.js and foo.js). The root folder is
  // keyed "". A package that identified >=2 modules gets its own folder.
  const usedByFolder = new Map<string, Set<string>>();
  const usedIdentifiers = new Set<string>();
  const nameCounts = countIdentifiedNames(modules, byFactoryVar);
  const declEdits: TextEdit[] = [];
  const refEdits: TextEdit[] = [];
  const plans = new Map<ExtractedModule, ModulePlan>();

  for (const mod of modules) {
    declEdits.push({ start: mod.declStart, end: mod.declEnd, replacement: "" });

    const record = byFactoryVar.get(mod.name);
    const naming = chooseFileName(
      mod.name,
      record,
      { usedByFolder, nameCounts },
      code.slice(mod.bodyStart, mod.bodyEnd)
    );

    let identifier: string | undefined;
    if (record) {
      // The identifier is DECOUPLED from the display path — derived from
      // the module's stable structural stem, so introducing a package
      // folder (a display change) never churns runtime.js references.
      const rename = planFactoryRename(
        record,
        stableStem(record),
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

/** Exact-name occurrence counts over identified (non-fallback) factories —
 * a name shared by >=2 is a package with multiple internal modules. Exact,
 * not case-folded: two packages differing only in case (Ab vs aB) are
 * distinct libraries, not one folder. */
function countIdentifiedNames(
  modules: ExtractedModule[],
  byFactoryVar: Map<string, CjsFactoryRecord>
): Map<string, number> {
  const counts = new Map<string, number>();
  for (const mod of modules) {
    const record = byFactoryVar.get(mod.name);
    if (record?.name && record.nameSource && record.nameSource !== "fallback") {
      counts.set(record.name, (counts.get(record.name) ?? 0) + 1);
    }
  }
  return counts;
}

/** The module's cross-version-stable identity stem: lib_<structuralHash8>
 * when classified, else the content-floored factory var. Used for the free
 * identifier so it survives display-name and folder changes. */
function stableStem(record: CjsFactoryRecord): string {
  return record.structuralHash
    ? hashFallbackName(record.structuralHash)
    : record.factoryVar;
}

/** A unique stem within `folder`'s namespace (case-folded). */
function uniqueInFolder(
  usedByFolder: Map<string, Set<string>>,
  folder: string,
  stem: string
): string {
  let used = usedByFolder.get(folder);
  if (!used) {
    used = new Set<string>();
    usedByFolder.set(folder, used);
  }
  return uniqueCaseInsensitiveName(stem, used);
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
 * from the module's stable stem, and emit one edit per reference. Returns
 * null when the rewrite is unsafe: unresolvable/shadowed binding, a WRITE
 * to the factory var (partial rewrite would corrupt scope), or no
 * capture-free identifier.
 */
function planFactoryRename(
  record: CjsFactoryRecord,
  identifierBase: string,
  usedIdentifiers: Set<string>
): { identifier: string; edits: TextEdit[] } | null {
  const declPath = record.factoryPath;
  const binding = declPath.scope.getBinding(record.factoryVar);
  if (!binding || binding.path.node !== declPath.node) return null;
  if (binding.constantViolations.length > 0) return null;

  const identifier = chooseCaptureFreeIdentifier(
    sanitizeIdentifier(identifierBase),
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

function classifyWithAst(code: string, priorNames?: Map<string, string[]>) {
  try {
    const ast = parseSync(code, {
      sourceType: "unambiguous",
      parserOpts: { errorRecovery: true }
    });
    if (!ast || ast.type !== "File") return null;
    const wrapper = findWrapperFunction(ast as t.File);
    const classification = classifyBunModules(ast as t.File, code, wrapper);
    if (classification) nameCjsFactories(classification, code, priorNames);
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

/** Like sanitizeFsName but KEEPS `/` as a path separator, sanitizing each
 * segment — so an @scope/name package becomes a nested folder
 * (vendor/@scope/name/…), the way node_modules lays scoped packages out. */
function sanitizeFsPath(name: string): string {
  return name
    .split("/")
    .map((seg) => sanitizeFsName(seg))
    .filter(Boolean)
    .join("/");
}

/**
 * Resolve the on-disk filename for a factory. Falls back to the factoryVar
 * when classification produced nothing (e.g., a body the regex saw but the
 * AST classifier missed). Collisions disambiguate deterministically with a
 * `-2`, `-3`, ... suffix in source order, folding case (`usedLower`) so two
 * names differing only in case can't collapse on a case-insensitive FS. The
 * `.js` extension is appended by the caller, so it is not part of the name
 * uniquified here.
 */
function chooseFileName(
  factoryVar: string,
  record: CjsFactoryRecord | undefined,
  scope: {
    usedByFolder: Map<string, Set<string>>;
    nameCounts: Map<string, number>;
  },
  bodyText: string
): NameLookup {
  const { usedByFolder, nameCounts } = scope;
  if (record?.name && record.nameSource) {
    // A package that identified >=2 modules groups into vendor/<package>/,
    // each module named by its stable structural stem — a human puts a
    // library's parts in one folder, not axios@1.0.0 / -2 / -3 scattered
    // in the root. Trusted cascade name; strip a trailing .js so appending
    // the extension can never yield highlight.js.js.
    //
    // Gate on the NAME, not nameSource: a lib_<hash> fallback identifies no
    // package (grouping it yields vendor/lib_x/lib_x.js), and the same name
    // carried from a prior release arrives as "carry-over", so a nameSource
    // test would group on the second hop what it left flat on the first —
    // renaming every reference to those modules.
    const base = stripJsExtension(record.name);
    const grouped =
      !isHashFallbackName(record.name) &&
      (nameCounts.get(record.name) ?? 0) >= 2;
    const folder = grouped ? sanitizeFsPath(base) : "";
    const stem = grouped ? stableStem(record) : sanitizeFsName(base);
    const unique = uniqueInFolder(usedByFolder, folder, stem);
    return {
      fileName: folder ? `${folder}/${unique}` : unique,
      name: record.name,
      nameSource: record.nameSource,
      structuralHash: record.structuralHash
    };
  }
  // No classification record (regex-path extraction): the raw factory var
  // is minified residue more often than not — the shared filename floor
  // hashes it (never vendor/H.js).
  return {
    fileName: uniqueInFolder(
      usedByFolder,
      "",
      vendorStemFor(factoryVar, bodyText)
    ),
    name: factoryVar,
    nameSource: "fallback",
    structuralHash: ""
  };
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
