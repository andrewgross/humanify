// WP4.3 probe: the graph-build-time TEXT the naming waves read, from the
// REAL TS (`buildUnifiedGraph` over a beautified text, the pipeline's own
// call) plus @babel/generator's output for the node kinds the prompts
// print — the ground truth for the Rust babel-printer emulation
// (`naming::waves::generate`) and the naming graph's extras
// (`naming::waves::graph`).
//
// Rows (JSONL, stdout):
//   {k:"order", ids}                       graph.nodes key order
//   {k:"fn", id, start, end, callees, scopeParent, deps, callSites,
//    code?, codeSha, body?, bodySha, params}  one per function node
//   {k:"mb", id, line, declaration, assignments, usages, deps}
//   {k:"scopeEdges", edges}
// `code` / `body` are babel's generate() of the function node / its body
// (compact:false, comments:false for the body — context-builder.ts); the
// full text is written only under --full, else its sha256 (bundle scale).
//
// Run: npx tsx test/parity/wp43-gen-probe.ts <text.js> [--full] [--only=<ids>] [--prior]
//   --prior: the PRIOR side (every function's generate() only — the
//            close-match prior context, prior-version.ts applyCloseMatches).
import { createHash } from "node:crypto";
import fs from "node:fs";
import * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { generate, parseSourceAst } from "../../src/babel-utils.js";
import { NULL_PROFILER } from "../../src/profiling/profiler.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

const args = process.argv.slice(2);
const file = args.find((a) => !a.startsWith("--"));
if (!file) throw new Error("usage: wp43-gen-probe.ts <text.js> [--full]");
const full = args.includes("--full");
// --only=<id,id,...>: the full text for those rows only (bisection).
const only = new Set(
  (args.find((a) => a.startsWith("--only="))?.slice(7) ?? "")
    .split(",")
    .filter(Boolean)
);
const code = fs.readFileSync(file, "utf8");
const ast = parseSourceAst(code);
if (!ast) throw new Error("parse failed");
const sha = (s: string) => createHash("sha256").update(s).digest("hex");
const out = (row: unknown) => process.stdout.write(`${JSON.stringify(row)}\n`);
const gen = (node: t.Node, opts?: object) => {
  try {
    return generate(node, opts).code;
  } catch {
    return "[code generation failed]";
  }
};
const MAX = 400_000;

const graph = buildUnifiedGraph(
  ast,
  "input.js",
  NULL_PROFILER,
  createIsEligible("bun", "bun"),
  code
);
out({ k: "order", ids: [...graph.nodes.keys()] });
out({ k: "scopeEdges", edges: [...graph.scopeParentEdges] });
for (const [id, rn] of graph.nodes) {
  const deps = [...(graph.dependencies.get(id) ?? [])];
  if (rn.type === "function") {
    const fn = rn.node;
    const node = fn.path.node;
    const size = (node.end ?? 0) - (node.start ?? 0);
    const fnCode = size <= MAX ? gen(node) : null;
    const body =
      size <= MAX ? gen(node.body, { compact: false, comments: false }) : null;
    const params = node.params
      .filter(
        (p) =>
          !t.isIdentifier(p) &&
          !(t.isRestElement(p) && t.isIdentifier(p.argument)) &&
          !(t.isAssignmentPattern(p) && t.isIdentifier(p.left))
      )
      .map((p) => gen(p, { compact: false, comments: false }));
    out({
      k: "fn",
      id,
      start: node.start,
      end: node.end,
      callees: [...fn.internalCallees].map((c) => c.sessionId),
      scopeParent: fn.scopeParent?.sessionId ?? null,
      deps,
      callSites: fn.callSites.map((c) => c.code),
      ...(full || only.has(id) ? { code: fnCode, body } : {}),
      codeSha: fnCode === null ? null : sha(fnCode),
      bodySha: body === null ? null : sha(body),
      params
    });
  } else {
    const mb = rn.node;
    out({
      k: "mb",
      id,
      line: mb.declarationLine,
      declaration: mb.declaration,
      assignments: mb.assignments,
      usages: mb.usages,
      deps
    });
  }
}
