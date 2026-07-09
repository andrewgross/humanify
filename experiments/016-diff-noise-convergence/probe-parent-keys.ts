/**
 * Sizing probe for ambiguous-bucket cracking via PARENT-CONTEXT evidence.
 *
 * The reservoir: thousands of structurally identical functions (q => q
 * identity arrows, thunks, schema builders) share hash buckets that no
 * current strategy cracks — both legs fresh-name them every run. Their
 * distinguishing identity is often WHERE they sit: the object-property
 * key, class-method name, or assignment target they're attached to
 * (`inputFilterSensitiveLog: q => q`). Property keys are rename-invariant
 * (hash CONTENT elsewhere in the pipeline), so (bucketHash, parentKey)
 * can pair members across versions.
 *
 * This probe measures, per multi-member hash bucket: how many members
 * carry a parent key, and how many are UNIQUE under (hash, parentKey) —
 * the population a parent-key resolver could crack.
 *
 * Usage:
 *   NODE_OPTIONS=--max-old-space-size=16384 npx tsx \
 *     experiments/016-diff-noise-convergence/probe-parent-keys.ts \
 *     <prepared-runtime.js>
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

/** Rename-invariant parent context key for a function node, or null. */
function parentKey(path: NodePath): string | null {
  const parent = path.parentPath;
  if (!parent) return null;
  const node = parent.node;
  if (t.isObjectProperty(node) && !node.computed) {
    if (t.isIdentifier(node.key)) return `prop:${node.key.name}`;
    if (t.isStringLiteral(node.key)) return `prop:${node.key.value}`;
  }
  if (t.isClassMethod(node) && !node.computed && t.isIdentifier(node.key)) {
    return `method:${node.key.name}`;
  }
  if (t.isCallExpression(node)) {
    const callee = node.callee;
    // fn passed as argument: key on the callee's PROPERTY name (rename-
    // invariant) when it is a member call like router.get(...)
    if (t.isMemberExpression(callee) && t.isIdentifier(callee.property)) {
      const idx = node.arguments.indexOf(
        path.node as t.ArgumentPlaceholder & t.Expression
      );
      return `arg:${callee.property.name}@${idx}`;
    }
  }
  if (t.isAssignmentExpression(node) && t.isMemberExpression(node.left)) {
    const prop = node.left.property;
    if (!node.left.computed && t.isIdentifier(prop)) {
      return `assign:${prop.name}`;
    }
  }
  return null;
}

async function main(): Promise<void> {
  const inputPath = process.argv[2];
  const code = fs.readFileSync(inputPath, "utf-8");
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("parse failed");
  const isEligible = createIsEligible("bun", "bun");
  const graph = buildUnifiedGraph(ast, "input.js", undefined, isEligible, code);
  const wrapperNode = graph.wrapperPath?.node;

  // hash → members
  const buckets = new Map<string, Array<{ id: string; key: string | null }>>();
  for (const [, node] of graph.nodes) {
    if (node.type !== "function") continue;
    const fn = node.node;
    if (fn.path.node === wrapperNode) continue;
    const hash = fn.fingerprint.structuralHash;
    const list = buckets.get(hash) ?? [];
    list.push({ id: fn.sessionId, key: parentKey(fn.path) });
    buckets.set(hash, list);
  }

  let multiBuckets = 0;
  let members = 0;
  let withKey = 0;
  let uniqueUnderKey = 0;
  let argOnlyUnique = 0;
  const keyedSamples: string[] = [];
  for (const [hash, list] of buckets) {
    if (list.length < 2) continue;
    multiBuckets++;
    members += list.length;
    const byKey = new Map<string, number>();
    for (const m of list) {
      if (m.key) {
        withKey++;
        byKey.set(m.key, (byKey.get(m.key) ?? 0) + 1);
      }
    }
    for (const m of list) {
      if (m.key && byKey.get(m.key) === 1) {
        uniqueUnderKey++;
        if (m.key.startsWith("arg:")) argOnlyUnique++;
        if (keyedSamples.length < 8)
          keyedSamples.push(
            `${m.id} ${m.key} (bucket ${hash} ×${list.length})`
          );
      }
    }
  }

  console.log(`multi-member buckets:         ${multiBuckets.toLocaleString()}`);
  console.log(`members in them:              ${members.toLocaleString()}`);
  console.log(`members with a parent key:    ${withKey.toLocaleString()}`);
  console.log(
    `UNIQUE under (hash,parentKey): ${uniqueUnderKey.toLocaleString()} (${((100 * uniqueUnderKey) / Math.max(1, members)).toFixed(1)}% of bucket members)`
  );
  console.log(
    `  of which arg:* (NEW evidence beyond memberKey): ${argOnlyUnique.toLocaleString()}`
  );
  console.log("\nsamples:");
  for (const s of keyedSamples) console.log(`  ${s}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
