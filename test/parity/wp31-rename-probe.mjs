// WP3.1 probe: validated rename's verdicts, run on the REAL TS functions
// (attemptValidatedRename / attemptShadowingRename / getRenameRejection /
// isExportInvolved / isExportDeclarationId from src/rename/validated-rename.ts).
//
// Each scenario is a program plus a sequence of operations; the probe
// prints every verdict, then the final name of every binding (keyed by its
// declaration identifier's span) and the emitted code. The Rust port
// (`rename::validated`, test `rename_scenarios_match_the_ts_probe`) replays
// the same sequences against the overlay and must reproduce every verdict
// and every final name. Selectors are defined HERE (not by the TS tests'
// helpers) so both sides agree by construction:
//
//   "program"            the program scope
//   { owner: n, nth }    `binding.scope` of the nth binding named n, bindings
//                        ordered by declaration identifier start
//   { fn: i }            the i-th Function-type scope in pre-order
//                        (FunctionDeclaration/Expression, Arrow, methods)
//   { binding: n, nth }  (binding selectors) the nth binding named n
//   { captured: k }      a binding captured earlier by a `capture` op
//
// Run: npx tsx test/parity/wp31-rename-probe.mjs > test/parity/wp31-rename.json
import { parseSync } from "@babel/core";

const { generate, traverse } = await import("../../src/babel-utils.js");
const {
  attemptShadowingRename,
  attemptValidatedRename,
  getRenameRejection,
  isExportDeclarationId,
  isExportInvolved
} = await import("../../src/rename/validated-rename.js");
const { SCENARIOS } = await import("./wp31-rename-scenarios.mjs");

function collect(ast) {
  const scopes = [];
  const seen = new Set();
  const bindings = [];
  const seenBindings = new Set();
  traverse(ast, {
    enter(path) {
      const s = path.scope;
      if (s && !seen.has(s)) {
        seen.add(s);
        scopes.push(s);
      }
      for (const b of Object.values(s.bindings)) {
        if (!seenBindings.has(b)) {
          seenBindings.add(b);
          bindings.push(b);
        }
      }
    }
  });
  bindings.sort((a, b) => a.identifier.start - b.identifier.start);
  // Pre-order by block start, outer first (a scope's block contains its
  // children's), matching the Rust view's scope index order.
  scopes.sort(
    (a, b) => a.block.start - b.block.start || b.block.end - a.block.end
  );
  return { scopes, bindings };
}

const FUNCTION_TYPES = new Set([
  "FunctionDeclaration",
  "FunctionExpression",
  "ArrowFunctionExpression",
  "ObjectMethod",
  "ClassMethod",
  "ClassPrivateMethod"
]);

function selectors(scopes, bindings) {
  const crawlName = new Map(bindings.map((b) => [b, b.identifier.name]));
  const byCrawlName = (n, nth = 0) =>
    bindings.filter((b) => crawlName.get(b) === n)[nth];
  const captured = new Map();
  const scopeOf = (sel) => {
    if (sel === "program") return scopes[0];
    if (sel.fn !== undefined)
      return scopes.filter((s) => FUNCTION_TYPES.has(s.block.type))[sel.fn];
    if (sel.owner !== undefined) return byCrawlName(sel.owner, sel.nth).scope;
    throw new Error(`bad scope selector ${JSON.stringify(sel)}`);
  };
  const bindingOf = (sel) =>
    sel.captured !== undefined
      ? captured.get(sel.captured)
      : byCrawlName(sel.binding, sel.nth);
  return { captured, scopeOf, bindingOf };
}

const verdict = (r) => ({ applied: r.applied, reason: r.reason ?? null });

function applyOp(op, { captured, scopeOf, bindingOf }) {
  switch (op.op) {
    case "capture":
      captured.set(op.as, scopeOf(op.scope).bindings[op.name]);
      return { op: "capture" };
    case "attempt":
      return verdict(
        attemptValidatedRename(
          scopeOf(op.scope),
          op.old,
          op.new,
          op.expected ? bindingOf(op.expected) : undefined
        )
      );
    case "rejection":
      return {
        rejection: getRenameRejection(scopeOf(op.scope), op.old, op.new)
      };
    case "shadow":
      return verdict(
        attemptShadowingRename(bindingOf(op.inner), bindingOf(op.owner), op.new)
      );
    case "exportFlags": {
      const b = bindingOf(op.binding);
      return {
        involved: isExportInvolved(b),
        declarationId: isExportDeclarationId(b)
      };
    }
    default:
      throw new Error(`bad op ${op.op}`);
  }
}

function run(scenario) {
  const ast = parseSync(scenario.code, {
    sourceType: scenario.sourceType ?? "module",
    configFile: false,
    babelrc: false
  });
  const { scopes, bindings } = collect(ast);
  const ctx = selectors(scopes, bindings);
  const results = scenario.ops.map((op) => applyOp(op, ctx));
  return {
    results,
    finalNames: bindings.map((b) => ({
      id: [b.identifier.start, b.identifier.end],
      name: b.identifier.name
    })),
    emitted: generate(ast, { compact: false }).code
  };
}

const out = SCENARIOS.map((s) => {
  try {
    return {
      label: s.label,
      code: s.code,
      sourceType: s.sourceType ?? "module",
      ops: s.ops,
      ...run(s)
    };
  } catch (err) {
    return { label: s.label, error: String(err?.message ?? err) };
  }
});
console.log(JSON.stringify({ scenarios: out }, null, 1));
