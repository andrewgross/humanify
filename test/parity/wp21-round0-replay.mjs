// probe: WP2.1 round-0 TS ground truth for the ambiguous-map ORDER question.
//
// Replicates the TS pipeline's round-0 function matching exactly:
//   1. both unified graphs built from the dump texts (prior: () => true,
//      fresh: createIsEligible("bun", "bun") — meta.json flags),
//   2. buildFingerprintIndex on both fn maps,
//   3. matchFunctions(priorIndex, newIndex, { enablePropagation: false }) —
//      the exact propagate INPUT (cascade + demote + revoke, no propagation),
//   4. a verbatim clone of propagation.ts's propagate with per-iteration
//      logging for the watch ids.
//
// Reports: the ambiguous Map's insertion-order positions of the watch ids
// (Rust's traced reconstruction put the revoked claimants mid-map at
// positions 1827/1828/2070 vs 169149 at 2141; TS's Map delete+set semantics
// should APPEND the revoked claimants at the END), each entry's candidate
// count, and the cloned propagate's per-iteration narrowing history.
//
// Usage: npx tsx test/parity/wp21-round0-replay.mjs <oracle-dump-dir>
import { readFileSync } from "node:fs";
import { parseSync } from "@babel/core";

const dumpDir = process.argv[2];
const WATCH = [
  "prior.js:98295:36",
  "prior.js:98304:34",
  "prior.js:156411:39",
  "prior.js:169149:55"
];

const { buildUnifiedGraph } = await import(
  "../../src/analysis/function-graph.js"
);
const { createIsEligible } = await import(
  "../../src/rename/rename-eligibility.js"
);
const { NULL_PROFILER } = await import("../../src/profiling/profiler.js");
const { buildFingerprintIndex, matchFunctions } = await import(
  "../../src/analysis/fingerprint-index.js"
);

const priorCode = readFileSync(`${dumpDir}/text/prior.js`, "utf8");
const freshCode = readFileSync(`${dumpDir}/text/fresh.js`, "utf8");

function parse(code) {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast) throw new Error("parse failed");
  return ast;
}

console.error("parsing prior...");
const priorAst = parse(priorCode);
console.error("building prior graph...");
const priorGraph = buildUnifiedGraph(
  priorAst,
  "prior.js",
  NULL_PROFILER,
  () => true,
  priorCode
);
console.error("parsing fresh...");
const freshAst = parse(freshCode);
console.error("building fresh graph...");
const freshGraph = buildUnifiedGraph(
  freshAst,
  "input.js",
  NULL_PROFILER,
  createIsEligible("bun", "bun"),
  freshCode
);

function splitFns(graph) {
  const fns = new Map();
  for (const [, node] of graph.nodes) {
    if (node.type === "function") fns.set(node.node.sessionId, node.node);
  }
  return fns;
}
const priorFnMap = splitFns(priorGraph);
const freshFnMap = splitFns(freshGraph);
console.error(
  `graphs: prior fns ${priorFnMap.size}, fresh fns ${freshFnMap.size}`
);

// validate the fresh fn set against the oracle dump's functions.json
const oracleRows = JSON.parse(
  readFileSync(`${dumpDir}/functions.json`, "utf8")
).functions;
const oracleFnIds = new Set(
  oracleRows.filter((r) => r.kind === "function").map((r) => r.sessionId)
);
let missingInProbe = 0;
let extraInProbe = 0;
for (const id of oracleFnIds) if (!freshFnMap.has(id)) missingInProbe++;
for (const id of freshFnMap.keys()) if (!oracleFnIds.has(id)) extraInProbe++;
console.error(
  `fresh-set check: oracle ${oracleFnIds.size} vs probe ${freshFnMap.size} — missingInProbe ${missingInProbe}, extraInProbe ${extraInProbe}`
);
if (missingInProbe > 0 || extraInProbe > 0) {
  const missSample = [...oracleFnIds]
    .filter((id) => !freshFnMap.has(id))
    .slice(0, 5);
  const extraSample = [...freshFnMap.keys()]
    .filter((id) => !oracleFnIds.has(id))
    .slice(0, 5);
  console.error(`  missing sample: ${missSample.join(", ")}`);
  console.error(`  extra sample: ${extraSample.join(", ")}`);
}

