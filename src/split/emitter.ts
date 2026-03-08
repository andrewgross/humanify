import * as t from "@babel/types";
import _traverse from "@babel/traverse";
import _generate from "@babel/generator";
import type { SplitPlan, SplitLedgerEntry, ParsedFile } from "./types.js";

// Handle CJS default export interop
const traverse = (typeof _traverse === "function" ? _traverse : (_traverse as any).default) as typeof _traverse;
const generate = (typeof _generate === "function" ? _generate : (_generate as any).default) as typeof _generate;

/**
 * JS built-in globals that should not be treated as cross-file references.
 */
const JS_BUILTINS = new Set([
  "undefined", "null", "NaN", "Infinity", "arguments",
  "console", "Object", "Array", "Function", "String", "Number", "Boolean",
  "Symbol", "BigInt", "Date", "RegExp", "Error", "TypeError", "RangeError",
  "ReferenceError", "SyntaxError", "URIError", "EvalError",
  "Map", "Set", "WeakMap", "WeakSet", "Promise",
  "Proxy", "Reflect", "JSON", "Math",
  "parseInt", "parseFloat", "isNaN", "isFinite",
  "encodeURI", "encodeURIComponent", "decodeURI", "decodeURIComponent",
  "setTimeout", "setInterval", "clearTimeout", "clearInterval",
  "queueMicrotask", "requestAnimationFrame", "cancelAnimationFrame",
  "globalThis", "window", "self", "document", "navigator",
  "Event", "Node", "Element", "HTMLElement", "SVGElement",
  "DocumentFragment", "Text", "Comment",
  "MutationObserver", "IntersectionObserver", "ResizeObserver",
  "performance", "fetch", "AbortController", "AbortSignal",
  "URL", "URLSearchParams", "Headers", "Request", "Response",
  "FormData", "Blob", "File", "FileReader",
  "ArrayBuffer", "DataView", "Float32Array", "Float64Array",
  "Int8Array", "Int16Array", "Int32Array",
  "Uint8Array", "Uint16Array", "Uint32Array", "Uint8ClampedArray",
  "SharedArrayBuffer", "Atomics",
  "eval", "require", "module", "exports", "__dirname", "__filename",
  "process", "Buffer", "global",
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
      collectPatternNames(decl.id, names);
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

function collectPatternNames(pattern: t.LVal, names: string[]): void {
  if (t.isIdentifier(pattern)) {
    names.push(pattern.name);
  } else if (t.isArrayPattern(pattern)) {
    for (const el of pattern.elements) {
      if (el) collectPatternNames(el, names);
    }
  } else if (t.isObjectPattern(pattern)) {
    for (const prop of pattern.properties) {
      if (t.isObjectProperty(prop)) {
        collectPatternNames(prop.value as t.LVal, names);
      } else if (t.isRestElement(prop)) {
        collectPatternNames(prop.argument, names);
      }
    }
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
    Identifier(path) {
      const name = path.node.name;
      if (JS_BUILTINS.has(name)) return;

      // Skip binding definitions
      if (path.isBindingIdentifier()) return;

      // Skip property access keys (obj.prop — "prop" is not a reference)
      if (
        path.parentPath?.isObjectProperty({ key: path.node }) &&
        !path.parentPath.node.computed
      ) return;
      if (
        path.parentPath?.isMemberExpression({ property: path.node }) &&
        !path.parentPath.node.computed
      ) return;

      // Skip function name in function declaration (it's the binding)
      if (path.parentPath?.isFunctionDeclaration({ id: path.node })) return;

      refs.add(name);
    },
    noScope: true,
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
    const specifiers = names.map(n =>
      t.importSpecifier(t.identifier(n), t.identifier(n))
    );
    const decl = t.importDeclaration(specifiers, t.stringLiteral(`./${sourceFile}`));
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
  const specifiers = sorted.map(n =>
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
  const fileExports = new Map<string, Array<{ exported: string; local: string }>>();

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
    const decl = t.exportNamedDeclaration(null, specifiers, t.stringLiteral(`./${file}`));
    lines.push(generate(decl).code);
  }

  return lines.join("\n") + "\n";
}

/**
 * Build the contents of all output files from a split plan.
 * Returns Map<filename, fileContent>.
 */
export function buildFileContents(
  plan: SplitPlan,
  parsedFiles: ParsedFile[]
): Map<string, string> {
  // Build a map of all entries grouped by output file
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

  // Sort entries by source line within each file
  for (const entries of fileEntries.values()) {
    entries.sort((a, b) => {
      const aLine = a.node.loc?.start.line ?? 0;
      const bLine = b.node.loc?.start.line ?? 0;
      return aLine - bLine;
    });
  }

  // Build nameToFile: which names are defined in which output file
  const nameToFile = new Map<string, string>();
  for (const [fileName, entries] of fileEntries) {
    for (const entry of entries) {
      for (const name of extractDeclaredNames(entry.node)) {
        nameToFile.set(name, fileName);
      }
    }
  }

  // Build source map: filePath → source
  const sourceMap = new Map<string, string>();
  for (const pf of parsedFiles) {
    sourceMap.set(pf.filePath, pf.source);
  }

  const result = new Map<string, string>();

  // Generate each output file
  for (const [fileName, entries] of fileEntries) {
    // Collect names defined in this file
    const localNames = new Set<string>();
    for (const entry of entries) {
      for (const name of extractDeclaredNames(entry.node)) {
        localNames.add(name);
      }
    }

    // Collect all referenced names and figure out imports
    const imports = new Map<string, string[]>(); // sourceFile → names
    for (const entry of entries) {
      const refs = collectReferencedNames(entry.node);
      for (const ref of refs) {
        if (localNames.has(ref)) continue; // defined locally
        const fromFile = nameToFile.get(ref);
        if (!fromFile || fromFile === fileName) continue; // unknown or same file
        if (!imports.has(fromFile)) imports.set(fromFile, []);
        const names = imports.get(fromFile)!;
        if (!names.includes(ref)) names.push(ref);
      }
    }

    // Build file content
    const parts: string[] = [];

    // Imports
    const importBlock = generateImports(imports);
    if (importBlock) {
      parts.push(importBlock);
      parts.push("");
    }

    // Source-extracted code body
    for (const entry of entries) {
      const source = sourceMap.get(entry.source);
      if (!source) throw new Error(`Source not found for ${entry.source}`);
      parts.push(extractSourceRange(source, entry.node));
    }

    // Exports
    const exportedNames = Array.from(localNames);
    if (exportedNames.length > 0) {
      parts.push("");
      parts.push(generateExports(exportedNames));
    }

    result.set(fileName, parts.join("\n") + "\n");
  }

  // Generate barrel index.js
  if (barrelExportEntry && t.isExportNamedDeclaration(barrelExportEntry.node)) {
    const exportNode = barrelExportEntry.node;
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
    const indexContent = generateBarrelIndex(exportNames, nameToFile);
    if (indexContent.trim()) {
      result.set("index.js", indexContent);
    }
  }

  return result;
}
