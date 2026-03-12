import * as t from "@babel/types";
import { generate } from "../babel-utils.js";
import type { ParsedFile, SplitLedgerEntry, SplitPlan } from "./types.js";

// We need the full traverse with noScope support, which babel-utils' simplified type doesn't cover.
// Use the same ESM/CJS interop pattern but keep the callable type via import() and Function cast.
import * as babelTraverse from "@babel/traverse";
const traverse = (
  typeof babelTraverse.default === "function"
    ? babelTraverse.default
    : (babelTraverse.default as any).default
) as (node: t.Node, opts: Record<string, any>) => void;

/**
 * JS built-in globals that should not be treated as cross-file references.
 */
const JS_BUILTINS = new Set([
  "undefined",
  "null",
  "NaN",
  "Infinity",
  "arguments",
  "console",
  "Object",
  "Array",
  "Function",
  "String",
  "Number",
  "Boolean",
  "Symbol",
  "BigInt",
  "Date",
  "RegExp",
  "Error",
  "TypeError",
  "RangeError",
  "ReferenceError",
  "SyntaxError",
  "URIError",
  "EvalError",
  "Map",
  "Set",
  "WeakMap",
  "WeakSet",
  "Promise",
  "Proxy",
  "Reflect",
  "JSON",
  "Math",
  "parseInt",
  "parseFloat",
  "isNaN",
  "isFinite",
  "encodeURI",
  "encodeURIComponent",
  "decodeURI",
  "decodeURIComponent",
  "setTimeout",
  "setInterval",
  "clearTimeout",
  "clearInterval",
  "queueMicrotask",
  "requestAnimationFrame",
  "cancelAnimationFrame",
  "globalThis",
  "window",
  "self",
  "document",
  "navigator",
  "Event",
  "Node",
  "Element",
  "HTMLElement",
  "SVGElement",
  "DocumentFragment",
  "Text",
  "Comment",
  "MutationObserver",
  "IntersectionObserver",
  "ResizeObserver",
  "performance",
  "fetch",
  "AbortController",
  "AbortSignal",
  "URL",
  "URLSearchParams",
  "Headers",
  "Request",
  "Response",
  "FormData",
  "Blob",
  "File",
  "FileReader",
  "ArrayBuffer",
  "DataView",
  "Float32Array",
  "Float64Array",
  "Int8Array",
  "Int16Array",
  "Int32Array",
  "Uint8Array",
  "Uint16Array",
  "Uint32Array",
  "Uint8ClampedArray",
  "SharedArrayBuffer",
  "Atomics",
  "eval",
  "require",
  "module",
  "exports",
  "__dirname",
  "__filename",
  "process",
  "Buffer",
  "global"
]);

/**
 * Extract names declared by a top-level statement.
 */
export function extractDeclaredNames(node: t.Statement): string[] {
  const names: string[] = [];

  if (t.isFunctionDeclaration(node) && node.id) {
    names.push(node.id.name);
  } else if (t.isClassDeclaration(node) && node.id) {
    names.push(node.id.name);
  } else if (t.isVariableDeclaration(node)) {
    for (const decl of node.declarations) {
      collectPatternNames(decl.id as t.LVal, names);
    }
  } else if (t.isExportNamedDeclaration(node)) {
    if (node.declaration) {
      names.push(...extractDeclaredNames(node.declaration));
    }
  } else if (t.isExportDefaultDeclaration(node)) {
    names.push("default");
  }

  return names;
}

function collectObjectPatternNames(
  pattern: t.ObjectPattern,
  names: string[]
): void {
  for (const prop of pattern.properties) {
    if (t.isObjectProperty(prop)) {
      collectPatternNames(prop.value as t.LVal, names);
    } else if (t.isRestElement(prop)) {
      collectPatternNames(prop.argument, names);
    }
  }
}

