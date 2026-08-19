/**
 * 080 — MINIFIED FREE VARIABLES: a noise mechanism no census counted.
 *
 *   npx tsx --max-old-space-size=32768 \
 *     experiments/080-noise-sources/free-variable-census.ts <treeRoot> [maxFiles]
 *
 * Found from one line Andrew picked out of a calm release:
 *
 *   -  childModule = moduleObjectVal && typeof bLn == "object" && bLn && ...
 *   +  childModule = moduleObjectVal && typeof ELn == "object" && ELn && ...
 *
 * `bLn` is declared nowhere in that file. It is a FREE reference the bundler
 * renamed — originally `module`, in lodash's freeModule idiom. Renaming works
 * on resolved BINDINGS, so a reference with no binding is invisible to it, and
 * `mintedCensus` counts bindings too, so it is invisible there as well. The
 * minifier picks different letters each release and the line churns forever.
 *
 * This counts them: program-level unresolved references that look minified.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync, traverse } from "@babel/core";
import type * as t from "@babel/types";

const [ROOT, MAX = "1500"] = process.argv.slice(2);
if (!ROOT) {
  console.error("usage: free-variable-census.ts <treeRoot> [maxFiles]");
  process.exit(1);
}

/** Globals a bundled CJS file legitimately references. */
const KNOWN = new Set([
  "require",
  "module",
  "exports",
  "process",
  "console",
  "globalThis",
  "global",
  "window",
  "self",
  "document",
  "Buffer",
  "URL",
  "URLSearchParams",
  "TextEncoder",
  "TextDecoder",
  "setTimeout",
  "clearTimeout",
  "setInterval",
  "clearInterval",
  "setImmediate",
  "queueMicrotask",
  "fetch",
  "Headers",
  "Request",
  "Response",
  "AbortController",
  "AbortSignal",
  "Event",
  "EventTarget",
  "WebSocket",
  "performance",
  "structuredClone",
  "Object",
  "Array",
  "String",
  "Number",
  "Boolean",
  "Symbol",
  "BigInt",
  "Math",
  "JSON",
  "Date",
  "RegExp",
  "Error",
  "TypeError",
  "RangeError",
  "SyntaxError",
  "ReferenceError",
  "EvalError",
  "URIError",
  "AggregateError",
  "Promise",
  "Proxy",
  "Reflect",
  "Map",
  "Set",
  "WeakMap",
  "WeakSet",
  "WeakRef",
  "FinalizationRegistry",
  "ArrayBuffer",
  "SharedArrayBuffer",
  "DataView",
  "Int8Array",
  "Uint8Array",
  "Uint8ClampedArray",
  "Int16Array",
  "Uint16Array",
  "Int32Array",
  "Uint32Array",
  "Float32Array",
  "Float64Array",
  "BigInt64Array",
  "BigUint64Array",
  "Atomics",
  "Intl",
  "WebAssembly",
  "escape",
  "unescape",
  "encodeURI",
  "decodeURI",
  "encodeURIComponent",
  "decodeURIComponent",
  "parseInt",
  "parseFloat",
  "isNaN",
  "isFinite",
  "NaN",
  "Infinity",
  "undefined",
  "arguments",
  "eval",
  "__dirname",
  "__filename",
  "Function",
  "Bun",
  "Deno",
  "btoa",
  "atob",
  "crypto",
  "Blob",
  "File",
  "FormData",
  "ReadableStream",
  "WritableStream",
  "TransformStream",
  "MessageChannel",
  "MessagePort",
  "Worker",
  "navigator"
]);

/**
 * Does this look like a minifier's output rather than a name a human wrote?
 * Short, or camel-ish with no lowercase word boundary, or trailing digits.
 */
function looksMinified(name: string): boolean {
  if (KNOWN.has(name)) return false;
  if (name.length <= 3) return true;
  if (/^[A-Za-z]{1,3}[0-9]{0,3}$/.test(name)) return true;
  // bLn, ELn, qi_16 — no vowel-rich word, mixed case with no clear boundary
  if (name.length <= 6 && !/[aeiou]{1}[a-z]{2,}/.test(name)) return true;
  return false;
}

function walk(dir: string, out: string[], limit: number): void {
  if (out.length >= limit) return;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (out.length >= limit) return;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out, limit);
    else if (e.name.endsWith(".js")) out.push(p);
  }
}

const files: string[] = [];
walk(path.join(ROOT, "src"), files, Number(MAX));

let parsed = 0;
let filesWithFree = 0;
const byName = new Map<string, number>();
const allUnresolved = new Map<string, number>();
const perFile: { file: string; names: string[] }[] = [];

for (const file of files) {
  let ast: t.File | null = null;
  try {
    ast = parseSync(fs.readFileSync(file, "utf8"), {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    }) as t.File;
  } catch {
    continue;
  }
  if (!ast) continue;
  parsed++;
  const found = new Set<string>();
  traverse(ast, {
    Program(p) {
      for (const name of Object.keys(p.scope.globals)) {
        if (KNOWN.has(name)) continue;
        allUnresolved.set(name, (allUnresolved.get(name) ?? 0) + 1);
        if (looksMinified(name)) found.add(name);
      }
      p.stop();
    }
  });
  if (found.size > 0) {
    filesWithFree++;
    perFile.push({ file: path.relative(ROOT, file), names: [...found] });
    for (const n of found) byName.set(n, (byName.get(n) ?? 0) + 1);
  }
}

console.log(`census over ${parsed} files of ${ROOT}\n`);
console.log(
  `files with a minified FREE variable: ${filesWithFree} (${((100 * filesWithFree) / parsed).toFixed(1)}%)`
);
console.log(`distinct minified free names:        ${byName.size}`);
console.log(
  `total (name, file) sites:            ${[...byName.values()].reduce((a, b) => a + b, 0)}\n`
);
console.log("most common:");
for (const [n, c] of [...byName.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 15)) {
  console.log(`  ${n.padEnd(12)} ${c} file(s)`);
}
console.log(
  `\nALL unresolved non-global names (filter check): ${allUnresolved.size} distinct`
);
for (const [n, c] of [...allUnresolved.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 12)) {
  console.log(`  ${n.padEnd(28)} ${c} file(s)`);
}
console.log("\nexamples:");
for (const e of perFile.slice(0, 8)) {
  console.log(`  ${e.file}: ${e.names.join(", ")}`);
}
console.log(
  "\nThese have NO BINDING, so the renamer cannot reach them and mintedCensus\n" +
    "does not count them. The minifier redraws the letters each release."
);
