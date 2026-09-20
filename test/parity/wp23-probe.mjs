// probe: WP2.3 ground truth — the statement-twin INVENTORY + the UNIQUE-TIER
// proposal join (the cascade-independent subset of
// src/prior-version/statement-twin.ts), frozen as the Rust port's
// expectations (crates/humanify-core/src/twins.rs).
//
// Runs the TS code ITSELF: topLevelStatements (statement-twin.ts :151) +
// statementHash (split/statement-hash.ts) over each side's buildUnifiedGraph
// (the same graph build the pipeline runs — prior-version.ts :284). The
// inventory builder (:227-270) is module-private, so this probe mirrors it
// EXACTLY for the parts the unique tier reads (hashes / hashCounts /
// uniqueIndex) and for the fn/binding → enclosing-statement assignment
// (:190-259) — the inventory's second half, which the Rust port maps by
// span containment.
//
// SPANS: babel's node.start/end are UTF-16 code-unit offsets; the dump's
// decided unit (and oxc's native one) is UTF-8 BYTE offsets (07 §1), so
// every span is converted before freezing. The conversion is exact, not
// approximate: the byte index is built once per text.
//
// ANCHORS (the trap this probe pins): the FRESH side is text/fresh.js —
// the PRE-rename minified text, the graph the cascade runs on. The PRIOR
// side is text/prior.js — the from-version's humanified output. The frozen
// pairs are (priorSpan, freshSpan) in that sense: the prior span is where
// the NAME comes from, the fresh span is where it lands.
//
// HASH BYTES are oxc-vs-babel serializer artifacts (WP1.4's gate): the Rust
// digests will NOT equal these. What transfers is the PARTITION — the
// equivalence classes (distinct-hash counts, bucket-size histogram, which
// statements share a hash) — so the frozen file carries those, plus the
// load-bearing counts and sample-pair spans.
//
// Usage: npx tsx test/parity/wp23-probe.mjs [dumpDir ...]
//   no args  → the five committed parity fixtures (test/parity/<f>/ts)
//   args     → dump dirs with text/{prior,fresh}.js (e.g. the oracle pair's
//              /work/oracle/.../dumps/2.1.85-2.1.86) — large, minutes
// Entries merge by directory label into test/parity/wp23-unique-twin-index.json.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { basename } from "node:path";

import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { statementHash } from "../../src/split/statement-hash.js";
import { topLevelStatements } from "../../src/prior-version/statement-twin.js";
import { parseSourceAst } from "../../src/babel-utils.js";

const FROZEN_PATH = new URL("./wp23-unique-twin-index.json", import.meta.url)
  .pathname;
const FIXTURES = ["disambiguation", "mitt", "nanoid", "preact", "r1b-synthetic"];
const SAMPLE_SIZE = 20;

// ── the TS inventory, mirrored (statement-twin.ts :227-270) ─────────────

function graphNodes(graph) {
  const fns = [];
  const bindings = [];
  for (const [, node] of graph.nodes) {
    if (node.type === "function") fns.push(node.node);
    else bindings.push(node.node);
  }
  return { fns, bindings };
}

/** statement-twin.ts :190-200 — walk up to the child of `containerNode`. */
function enclosingStatementNode(path, containerNode) {
  let current = path;
  while (current && current.parent !== containerNode) {
    current = current.parentPath;
  }
  return current?.node ?? null;
}

/** statement-twin.ts :202-206 — the module binding's declaration path. */
function moduleBindingDeclPath(node) {
  const binding = node.scope.bindings[node.name];
  return binding ? binding.path : null;
}

/** One side's inventory: the hash bookkeeping the unique tier reads plus
 * the enclosing-statement assignment for the graph rows. */
function buildSideInventoryMirror(graph, byteIndexOf) {
  const statements = topLevelStatements(graph);
  const containerNode = statements[0]?.parent ?? null;
  const stmtIndexByNode = new Map();
  const hashes = [];
  const hashCounts = new Map();
  statements.forEach((stmt, i) => {
    stmtIndexByNode.set(stmt.node, i);
    const hash = statementHash(stmt.node);
    hashes.push(hash);
    hashCounts.set(hash, (hashCounts.get(hash) ?? 0) + 1);
  });
  const uniqueIndex = new Map();
  hashes.forEach((hash, i) => {
    if (hashCounts.get(hash) === 1) uniqueIndex.set(hash, i);
  });

  const assign = (items, pathOf) => {
    const byStatement = new Map();
    let unassigned = 0;
    for (const item of items) {
      const path = pathOf(item);
      const stmtNode = path ? enclosingStatementNode(path, containerNode) : null;
      const idx = stmtNode ? stmtIndexByNode.get(stmtNode) : undefined;
      if (idx === undefined) {
        unassigned++;
        continue;
      }
      const list = byStatement.get(idx) ?? [];
      if (list.length === 0) byStatement.set(idx, list);
      list.push(item);
    }
    return { byStatement, unassigned };
  };

  let assignedFunctions = 0;
  let assignedBindings = 0;
  let unassignedFunctions = 0;
  let unassignedBindings = 0;
  if (containerNode) {
    const { fns, bindings } = graphNodes(graph);
    const fnAssign = assign(fns, (fn) => fn.path);
    const bindingAssign = assign(bindings, moduleBindingDeclPath);
    assignedFunctions = [...fnAssign.byStatement.values()].reduce(
      (n, list) => n + list.length,
      0
    );
    unassignedFunctions = fnAssign.unassigned;
    assignedBindings = [...bindingAssign.byStatement.values()].reduce(
      (n, list) => n + list.length,
      0
    );
    unassignedBindings = bindingAssign.unassigned;
  }

  const spans = statements.map((s) => byteIndexOf(s.node.start, s.node.end));
  const bucketHistogram = {};
  for (const count of hashCounts.values()) {
    bucketHistogram[count] = (bucketHistogram[count] ?? 0) + 1;
  }
  return {
    statements: statements.length,
    distinctHashes: hashCounts.size,
    uniqueHashes: uniqueIndex.size,
    maxBucket: Math.max(0, ...hashCounts.values()),
    bucketHistogram,
    assignedFunctions,
    unassignedFunctions,
    assignedBindings,
    unassignedBindings,
    _internal: { hashes, hashCounts, uniqueIndex, spans },
  };
}