function collectPatternNames(pattern: t.LVal, names: string[]): void {
  if (t.isIdentifier(pattern)) {
    names.push(pattern.name);
  } else if (t.isArrayPattern(pattern)) {
    for (const el of pattern.elements) {
      if (el) collectPatternNames(el as t.LVal, names);
    }
  } else if (t.isObjectPattern(pattern)) {
    collectObjectPatternNames(pattern, names);
  } else if (t.isAssignmentPattern(pattern)) {
    collectPatternNames(pattern.left, names);
  } else if (t.isRestElement(pattern)) {
    collectPatternNames(pattern.argument, names);
  }
}

/**
 * Collect all referenced identifier names in a statement's subtree.
 * Excludes binding sites (declarations) and JS builtins.
 */
export function collectReferencedNames(node: t.Statement): Set<string> {
  const refs = new Set<string>();

  // Wrap in a dummy program for traverse
  const program = t.program([node]);
  const file = t.file(program);

  traverse(file, {
    Identifier(path: any) {
      const name = path.node.name;
      if (JS_BUILTINS.has(name)) return;

      // Skip binding definitions
      if (path.isBindingIdentifier()) return;

      // Skip property access keys (obj.prop — "prop" is not a reference)
      if (
        path.parentPath?.isObjectProperty({ key: path.node }) &&
        !path.parentPath.node.computed
      )
        return;
      if (
        path.parentPath?.isMemberExpression({ property: path.node }) &&
        !path.parentPath.node.computed
      )
        return;

      // Skip function name in function declaration (it's the binding)
      if (path.parentPath?.isFunctionDeclaration({ id: path.node })) return;

      refs.add(name);
    },
    noScope: true
  });

  return refs;
}

/**
 * Extract source text for a statement using its location info.
 */
export function extractSourceRange(source: string, node: t.Statement): string {
  if (!node.loc) {
    throw new Error(`Node missing location info: ${node.type}`);
  }
  const lines = source.split("\n");
  // loc lines are 1-based
  const startLine = node.loc.start.line - 1;
  const endLine = node.loc.end.line; // exclusive for slice
  return lines.slice(startLine, endLine).join("\n");
}

/**
 * Generate import declarations grouped by source file.
 * refs: Map<sourceFile, names[]>
 */
export function generateImports(refs: Map<string, string[]>): string {
  const lines: string[] = [];
  const sortedFiles = Array.from(refs.keys()).sort();
  for (const sourceFile of sortedFiles) {
    const names = refs.get(sourceFile)!.sort();
    const specifiers = names.map((n) =>
      t.importSpecifier(t.identifier(n), t.identifier(n))
    );
    const decl = t.importDeclaration(
      specifiers,
      t.stringLiteral(`./${sourceFile}`)
    );
    lines.push(generate(decl).code);
  }
  return lines.join("\n");
}

/**
 * Generate a named export declaration for a list of names.
 */
export function generateExports(names: string[]): string {
  if (names.length === 0) return "";
  const sorted = [...names].sort();
  const specifiers = sorted.map((n) =>
    t.exportSpecifier(t.identifier(n), t.identifier(n))
  );
  const decl = t.exportNamedDeclaration(null, specifiers);
  return generate(decl).code;
}

/**
 * Generate a barrel index.js that re-exports names from the correct files.
 */
export function generateBarrelIndex(
  exportNames: Array<{ exported: string; local: string }>,
  nameToFile: Map<string, string>
): string {
  // Group exports by source file
  const fileExports = new Map<
    string,
    Array<{ exported: string; local: string }>
  >();

  for (const { exported, local } of exportNames) {
    const file = nameToFile.get(local);
    if (!file) continue;
    if (!fileExports.has(file)) fileExports.set(file, []);
    fileExports.get(file)!.push({ exported, local });
  }

  const lines: string[] = [];
  const sortedFiles = Array.from(fileExports.keys()).sort();

  for (const file of sortedFiles) {
    const names = fileExports.get(file)!;
    const specifiers = names.map(({ exported, local }) =>
      t.exportSpecifier(t.identifier(local), t.identifier(exported))
    );
    const decl = t.exportNamedDeclaration(
      null,
      specifiers,
      t.stringLiteral(`./${file}`)
    );
    lines.push(generate(decl).code);
  }

  return lines.join("\n") + "\n";
}

