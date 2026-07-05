/**
 * Inspect where bang banners attach in the parsed AST.
 *
 * Usage:
 *   node --max-old-space-size=8192 --import tsx/esm \
 *     experiments/013-bun-cjs-classification/inspect-banners.ts <bundle.js>
 */

import { readFileSync } from "node:fs";
import { parseSync } from "@babel/core";

const src = readFileSync(process.argv[2], "utf-8");
const ast = parseSync(src, {
  sourceType: "unambiguous",
  parserOpts: { errorRecovery: true }
});
if (!ast || ast.type !== "File") throw new Error("parse failed");

let found = 0;
const MAX = 50;
function visit(
  node: unknown,
  parent: unknown,
  key: string,
  path: string[]
): void {
  if (found >= MAX) return;
  if (Array.isArray(node)) {
    node.forEach((n, i) =>
      visit(n, node, String(i), [...path, `${key}[${i}]`])
    );
    return;
  }
  if (!node || typeof node !== "object") return;
  const n = node as Record<string, unknown>;
  if (
    n.type === "CommentBlock" &&
    typeof n.value === "string" &&
    n.value.startsWith("!")
  ) {
    const parentType = (parent as { type?: string } | null)?.type ?? "?";
    const loc = (n.loc as { start?: { line: number } } | undefined)?.start
      ?.line;
    console.log(
      `\n# Banner @${loc}: ${(n.value as string).trim().slice(0, 60)}`
    );
    console.log(`  parent: ${parentType}.${key}`);
    console.log(`  path:   ${path.slice(-4).join(" > ")}`);
    found++;
  }
  for (const k of Object.keys(n)) {
    if (k === "loc" || k === "start" || k === "end" || k === "extra") continue;
    visit(n[k], n, k, [...path, k]);
  }
}
visit(ast, null, "", []);

if (found === 0) {
  console.log("No banner-style block comments found in the AST.");
}
