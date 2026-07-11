/**
 * Exp023 v0 prototype: name-carried split-assignment stability, measured.
 *
 * Splits a humanified bundle's wrapper-body statements into files. With no
 * prior ledger, groups by adjacency chunks (placeholder grouping — quality
 * comes later; this prototype measures STABILITY only). With a prior
 * ledger, each statement inherits the file its declared names had in the
 * prior release (unanimous vote); statements with no usable prior follow
 * their preceding statement (locality default), so genuinely-new code
 * lands next to its neighbors.
 *
 * Emits naive per-file concatenations (no imports — NOT runnable; churn
 * measurement only) plus a _ledger.json the next release consumes.
 *
 *   npx tsx proto-stable-split.ts <humanified.js> <outDir> [--prior <ledger.json>]
 */
import fs from "node:fs";
import path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { generate } from "../../src/babel-utils.js";

const CHUNK = 200;

interface Ledger {
  nameToFile: Record<string, string>;
  files: string[];
}

function arg(name: string): string | undefined {
  const idx = process.argv.indexOf(`--${name}`);
  return idx === -1 ? undefined : process.argv[idx + 1];
}

function wrapperBody(file: string): t.Statement[] {
  const ast = parseSync(fs.readFileSync(file, "utf-8"), {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast || ast.type !== "File") throw new Error(`parse failed: ${file}`);
  const first = ast.program.body[0];
  if (!t.isExpressionStatement(first)) throw new Error("no wrapper");
  let expr = first.expression;
  if (t.isCallExpression(expr)) expr = expr.callee as t.Expression;
  if (!t.isFunctionExpression(expr) || !t.isBlockStatement(expr.body))
    throw new Error("no wrapper fn");
  return expr.body.body;
}

function declaredNames(stmt: t.Statement): string[] {
  return Object.keys(t.getBindingIdentifiers(stmt, false));
}

/** Inherit the unanimous prior file of the statement's declared names. */
function priorFileFor(
  names: string[],
  prior: Ledger
): { file?: string; conflict: boolean } {
  const files = new Set<string>();
  for (const n of names) {
    const f = prior.nameToFile[n];
    if (f) files.add(f);
  }
  if (files.size === 1) return { file: [...files][0], conflict: false };
  return { conflict: files.size > 1 };
}

function main(): void {
  const [input, outDir] = process.argv.slice(2);
  const priorPath = arg("prior");
  const body = wrapperBody(input);
  const prior: Ledger | null = priorPath
    ? JSON.parse(fs.readFileSync(priorPath, "utf-8"))
    : null;

  const assignment: string[] = new Array(body.length);
  let inherited = 0;
  let residueLocality = 0;
  let conflicts = 0;
  let freshFiles = 0;

  for (let i = 0; i < body.length; i++) {
    const names = declaredNames(body[i]);
    if (prior) {
      const hit = priorFileFor(names, prior);
      if (hit.file) {
        assignment[i] = hit.file;
        inherited++;
        continue;
      }
      if (hit.conflict) conflicts++;
      // Residue: follow the preceding statement (locality), else a fresh
      // chunk file for a leading run of new code.
      if (i > 0) {
        assignment[i] = assignment[i - 1];
        residueLocality++;
      } else {
        assignment[i] = "new_000.js";
        freshFiles++;
      }
    } else {
      assignment[i] =
        `chunk_${String(Math.floor(i / CHUNK)).padStart(3, "0")}.js`;
    }
  }

  // Emit naive per-file contents + ledger.
  fs.mkdirSync(outDir, { recursive: true });
  const byFile = new Map<string, string[]>();
  const nameToFile: Record<string, string> = {};
  let redeclared = 0;
  for (let i = 0; i < body.length; i++) {
    const file = assignment[i];
    if (!byFile.has(file)) byFile.set(file, []);
    byFile.get(file)?.push(generate(body[i], { compact: false }).code);
    for (const n of declaredNames(body[i])) {
      if (nameToFile[n] && nameToFile[n] !== file) redeclared++;
      nameToFile[n] = file;
    }
  }
  for (const [file, parts] of byFile) {
    fs.writeFileSync(path.join(outDir, file), `${parts.join("\n")}\n`);
  }
  const ledger: Ledger = { nameToFile, files: [...byFile.keys()].sort() };
  fs.writeFileSync(
    path.join(outDir, "_ledger.json"),
    JSON.stringify(ledger, null, 1)
  );

  console.log(
    JSON.stringify(
      {
        statements: body.length,
        files: byFile.size,
        mode: prior ? "prior-carried" : "fresh-chunks",
        inherited,
        residueLocality,
        conflicts,
        freshFiles,
        crossFileRedeclarations: redeclared
      },
      null,
      2
    )
  );
}

main();