console.error("building indexes...");
const priorIndex = buildFingerprintIndex(priorFnMap);
const newIndex = buildFingerprintIndex(freshFnMap);

console.error("running matchFunctions (enablePropagation: false)...");
const cascade = matchFunctions(priorIndex, newIndex, {
  enablePropagation: false
});
console.error(
  `cascade: matches ${cascade.matches.size}, ambiguous ${cascade.ambiguous.size}, unmatched ${cascade.unmatched.length}`
);

// ── the ambiguous Map's INSERTION ORDER at propagate start ──
const ambiguousArr = [...cascade.ambiguous.entries()];
const posOf = new Map();
for (const [i, [id]] of ambiguousArr.entries()) posOf.set(id, i);
console.log(
  JSON.stringify({
    ambiguousSize: ambiguousArr.length,
    matches: cascade.matches.size
  })
);
for (const id of WATCH) {
  const pos = posOf.get(id);
  const cands = cascade.ambiguous.get(id);
  console.log(
    `ORDER ${id} pos=${pos ?? "ABSENT"} cand=${cands ? cands.length : "-"}`
  );
}
// where do the four sit relative to the whole map?
const positions = WATCH.map((id) => posOf.get(id)).filter(
  (p) => p !== undefined
);
if (positions.length === 4) {
  console.log(
    `ORDER span: min=${Math.min(...positions)} max=${Math.max(...positions)} of ${ambiguousArr.length}`
  );
}

// ── verbatim clone of propagation.ts's propagate, with watch logging ──
const WATCH_SET = new Set(WATCH);
function log(line) {
  console.log(line);
}

function buildReverseMatches(matches) {
  const reverse = new Map();
  for (const [oldId, newId] of matches) reverse.set(newId, oldId);
  return reverse;
}

function buildScopeChildrenIndex(functions) {
  const children = new Map();
  for (const [id, fn] of functions) {
    if (!fn.scopeParent) continue;
    const parentId = fn.scopeParent.sessionId;
    let list = children.get(parentId);
    if (!list) {
      list = [];
      children.set(parentId, list);
    }
    list.push(id);
  }
  const positionOf = (id) => {
    const pos = functions.get(id)?.position;
    return pos ? pos.line * 100000 + pos.column : Number.MAX_SAFE_INTEGER;
  };
  for (const list of children.values()) {
    list.sort((a, b) => positionOf(a) - positionOf(b));
  }
  return children;
}

function buildCallersIndex(functions) {
  const callers = new Map();
  for (const [id, fn] of functions) {
    for (const callee of fn.internalCallees) {
      let callerSet = callers.get(callee.sessionId);
      if (!callerSet) {
        callerSet = new Set();
        callers.set(callee.sessionId, callerSet);
      }
      callerSet.add(id);
    }
  }
  return callers;
}

function getMatchedNewIds(nodes, matches) {
  const result = [];
  for (const node of nodes) {
    const matchedNewId = matches.get(node.sessionId);
    if (matchedNewId) result.push(matchedNewId);
  }
  return result;
}

function filterByMatchedCallees(oldFn, candidates, state) {
  const matchedCalleeNewIds = getMatchedNewIds(
    oldFn.internalCallees,
    state.matches
  );
  if (matchedCalleeNewIds.length === 0) return null;
  return candidates.filter((candId) => {
    const candFn = state.newFunctions.get(candId);
    if (!candFn) return false;
    const candCalleeIds = new Set(
      [...candFn.internalCallees].map((c) => c.sessionId)
    );
    return matchedCalleeNewIds.every((id) => candCalleeIds.has(id));
  });
}

function filterByMatchedCallers(oldFn, candidates, state) {
  const matchedCallerNewIds = getMatchedNewIds(oldFn.callers, state.matches);
  if (matchedCallerNewIds.length === 0) return null;
  return candidates.filter((candId) => {
    const candCallers = state.newCallers.get(candId);
    if (!candCallers) return false;
    return matchedCallerNewIds.every((callerId) => candCallers.has(callerId));
  });
}

