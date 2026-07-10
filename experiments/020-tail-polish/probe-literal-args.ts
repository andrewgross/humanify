/**
 * Sizing probe for preserving string-literal ARGUMENTS in the function
 * hash. Function-body hashing blurs strings to LENGTH
 * (`require("fs")` → `require(S=__STR_2__)`), so functions distinguished
 * only by a same-length literal arg (fs vs os vs vm) collide in one
 * ambiguous bucket. Binding INITS already preserve full literal value —
 * this gap is function bodies only.
 *
 * For each multi-member function bucket, this collects each member's
 * multiset of call-argument string literals (optionally only for a
 * callee allowlist) and reports how many members would move to a
 * SMALLER bucket if that multiset were appended to the hash key — the
 * population the change could crack.
 *
 * Usage:
 *   NODE_OPTIONS=--max-old-space-size=16384 npx tsx \
 *     experiments/020-tail-polish/probe-literal-args.ts <prepared.js> [--require-only]
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

const REQUIRE_LIKE = new Set(["require", "import"]);

/** Sorted multiset key of call-argument string literals in a function body. */
function argLiteralKey(
  fnPath: NodePath<t.Function>,
  requireOnly: boolean
): string {
  const lits: string[] = [];
  fnPath.traverse({
    CallExpression(call: NodePath<t.CallExpression>) {
      const callee = call.node.callee;
      const calleeName = t.isIdentifier(callee)
        ? callee.name
        : t.isImport(callee)
          ? "import"
          : null;
      if (requireOnly && !(calleeName && REQUIRE_LIKE.has(calleeName))) return;
      for (const arg of call.node.arguments) {
        if (t.isStringLiteral(arg))
          lits.push(`${calleeName ?? "?"}:${arg.value}`);
      }
    }
  });
  return lits.sort().join(",");
}

async function main(): Promise<void> {
  const inputPath = process.argv[2];
  const requireOnly = process.argv.includes("--require-only");
  const code = fs.readFileSync(inputPath, "utf-8");
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("parse failed");
  const isEligible = createIsEligible("bun", "bun");
  const graph = buildUnifiedGraph(ast, "input.js", undefined, isEligible, code);
  const wrapperNode = graph.wrapperPath?.node;

  const buckets = new Map<string, Array<{ id: string; argKey: string }>>();
  for (const [, node] of graph.nodes) {
    if (node.type !== "function") continue;
    const fn = node.node;
    if (fn.path.node === wrapperNode) continue;
    const hash = fn.fingerprint.structuralHash;
    const list = buckets.get(hash) ?? [];
    list.push({
      id: fn.sessionId,
      argKey: argLiteralKey(fn.path as NodePath<t.Function>, requireOnly)
    });
    buckets.set(hash, list);
  }

  let multiMembers = 0;
  let splittable = 0; // members whose bucket has >=2 distinct non-empty argKeys
  let nowUnique = 0; // members that become unique under (hash, argKey)
  const samples: string[] = [];
  for (const [hash, list] of buckets) {
    if (list.length < 2) continue;
    multiMembers += list.length;
    const distinct = new Set(list.map((m) => m.argKey));
    const nonEmpty = new Set([...distinct].filter((k) => k !== ""));
    if (nonEmpty.size < 2) continue; // literal args don't discriminate here
    const byArg = new Map<string, number>();
    for (const m of list) byArg.set(m.argKey, (byArg.get(m.argKey) ?? 0) + 1);
    for (const m of list) {
      if (m.argKey === "") continue;
      splittable++;
      if (byArg.get(m.argKey) === 1) {
        nowUnique++;
        if (samples.length < 8)
          samples.push(
            `${m.id} [${m.argKey}] (bucket ${hash} ×${list.length})`
          );
      }
    }
  }

  const scope = requireOnly
    ? "require/import args only"
    : "all call-arg strings";
  console.log(`scope: ${scope}`);
  console.log(
    `multi-member bucket members:       ${multiMembers.toLocaleString()}`
  );
  console.log(
    `  in buckets literal args split:   ${splittable.toLocaleString()}`
  );
  console.log(
    `  become UNIQUE under (hash,args):  ${nowUnique.toLocaleString()}`
  );
  console.log("\nsamples:");
  for (const s of samples) console.log(`  ${s}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
