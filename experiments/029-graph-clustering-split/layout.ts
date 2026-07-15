/**
 * Materialize the balanced clustered split to a real directory so the
 * folder/file LAYOUT can be browsed, and characterize the largest
 * statements (the "megastatement" that can't be split).
 *
 *   NODE_OPTIONS=--max-old-space-size=8192 tsx layout.ts 2.1.89 [outDir]
 *
 * NOTE: file/folder names here are MECHANICAL placeholders (d0_3/d1_1/
 * file_2.js) — LLM naming is the deferred next step. Judge the STRUCTURE
 * (sizes, nesting, grouping), not the names.
 */

import fs from "node:fs";
import path from "node:path";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { loadBeautified } from "./lib/io.js";
import { seamBalancedSplit } from "./lib/split.js";

function bodyOf(code: string): t.Statement[] {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error("no wrapper");
  const node = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(node)) throw new Error("body not block");
  return node.body;
}

function lineSpan(s: t.Statement): number {
  return s.loc ? s.loc.end.line - s.loc.start.line + 1 : 1;
}

/** Best-effort human label for what a top-level statement declares. */
function describe(s: t.Statement, code: string): string {
  let kind = s.type;
  let name = "";
  if (t.isVariableDeclaration(s)) {
    kind = `${s.kind} decl (${s.declarations.length})`;
    const d0 = s.declarations[0];
    if (t.isIdentifier(d0?.id)) name = d0.id.name;
    if (d0?.init) kind += ` = ${d0.init.type}`;
  } else if (t.isFunctionDeclaration(s)) {
    kind = "function";
    name = s.id?.name ?? "";
  } else if (t.isClassDeclaration(s)) {
    kind = "class";
    name = s.id?.name ?? "";
  } else if (t.isExpressionStatement(s)) {
    kind = `expr (${s.expression.type})`;
  }
  const head = (s.start != null ? code.slice(s.start, s.start + 90) : "")
    .replace(/\s+/g, " ")
    .trim();
  return `${kind}${name ? ` ${name}` : ""} — ${head}…`;
}

function main(): void {
  const version = process.argv[2] ?? "2.1.89";
  const outDir =
    process.argv[3] ?? `/Users/andrewgross/Development/exp029-sample-tree`;
  loadBeautified(version).then((code) => {
    const body = bodyOf(code);

    // 1. Largest statements — the megastatement.
    const ranked = body
      .map((s, i) => ({ i, lines: lineSpan(s), s }))
      .sort((a, b) => b.lines - a.lines)
      .slice(0, 8);
    console.log(`=== 8 largest top-level statements (of ${body.length}) ===`);
    for (const r of ranked) {
      console.log(
        `  ${String(r.lines).padStart(6)} lines  ${describe(r.s, code)}`
      );
    }

    // 2. Materialize the balanced split.
    const { fileContents, order } = seamBalancedSplit(code, body);
    fs.rmSync(outDir, { recursive: true, force: true });
    for (const [rel, content] of fileContents) {
      const abs = path.join(outDir, rel);
      fs.mkdirSync(path.dirname(abs), { recursive: true });
      fs.writeFileSync(abs, content);
    }
    console.log(`\n=== wrote ${fileContents.size} files to ${outDir} ===`);

    // 3. Which file holds the megastatement?
    const megaFile = order[ranked[0].i];
    console.log(
      `megastatement (${ranked[0].lines} lines) lives alone in: ${megaFile}`
    );

    // 4. Top-level folder overview.
    const top = new Map<string, { files: number; lines: number }>();
    for (const [rel, content] of fileContents) {
      const t0 = rel.split("/")[0];
      const e = top.get(t0) ?? { files: 0, lines: 0 };
      e.files++;
      e.lines += content.split("\n").length - 1;
      top.set(t0, e);
    }
    console.log(
      `\n=== ${top.size} top-level folders (file count, total lines) ===`
    );
    for (const [name, e] of [...top.entries()]
      .sort((a, b) => b[1].lines - a[1].lines)
      .slice(0, 15)) {
      console.log(
        `  ${name.padEnd(8)}  ${String(e.files).padStart(4)} files  ${String(e.lines).padStart(7)} lines`
      );
    }
    console.log(
      `\nBrowse it:  open ${outDir}   (or:  find ${outDir} | head -40)`
    );
  });
}

main();