function filterByScopeParent(oldFn, candidates, state) {
  if (!oldFn.scopeParent) return null;
  const matchedParentNewId = state.matches.get(oldFn.scopeParent.sessionId);
  if (!matchedParentNewId) return null;
  return candidates.filter((candId) => {
    const candFn = state.newFunctions.get(candId);
    if (!candFn) return false;
    return candFn.scopeParent?.sessionId === matchedParentNewId;
  });
}

function filterByMatchedExternalRefs(oldFn, candidates, state) {
  const evidence = state.externalRefEvidence;
  if (!evidence) return null;
  const oldRefIds = evidence.oldRefs.get(oldFn.sessionId);
  if (!oldRefIds || oldRefIds.size === 0) return null;
  const expectedNewIds = [];
  for (const oldRefId of oldRefIds) {
    const newRefId = evidence.refMatches.get(oldRefId);
    if (newRefId) expectedNewIds.push(newRefId);
  }
  if (expectedNewIds.length === 0) return null;
  return candidates.filter((candId) => {
    const candRefs = evidence.newRefs.get(candId);
    if (!candRefs) return false;
    return expectedNewIds.every((id) => candRefs.has(id));
  });
}

function applyConstraintStrategies(oldFn, candidates, state) {
  let pool = candidates;
  let evidenced = false;
  let rung;
  const strategies = [
    ["matchedCallee", filterByMatchedCallees],
    ["matchedCaller", filterByMatchedCallers],
    ["scopeParent", filterByScopeParent],
    ["externalRefs", filterByMatchedExternalRefs]
  ];
  for (const [name, strategy] of strategies) {
    const filtered = strategy(oldFn, pool, state);
    if (filtered === null) continue;
    if (filtered.length === 0) return "contradiction";
    if (filtered.length < pool.length || pool.length === 1) {
      evidenced = true;
      rung = name;
    }
    pool = filtered;
    if (pool.length === 1 && evidenced) break;
  }
  return { pool, evidenced, rung };
}

function tryScopeOrdinalMatch(oldId, candidates, state) {
  const oldFn = state.oldFunctions.get(oldId);
  if (!oldFn?.scopeParent) return null;
  const matchedParentNewId = state.matches.get(oldFn.scopeParent.sessionId);
  if (!matchedParentNewId) return null;
  const oldHash = oldFn.fingerprint.structuralHash;
  const oldSiblings = (
    state.oldScopeChildren.get(oldFn.scopeParent.sessionId) ?? []
  ).filter((id) => {
    const fn = state.oldFunctions.get(id);
    return fn?.fingerprint.structuralHash === oldHash;
  });
  const newSiblings = (
    state.newScopeChildren.get(matchedParentNewId) ?? []
  ).filter((id) => {
    const fn = state.newFunctions.get(id);
    return fn?.fingerprint.structuralHash === oldHash;
  });
  if (oldSiblings.length !== newSiblings.length) return null;
  if (oldSiblings.length === 0) return null;
  const ordinal = oldSiblings.indexOf(oldId);
  if (ordinal === -1) return null;
  const matched = newSiblings[ordinal];
  if (!candidates.includes(matched)) return null;
  return matched;
}

function narrowCandidates(oldId, candidates, state, _iter) {
  const oldFn = state.oldFunctions.get(oldId);
  if (!oldFn) return { pool: candidates, evidenced: false };
  const available = candidates.filter(
    (candId) => !state.reverseMatches.has(candId)
  );
  if (available.length === 0) return { pool: available, evidenced: false };
  const narrowed = applyConstraintStrategies(oldFn, available, state);
  if (narrowed === "contradiction") return { pool: [], evidenced: false };
  if (narrowed.pool.length === 1 && narrowed.evidenced) return narrowed;
  if (narrowed.pool.length > 1) {
    const ordinalMatch = tryScopeOrdinalMatch(oldId, narrowed.pool, state);
    if (ordinalMatch)
      return { pool: [ordinalMatch], evidenced: true, rung: "scopeOrdinal" };
  }
  return narrowed;
}

