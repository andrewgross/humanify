// probe: WP2.1 evidence ground truth — recompute the TS side's
// buildExternalRefEvidence oldRefs (prior-version.ts :1703-1740) for the
// prior-side functions the Rust matches gate dumped in evidence.json, and
// diff the two ref sets. The refs feed the externalRefs propagation rung;
// a ref-set difference shifts WHICH iteration a chain resolution lands in
// (the WP2.1 residual: a stillAmbiguous pool that kept claimed candidates
// in TS and dropped them in Rust).
//
// Usage: npx tsx test/parity/wp21-evdiff-prior.mjs <ts-dump-dir> <rust-evidence.json>
import { readFileSync } from "node:fs";
import { parseSync } from "@babel/core";

const { buildUnifiedGraph } = await import(
  "../../src/analysis/function-graph.js"
);
const { NULL_PROFILER } = await import("../../src/profiling/profiler.js");

const dumpDir = process.argv[2];
const evidencePath = process.argv[3];
const priorCode = readFileSync(`${dumpDir}/text/prior.js`, "utf8");

const ast = parseSync(priorCode, {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
});
if (!ast) throw new Error("prior text failed to parse");

// prior-version.ts :284-288 — the prior side's unified graph, ALL
// bindings eligible.
const graph = buildUnifiedGraph(
  ast,
  "prior.js",
  NULL_PROFILER,
  () => true,
  priorCode
);
const priorFunctions = [];
const priorBindings = [];
for (const [, node] of graph.nodes) {
  if (node.type === "function") priorFunctions.push(node.node);
  else priorBindings.push(node.node);
}
const priorFnMap = new Map(priorFunctions.map((fn) => [fn.sessionId, fn]));

// ── verbatim from prior-version.ts :1748-1823 ──
function referenceIdsByBinding(bindingsById, functions) {
  const ids = bindingsById
    ? bindingIdsByBindingObject(bindingsById)
    : new Map();
  for (const [binding, fnId] of functionIdsByBinding(functions)) {
    ids.set(binding, fnId);
  }
  return ids;
}

function functionIdsByBinding(functions) {
  const map = new Map();
  for (const fn of functions) {
    const binding = holdingBinding(fn);
    if (binding) map.set(binding, fn.sessionId);
  }
  return map;
}

function holdingBinding(fn) {
  const path = fn.path;
  if (path.isFunctionDeclaration()) {
    const id = path.node.id;
    if (!id) return null;
    const binding =
      path.parentPath?.scope.getBinding(id.name) ??
      path.scope.getBinding(id.name);
    return binding && binding.path.node === path.node ? binding : null;
  }
  const parent = path.parentPath;
  if (parent?.isVariableDeclarator() && parent.node.id.type === "Identifier") {
    const binding = parent.scope.getBinding(parent.node.id.name);
    return binding && binding.path.node === parent.node ? binding : null;
  }
  return null;
}

function bindingIdsByBindingObject(byId) {
  const map = new Map();
  for (const [sessionId, node] of byId) {
    const binding = node.scope.getBinding(node.name);
    if (binding) map.set(binding, sessionId);
  }
  return map;
}

function collectReferencedBindingIds(fn, idsByBinding) {
  const refs = new Set();
  fn.path.traverse({
    Identifier(idPath) {
      if (!idPath.isReferencedIdentifier()) return;
      const binding = idPath.scope.getBinding(idPath.node.name);
      if (!binding) return;
      const id = idsByBinding.get(binding);
      if (id) refs.add(id);
    }
  });
  return refs;
}
// ── end verbatim block ──

const priorById = new Map(priorBindings.map((b) => [b.sessionId, b]));
const idsByBinding = referenceIdsByBinding(priorById, priorFunctions);
console.error(
  `ts-side: ${priorFunctions.length} fns, ${priorBindings.length} bindings, ` +
    `${idsByBinding.size} identity-mapped ids`
);

const evidence = JSON.parse(readFileSync(evidencePath, "utf8")).evidence;
const oldRefs = evidence.oldRefs;
const oldIds = [...oldRefs.keys()].filter((id) => id.startsWith("prior.js:"));
console.error(`evidence: ${oldIds.length} prior-side oldRefs entries`);

let same = 0;
const diffs = [];
for (const id of oldIds) {
  const fn = priorFnMap.get(id);
  if (!fn) {
    diffs.push({ id, error: "not in the rebuilt prior graph" });
    continue;
  }
  const tsRefs = collectReferencedBindingIds(fn, idsByBinding);
  const rustRefs = new Set(oldRefs[id]);
  const missing = [...tsRefs].filter((r) => !rustRefs.has(r));
  const extra = [...rustRefs].filter((r) => !tsRefs.has(r));
  if (missing.length === 0 && extra.length === 0) {
    same++;
    continue;
  }
  diffs.push({ id, missing, extra });
}
console.log(
  JSON.stringify(
    { same, diffCount: diffs.length, diffs: diffs.slice(0, 60) },
    null,
    1
  )
);