/** The unique-tier join, mirrored (statement-twin.ts :1135-1150). */
function uniqueTwinProposals(prior, fresh) {
  const pairs = [];
  for (let i = 0; i < fresh._internal.hashes.length; i++) {
    const hash = fresh._internal.hashes[i];
    if (fresh._internal.hashCounts.get(hash) !== 1) continue;
    const priorIdx = prior._internal.uniqueIndex.get(hash);
    if (priorIdx === undefined) continue;
    pairs.push({ freshIdx: i, priorIdx });
  }
  return pairs;
}

// ── byte offsets (babel UTF-16 code units → UTF-8 bytes) ────────────────

function byteIndexOfFactory(code) {
  // byte offset at each char index; surrogate pairs count once (at the
  // high surrogate; the low one adds 0).
  const index = new Int32Array(code.length + 1);
  let byte = 0;
  for (let i = 0; i < code.length; i++) {
    index[i] = byte;
    const c = code.charCodeAt(i);
    if (c < 0x80) byte += 1;
    else if (c >= 0xd800 && c <= 0xdbff) byte += 4; // pair; the low half adds 0
    else if (c >= 0xdc00 && c <= 0xdfff) byte += 0;
    else if (c < 0x800) byte += 2;
    else byte += 3;
  }
  index[code.length] = byte;
  return (start, end) => [index[start], index[end]];
}

// ── one dump dir ─────────────────────────────────────────────────────────

function probeDumpDir(dir, label) {
  const priorPath = `${dir}/text/prior.js`;
  const freshPath = `${dir}/text/fresh.js`;
  if (!existsSync(priorPath) || !existsSync(freshPath)) {
    return { label, error: `missing text/{prior,fresh}.js under ${dir}` };
  }
  const priorCode = readFileSync(priorPath, "utf8");
  const freshCode = readFileSync(freshPath, "utf8");

  // The pipeline's own parse funnel (babel-utils.ts) — sourceType
  // "unambiguous", no config discovery.
  const priorAst = parseSourceAst(priorCode);
  const freshAst = parseSourceAst(freshCode);
  if (!priorAst || !freshAst) return { label, error: "parse returned null" };

  // The graphs, exactly as the pipeline builds them: the PRIOR graph with
  // the pipeline's is-eligible (prior-version.ts :284 passes () => true),
  // the FRESH graph with the public-entry default.
  const priorGraph = buildUnifiedGraph(priorAst, "prior.js", undefined, () => true, priorCode);
  const freshGraph = buildUnifiedGraph(freshAst, "fresh.js");

  const priorFull = buildSideInventoryMirror(priorGraph, byteIndexOfFactory(priorCode));
  const freshFull = buildSideInventoryMirror(freshGraph, byteIndexOfFactory(freshCode));

  const pairs = uniqueTwinProposals(priorFull, freshFull);

  const { spans: priorSpans } = priorFull._internal;
  const { spans: freshSpans } = freshFull._internal;
  const prior = { ...priorFull };
  const fresh = { ...freshFull };
  delete prior.anchor;
  delete fresh.anchor;
  delete prior._internal;
  delete fresh._internal;

  return {
    label,
    dir,
    prior,
    fresh,
    uniqueTwins: pairs.length,
    pairsSample: pairs.slice(0, SAMPLE_SIZE).map(({ freshIdx, priorIdx }) => ({
      freshIdx,
      priorIdx,
      freshSpan: freshSpans[freshIdx],
      priorSpan: priorSpans[priorIdx],
    })),
  };
}

// ── run ──────────────────────────────────────────────────────────────────

const args = process.argv.slice(2);
const targets = args.length
  ? args.map((dir) => ({ dir, label: basename(dir) }))
  : FIXTURES.map((f) => ({
      dir: new URL(`./${f}/ts`, import.meta.url).pathname,
      label: f,
    }));

const frozen = existsSync(FROZEN_PATH)
  ? JSON.parse(readFileSync(FROZEN_PATH, "utf8"))
  : { probe: "test/parity/wp23-probe.mjs", note: "entries keyed by dump label", pairs: {} };
frozen.pairs ??= {};

for (const { dir, label } of targets) {
  const entry = probeDumpDir(dir, label);
  frozen.pairs[label] = entry;
  if (entry.error) {
    console.log(`${label}: ERROR ${entry.error}`);
  } else {
    console.log(
      `${label}: prior ${entry.prior.statements} stmts / fresh ${entry.fresh.statements} stmts, ` +
        `uniqueTwins ${entry.uniqueTwins} ` +
        `(prior distinct ${entry.prior.distinctHashes}, fresh distinct ${entry.fresh.distinctHashes})`
    );
  }
}

writeFileSync(FROZEN_PATH, `${JSON.stringify(frozen, null, 2)}\n`);
console.log(`frozen → ${FROZEN_PATH}`);