// The replay mirrors propagate()'s exact branch structure — refactoring
// would break the fidelity the mechanism trace depends on.
// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: replay fidelity
function runOneIteration(state, iteration) {
  let newlyResolved = 0;
  const entries = [...state.ambiguous.entries()];
  for (const [oldId, candidates] of entries) {
    if (!state.ambiguous.has(oldId)) continue;
    const { pool, evidenced, rung } = narrowCandidates(
      oldId,
      candidates,
      state,
      iteration
    );
    if (WATCH_SET.has(oldId)) {
      const action =
        pool.length === 1 && evidenced
          ? `resolved -> ${pool[0]} (rung ${rung})`
          : pool.length > 1 && pool.length < candidates.length
            ? `written ${pool.length}`
            : `kept (pool ${pool.length})`;
      log(
        `TSNARROW iter=${iteration} ${oldId} cand=${candidates.length} pool=${pool.length} ev=${evidenced} rung=${rung ?? "None"} act=${action}`
      );
    }
    if (pool.length === 1 && evidenced) {
      state.matches.set(oldId, pool[0]);
      state.reverseMatches.set(pool[0], oldId);
      state.ambiguous.delete(oldId);
      if (rung) state.byRung[rung]++;
      newlyResolved++;
    } else if (pool.length > 1 && pool.length < candidates.length) {
      state.ambiguous.set(oldId, pool);
    }
  }
  return newlyResolved;
}

function propagate(matches, ambiguous, oldIndex, newIndex) {
  const byRung = {
    matchedCallee: 0,
    matchedCaller: 0,
    scopeParent: 0,
    externalRefs: 0,
    scopeOrdinal: 0
  };
  if (ambiguous.size === 0) return { resolved: 0, iterations: 0, byRung };
  const state = {
    matches,
    reverseMatches: buildReverseMatches(matches),
    ambiguous,
    oldFunctions: oldIndex.functions,
    newFunctions: newIndex.functions,
    newCallers: buildCallersIndex(newIndex.functions),
    oldScopeChildren: buildScopeChildrenIndex(oldIndex.functions),
    newScopeChildren: buildScopeChildrenIndex(newIndex.functions),
    externalRefEvidence: undefined,
    byRung
  };
  const maxIterations = 10;
  let totalResolved = 0;
  for (let i = 0; i < maxIterations; i++) {
    const newlyResolved = runOneIteration(state, i);
    totalResolved += newlyResolved;
    log(
      `TSITER ${i} resolvedThisIter=${newlyResolved} live=${state.ambiguous.size}`
    );
    if (newlyResolved === 0) break;
  }
  return { resolved: totalResolved, iterations: maxIterations, byRung };
}

console.error("running cloned propagate (round-0, no externalRefEvidence)...");
const matches = cascade.matches;
const ambiguous = cascade.ambiguous;
const prop = propagate(matches, ambiguous, priorIndex, newIndex);
console.log(
  `TSPROP end resolved=${prop.resolved} byRung=${JSON.stringify(prop.byRung)}`
);

for (const id of WATCH) {
  const paired = matches.get(id);
  const pool = ambiguous.get(id);
  console.log(
    `TSFINAL ${id} ${paired ? `matched ${paired}` : pool ? `stillAmbiguous ${pool.length}` : "absent"}`
  );
}

// final state vs the oracle dump's pairs
const oracleMatches = JSON.parse(
  readFileSync(`${dumpDir}/matches.json`, "utf8")
);
const oraclePairMap = new Map(
  oracleMatches.pairs.map((p) => [p.prior, p.fresh])
);
for (const id of WATCH) {
  const mine = matches.get(id);
  const oracle = oraclePairMap.get(id);
  console.log(
    `TSVSORACLE ${id} mine=${mine ?? "-"} oracle=${oracle ?? "-"} ${mine === oracle ? "SAME" : "DIFF"}`
  );
}
