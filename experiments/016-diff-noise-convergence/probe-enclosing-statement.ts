/**
 * GO/NO-GO probe for cracking KEYLESS ambiguous-bucket clones by their
 * ENCLOSING STATEMENT's rename-invariant hash.
 *
 * The still-ambiguous reservoir after memberKey is keyless (identity
 * arrows in argument/array positions, bare declarators). Their bucket
 * content is identical by definition, but the statement AROUND them
 * (`inputFilterSensitiveLog: q => q` inside a distinctive options
 * object) often is not. hashPathWithMapping on the nearest Statement
 * ancestor is exactly statement-align's normalization — if members are
 * unique under (bucketHash, enclosingStmtHash), a cross-version pairing
 * on that key is precision-strong.
 *
 * Usage:
 *   NODE_OPTIONS=--max-old-space-size=16384 npx tsx \
 *     experiments/016-diff-noise-convergence/probe-enclosing-statement.ts \
 *     <prepared-runtime.js>
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { hashPathWithMapping } from "../../src/analysis/structural-hash.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

async function main(): Promise<void> {
  const inputPath = process.argv[2];
  const code = fs.readFileSync(inputPath, "utf-8");
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("parse failed");
  const isEligible = createIsEligible("bun", "bun");
  const graph = buildUnifiedGraph(ast, "input.js", undefined, isEligible, code);
  const wrapperNode = graph.wrapperPath?.node;

  const buckets = new Map<
    string,
    Array<{ id: string; stmtHash: string | null }>
  >();
  for (const [, node] of graph.nodes) {
    if (node.type !== "function") continue;
    const fn = node.node;
    if (fn.path.node === wrapperNode) continue;
    const hash = fn.fingerprint.structuralHash;
    const list = buckets.get(hash) ?? [];
    list.push({ id: fn.sessionId, stmtHash: null });
    buckets.set(hash, list);
  }

  // Hash enclosing statements only for multi-member buckets (cost control).
  const wanted = new Map<string, { id: string; stmtHash: string | null }>();
  for (const [, list] of buckets) {
    if (list.length < 2) continue;
    for (const m of list) wanted.set(m.id, m);
  }
  for (const [, node] of graph.nodes) {
    if (node.type !== "function") continue;
    const fn = node.node;
    const member = wanted.get(fn.sessionId);
    if (!member) continue;
    const stmt = fn.path.getStatementParent();
    if (!stmt || t.isProgram(stmt.node)) continue;
    try {
      member.stmtHash = hashPathWithMapping(stmt).hash;
    } catch {
      // unhashable statement — leave null
    }
  }

  let members = 0;
  let withStmt = 0;
  let uniqueUnderStmt = 0;
  const samples: string[] = [];
  for (const [hash, list] of buckets) {
    if (list.length < 2) continue;
    members += list.length;
    const byStmt = new Map<string, number>();
    for (const m of list) {
      if (m.stmtHash) {
        withStmt++;
        byStmt.set(m.stmtHash, (byStmt.get(m.stmtHash) ?? 0) + 1);
      }
    }
    for (const m of list) {
      if (m.stmtHash && byStmt.get(m.stmtHash) === 1) {
        uniqueUnderStmt++;
        if (samples.length < 6)
          samples.push(`${m.id} (bucket ${hash} ×${list.length})`);
      }
    }
  }

  console.log(`bucket members:                    ${members.toLocaleString()}`);
  console.log(
    `with hashable enclosing statement: ${withStmt.toLocaleString()}`
  );
  console.log(
    `UNIQUE under (hash, stmtHash):     ${uniqueUnderStmt.toLocaleString()} (${((100 * uniqueUnderStmt) / Math.max(1, members)).toFixed(1)}% of members)`
  );
  console.log("\nsamples:");
  for (const s of samples) console.log(`  ${s}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