/** Group ledger entries by output file, separating the barrel export entry. */
function groupEntriesByFile(plan: SplitPlan): {
  fileEntries: Map<string, SplitLedgerEntry[]>;
  barrelExportEntry: SplitLedgerEntry | undefined;
} {
  const fileEntries = new Map<string, SplitLedgerEntry[]>();
  let barrelExportEntry: SplitLedgerEntry | undefined;

  for (const entry of plan.ledger.entries.values()) {
    if (entry.outputFile === "index.js") {
      barrelExportEntry = entry;
      continue;
    }
    const file = entry.outputFile!;
    if (!fileEntries.has(file)) fileEntries.set(file, []);
    fileEntries.get(file)!.push(entry);
  }

  for (const entries of fileEntries.values()) {
    entries.sort((a, b) => {
      const aLine = a.node.loc?.start.line ?? 0;
      const bLine = b.node.loc?.start.line ?? 0;
      return aLine - bLine;
    });
  }

  return { fileEntries, barrelExportEntry };
}

/** Add a single reference to the cross-file import map if it is from another file. */
function addCrossFileRef(
  ref: string,
  fileName: string,
  localNames: Set<string>,
  nameToFile: Map<string, string>,
  imports: Map<string, string[]>
): void {
  if (localNames.has(ref)) return;
  const fromFile = nameToFile.get(ref);
  if (!fromFile || fromFile === fileName) return;
  if (!imports.has(fromFile)) imports.set(fromFile, []);
  const names = imports.get(fromFile)!;
  if (!names.includes(ref)) names.push(ref);
}

/** Build the cross-file import map for a single file's entries. */
function buildFileImports(
  fileName: string,
  entries: SplitLedgerEntry[],
  localNames: Set<string>,
  nameToFile: Map<string, string>
): Map<string, string[]> {
  const imports = new Map<string, string[]>();
  for (const entry of entries) {
    const refs = collectReferencedNames(entry.node);
    for (const ref of refs) {
      addCrossFileRef(ref, fileName, localNames, nameToFile, imports);
    }
  }
  return imports;
}

/** Collect local names and cross-file imports for all files. */
function collectLocalNamesAndImports(
  fileEntries: Map<string, SplitLedgerEntry[]>,
  nameToFile: Map<string, string>
): {
  fileLocalNames: Map<string, Set<string>>;
  fileImports: Map<string, Map<string, string[]>>;
} {
  const fileLocalNames = new Map<string, Set<string>>();
  const fileImports = new Map<string, Map<string, string[]>>();

  for (const [fileName, entries] of fileEntries) {
    const localNames = new Set<string>();
    for (const entry of entries) {
      for (const name of extractDeclaredNames(entry.node)) {
        localNames.add(name);
      }
    }
    fileLocalNames.set(fileName, localNames);
    fileImports.set(
      fileName,
      buildFileImports(fileName, entries, localNames, nameToFile)
    );
  }

  return { fileLocalNames, fileImports };
}

/** Build set of all names imported by any file. */
function buildImportedByOthers(
  fileImports: Map<string, Map<string, string[]>>
): Set<string> {
  const importedByOthers = new Set<string>();
  for (const imports of fileImports.values()) {
    for (const names of imports.values()) {
      for (const name of names) {
        importedByOthers.add(name);
      }
    }
  }
  return importedByOthers;
}

/** Build content for a single output file. */
function buildSingleFileContent(
  fileName: string,
  entries: SplitLedgerEntry[],
  localNames: Set<string>,
  imports: Map<string, string[]>,
  sourceMap: Map<string, string>,
  importedByOthers: Set<string>,
  barrelLocalNames: Set<string>
): string {
  const parts: string[] = [];

  const importBlock = generateImports(imports);
  if (importBlock) {
    parts.push(importBlock);
    parts.push("");
  }

  for (const entry of entries) {
    const source = sourceMap.get(entry.source);
    if (!source) throw new Error(`Source not found for ${entry.source}`);
    parts.push(extractSourceRange(source, entry.node));
  }

  const exportedNames = Array.from(localNames).filter(
    (name) => importedByOthers.has(name) || barrelLocalNames.has(name)
  );
  if (exportedNames.length > 0) {
    parts.push("");
    parts.push(generateExports(exportedNames));
  }

  return parts.join("\n") + "\n";
}

/** Extract barrel export names from an export declaration. */
function extractBarrelExportNames(
  exportNode: t.ExportNamedDeclaration
): Array<{ exported: string; local: string }> {
  const exportNames: Array<{ exported: string; local: string }> = [];
  for (const spec of exportNode.specifiers) {
    if (t.isExportSpecifier(spec)) {
      const exported = t.isIdentifier(spec.exported)
        ? spec.exported.name
        : spec.exported.value;
      const local = spec.local.name;
      exportNames.push({ exported, local });
    }
  }
  return exportNames;
}

/** Build nameToFile map: declared name → output file. */
function buildNameToFile(
  fileEntries: Map<string, SplitLedgerEntry[]>
): Map<string, string> {
  const nameToFile = new Map<string, string>();
  for (const [fileName, entries] of fileEntries) {
    for (const entry of entries) {
      for (const name of extractDeclaredNames(entry.node)) {
        nameToFile.set(name, fileName);
      }
    }
  }
  return nameToFile;
}

/** Collect barrel local names from a barrel export entry. */
function collectBarrelLocalNames(
  barrelExportEntry: SplitLedgerEntry | undefined
): Set<string> {
  const barrelLocalNames = new Set<string>();
  if (barrelExportEntry && t.isExportNamedDeclaration(barrelExportEntry.node)) {
    for (const spec of barrelExportEntry.node.specifiers) {
      if (t.isExportSpecifier(spec)) {
        barrelLocalNames.add(spec.local.name);
      }
    }
  }
  return barrelLocalNames;
}

/**
 * Build the contents of all output files from a split plan.
 * Returns Map<filename, fileContent>.
 */
export function buildFileContents(
  plan: SplitPlan,
  parsedFiles: ParsedFile[]
): Map<string, string> {
  const { fileEntries, barrelExportEntry } = groupEntriesByFile(plan);
  const nameToFile = buildNameToFile(fileEntries);
  const barrelLocalNames = collectBarrelLocalNames(barrelExportEntry);

  const sourceMap = new Map<string, string>();
  for (const pf of parsedFiles) {
    sourceMap.set(pf.filePath, pf.source);
  }

  const { fileLocalNames, fileImports } = collectLocalNamesAndImports(
    fileEntries,
    nameToFile
  );
  const importedByOthers = buildImportedByOthers(fileImports);

  const result = new Map<string, string>();

  for (const [fileName, entries] of fileEntries) {
    result.set(
      fileName,
      buildSingleFileContent(
        fileName,
        entries,
        fileLocalNames.get(fileName)!,
        fileImports.get(fileName)!,
        sourceMap,
        importedByOthers,
        barrelLocalNames
      )
    );
  }

  // Generate barrel index.js
  if (barrelExportEntry && t.isExportNamedDeclaration(barrelExportEntry.node)) {
    const exportNames = extractBarrelExportNames(barrelExportEntry.node);
    const indexContent = generateBarrelIndex(exportNames, nameToFile);
    if (indexContent.trim()) {
      result.set("index.js", indexContent);
    }
  }

  return result;
}
