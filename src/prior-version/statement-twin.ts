/**
 * Statement-twin wholesale name inheritance (noise-reduction Lever 1).
 *
 * The function matcher's fingerprints are literal-blind, so same-shaped
 * siblings (lazy-init thunks, config wrappers) collide into ambiguous
 * buckets; when the counts differ across versions the ordinal tier
 * abstains and every sibling reaches the LLM, which re-rolls their
 * internal locals — the dominant reducible share of cross-version diff
 * noise (82.7% of noise lines have a unique twin; see
 * experiments/034-eval-harness/results/lever1-ceiling/).
 *
 * The split's `statementHash` is the complementary lens: it preserves
 * literals and masks every identifier. When a top-level statement's hash
 * is UNIQUE on both sides (count 1:1), the pair is the same code modulo
 * renaming, and every binding declared inside it can inherit its prior
 * name through the placeholder-slot bridge — deterministic, no LLM.
 *
 * Precision gates, in order (abstain on any failure — a wrong name on the
 * wrong binding is worse than a missed inherit):
 *  1. Unique twin: statementHash count 1 on both sides.
 *  2. Candidacy: the statement still contains a pending function or an
 *     unclaimed pending module binding (fully-settled statements are
 *     already covered by the exact/cascade tiers).
 *  3. Statement callee gate: the hash masks callee identity, so a
 *     same-shape twin calling DIFFERENT matched functions is a false twin
 *     — mirror of `bindingRolesAgree`'s callee veto at statement scope.
 *  4. Role gate: declared module bindings must pairwise satisfy
 *     `bindingRolesAgree` (content + callee corroboration).
 *  5. Structural gate: the placeholder-walk hash (property names and free
 *     identifiers verbatim) must also match — this is what guarantees the
 *     slot sets align for the bridge.
 *  6. Per-slot owner gate: only bindings declared inside the statement,
 *     owned by pending functions (or pending unclaimed module bindings),
 *     ever transfer. Settled owners (exact-matched, frozen, eval-taint)
 *     are never touched.
 *
 * Computed while the prior AST is alive (inside matchPriorVersion); the
 * result holds fresh-side Bindings and plain strings only.
 */
import type { NodePath } from "@babel/core";
import type * as babelTraverse from "@babel/traverse";
import type * as t from "@babel/types";
import { hashPathWithMapping } from "../analysis/structural-hash.js";
import type {
  FunctionNode,
  ModuleBindingNode,
  UnifiedGraph
} from "../analysis/types.js";
import { debug } from "../debug.js";
import { isPending, type TransferPair } from "../rename/lifecycle.js";
import { statementHash } from "../split/statement-hash.js";
import { bindingRolesAgree, computeBindingRole } from "./binding-role.js";

export interface StatementTwinStats {
  freshStatements: number;
  priorStatements: number;
  /** statementHash present exactly once on both sides */
  uniqueTwins: number;
  /** unique twins containing pending work (the only ones bridged) */
  candidates: number;
  /** candidates rejected by the statement-level callee identity gate */
  vetoedCallee: number;
  /** candidates rejected by the module-binding role gate */
  vetoedRole: number;
  /** candidates rejected by the placeholder-walk structural gate */
  vetoedStructural: number;
  /** candidates that produced at least one transfer pair */
  transferredTwins: number;
  pairs: number;
}

export interface StatementTwinTransfers {
  /** Gated transfer pairs; every binding is the fresh-side resolved Binding. */
  pairs: TransferPair[];
  stats: StatementTwinStats;
}

export function emptyStatementTwinTransfers(): StatementTwinTransfers {
  return {
    pairs: [],
    stats: {
      freshStatements: 0,
      priorStatements: 0,
      uniqueTwins: 0,
      candidates: 0,
      vetoedCallee: 0,
      vetoedRole: 0,
      vetoedStructural: 0,
      transferredTwins: 0,
      pairs: 0
    }
  };
}

/** Top-level statement paths: wrapper body when present, else Program body. */
function topLevelStatements(graph: UnifiedGraph): NodePath[] {
  if (graph.wrapperPath) {
    const body = graph.wrapperPath.get("body");
    if (!Array.isArray(body) && body.isBlockStatement()) {
      return body.get("body") as NodePath[];
    }
    return [];
  }
  const programPath = graph.targetScope.path;
  if (programPath.isProgram()) {
    return programPath.get("body") as NodePath[];
  }
  return [];
}

/** One side's statement inventory with node→statement-index assignment. */
interface SideInventory {
  statements: NodePath[];
  hashes: string[];
  hashCounts: Map<string, number>;
  /** hash → statement index, only for count-1 hashes */
  uniqueIndex: Map<string, number>;
  fnsByStatement: Map<number, FunctionNode[]>;
  bindingsByStatement: Map<number, ModuleBindingNode[]>;
}

function graphNodes(graph: UnifiedGraph): {
  fns: FunctionNode[];
  bindings: ModuleBindingNode[];
} {
  const fns: FunctionNode[] = [];
  const bindings: ModuleBindingNode[] = [];
  for (const [, node] of graph.nodes) {
    if (node.type === "function") fns.push(node.node);
    else bindings.push(node.node);
  }
  return { fns, bindings };
}

/** Walk up from a path to the child directly under `containerNode`. */
function enclosingStatementNode(
  path: NodePath | null | undefined,
  containerNode: t.Node
): t.Node | null {
  let current: NodePath | null | undefined = path;
  while (current && current.parent !== containerNode) {
    current = current.parentPath;
  }
  return current?.node ?? null;
}

/** The declaration path of a module binding, or null when unresolvable. */
function moduleBindingDeclPath(node: ModuleBindingNode): NodePath | null {
  const binding = node.scope.bindings[node.name];
  return binding ? (binding.path as NodePath) : null;
}

/** Group items under the top-level statement that encloses each of them. */
function assignToStatements<T>(
  items: T[],
  pathOf: (item: T) => NodePath | null,
  stmtIndexByNode: Map<t.Node, number>,
  containerNode: t.Node
): Map<number, T[]> {
  const byStatement = new Map<number, T[]>();
  for (const item of items) {
    const stmtNode = enclosingStatementNode(pathOf(item), containerNode);
    const idx = stmtNode ? stmtIndexByNode.get(stmtNode) : undefined;
    if (idx === undefined) continue;
    const list = byStatement.get(idx) ?? [];
    if (list.length === 0) byStatement.set(idx, list);
    list.push(item);
  }
  return byStatement;
}

function buildSideInventory(graph: UnifiedGraph): SideInventory {
  const statements = topLevelStatements(graph);
  const containerNode = statements[0]?.parent ?? null;
  const stmtIndexByNode = new Map<t.Node, number>();
  const hashes: string[] = [];
  const hashCounts = new Map<string, number>();
  for (let i = 0; i < statements.length; i++) {
    stmtIndexByNode.set(statements[i].node, i);
    const hash = statementHash(statements[i].node as t.Statement);
    hashes.push(hash);
    hashCounts.set(hash, (hashCounts.get(hash) ?? 0) + 1);
  }
  const uniqueIndex = new Map<string, number>();
  for (let i = 0; i < hashes.length; i++) {
    if (hashCounts.get(hashes[i]) === 1) uniqueIndex.set(hashes[i], i);
  }

  let fnsByStatement = new Map<number, FunctionNode[]>();
  let bindingsByStatement = new Map<number, ModuleBindingNode[]>();
  if (containerNode) {
    const { fns, bindings } = graphNodes(graph);
    fnsByStatement = assignToStatements(
      fns,
      (fn) => fn.path,
      stmtIndexByNode,
      containerNode
    );
    bindingsByStatement = assignToStatements(
      bindings,
      moduleBindingDeclPath,
      stmtIndexByNode,
      containerNode
    );
  }

  return {
    statements,
    hashes,
    hashCounts,
    uniqueIndex,
    fnsByStatement,
    bindingsByStatement
  };
}

/** External function-callee evidence of one statement's contained nodes. */
interface CalleeEvidence {
  fnCalleeIds: Set<string>;
  hasBindingCallees: boolean;
}

function statementCalleeEvidence(
  fns: FunctionNode[],
  bindings: ModuleBindingNode[]
): CalleeEvidence {
  const containedIds = new Set<string>();
  for (const fn of fns) containedIds.add(fn.sessionId);
  for (const b of bindings) containedIds.add(b.sessionId);
  const fnCalleeIds = new Set<string>();
  let hasBindingCallees = false;
  const record = (callee: FunctionNode | ModuleBindingNode) => {
    if (containedIds.has(callee.sessionId)) return;
    if (callee.sessionId.startsWith("module:")) hasBindingCallees = true;
    else fnCalleeIds.add(callee.sessionId);
  };
  for (const fn of fns) {
    for (const callee of fn.internalCallees) record(callee);
  }
  for (const b of bindings) {
    for (const callee of b.internalCallees) record(callee);
  }
  return { fnCalleeIds, hasBindingCallees };
}

/**
 * Statement-level mirror of `bindingRolesAgree`'s callee veto: when both
 * sides' contained nodes reference only matched external functions, the
 * prior's callees mapped through the function matches must equal the
 * fresh side's. Module-binding callees or unmatched functions are
 * inconclusive (no veto) — matching is incomplete, not contradictory.
 */
function calleeSetsAgree(
  prior: CalleeEvidence,
  fresh: CalleeEvidence,
  fnMatches: ReadonlyMap<string, string>
): boolean {
  if (prior.hasBindingCallees || fresh.hasBindingCallees) return true;
  if (prior.fnCalleeIds.size === 0 || fresh.fnCalleeIds.size === 0) {
    return true;
  }
  const mapped: string[] = [];
  for (const id of prior.fnCalleeIds) {
    const match = fnMatches.get(id);
    if (!match) return true; // unmatched prior callee — inconclusive
    mapped.push(match);
  }
  const key = (ids: Iterable<string>) => [...new Set(ids)].sort().join("|");
  return key(mapped) === key(fresh.fnCalleeIds);
}

/** Declaration source order, for positional pairing of declared bindings. */
function byDeclarationOrder(nodes: ModuleBindingNode[]): ModuleBindingNode[] {
  return [...nodes].sort(
    (a, b) => (a.identifier.start ?? 0) - (b.identifier.start ?? 0)
  );
}

/**
 * Pairwise role corroboration for the module bindings the twin statements
 * declare. Counts must line up (same structure ⇒ same declarations) and
 * every pair must positively agree.
 */
function declaredRolesAgree(
  priorBindings: ModuleBindingNode[],
  freshBindings: ModuleBindingNode[],
  fnMatches: ReadonlyMap<string, string>
): boolean {
  if (priorBindings.length !== freshBindings.length) return false;
  if (priorBindings.length === 0) return true;
  const prior = byDeclarationOrder(priorBindings);
  const fresh = byDeclarationOrder(freshBindings);
  for (let i = 0; i < prior.length; i++) {
    const agreement = bindingRolesAgree(
      computeBindingRole(prior[i]),
      computeBindingRole(fresh[i]),
      fnMatches
    );
    if (!agreement.agrees) return false;
  }
  return true;
}

/** Context for the per-slot owner gate. */
interface OwnerGateContext {
  fnByNode: Map<t.Node, FunctionNode>;
  moduleNodeByName: Map<string, ModuleBindingNode>;
  claimedOldNames: ReadonlySet<string>;
  wrapperNode: t.Node | null;
}

/**
 * Whether a binding declared inside the twin statement may transfer:
 * function-owned bindings need a PENDING owner (settled owners — exact
 * matches, frozen wrapper/library/eval-taint — are never touched);
 * module-level bindings need a pending, cascade-unclaimed module node (or
 * a pending declared function for function-declaration names). Unknown
 * owners abstain.
 */
function ownerAllowsTransfer(
  binding: babelTraverse.Binding,
  oldName: string,
  ctx: OwnerGateContext
): boolean {
  const scopePath = binding.scope.path as NodePath;
  const fnPath = scopePath.isFunction()
    ? scopePath
    : scopePath.getFunctionParent();
  const isModuleLevel = !fnPath || fnPath.node === ctx.wrapperNode;
  if (!isModuleLevel) {
    const ownerFn = ctx.fnByNode.get(fnPath.node);
    return ownerFn ? isPending(ownerFn) : false;
  }
  if (ctx.claimedOldNames.has(oldName)) return false;
  const moduleNode = ctx.moduleNodeByName.get(oldName);
  if (moduleNode) return isPending(moduleNode);
  if (binding.path.isFunctionDeclaration()) {
    const declaredFn = ctx.fnByNode.get(binding.path.node);
    return declaredFn ? isPending(declaredFn) : false;
  }
  return false;
}

/** Bridge one twin pair's slots into gated transfer pairs. */
function bridgeTwinSlots(
  freshStmt: NodePath,
  priorStmt: NodePath,
  ctx: OwnerGateContext
): TransferPair[] | null {
  const fresh = hashPathWithMapping(freshStmt);
  const prior = hashPathWithMapping(priorStmt);
  // Property names and free identifiers are verbatim in this hash; equal
  // hashes also guarantee the slot walks align (same serialization).
  if (fresh.hash !== prior.hash) return null;
  if (fresh.mapping.size !== prior.mapping.size) return null;

  const pairs: TransferPair[] = [];
  for (const [slot, freshName] of fresh.mapping) {
    const priorName = prior.mapping.get(slot);
    if (!priorName || priorName === freshName) continue;
    const binding = fresh.bindings.get(slot);
    if (!binding) continue;
    const declPath = binding.path as NodePath;
    const anchored =
      declPath.node === freshStmt.node || declPath.isDescendant(freshStmt);
    if (!anchored) continue; // outer reference — its own statement names it
    if (!ownerAllowsTransfer(binding, freshName, ctx)) continue;
    pairs.push({ oldName: freshName, newName: priorName, binding });
  }
  return pairs;
}

export interface StatementTwinInput {
  priorGraph: UnifiedGraph;
  newGraph: UnifiedGraph;
  /** Function matches, prior session id → new session id. */
  fnMatches: ReadonlyMap<string, string>;
  /** Module-binding old names already claimed by the cascade / var-name
   *  transfers — the finer tiers win; twins fill only the residue. */
  claimedOldNames: ReadonlySet<string>;
}

/** True when the statement still contains work no finer tier has settled. */
function hasPendingWork(
  fns: FunctionNode[],
  bindings: ModuleBindingNode[],
  claimedOldNames: ReadonlySet<string>
): boolean {
  return (
    fns.some(isPending) ||
    bindings.some((b) => isPending(b) && !claimedOldNames.has(b.name))
  );
}

interface TwinRef {
  freshSide: SideInventory;
  priorSide: SideInventory;
  freshIdx: number;
  priorIdx: number;
}

/**
 * Run one unique twin through the candidacy check and the three precision
 * gates, returning its bridged pairs (empty on any abstention; the veto
 * counters record which gate fired).
 */
function gateAndBridgeTwin(
  twin: TwinRef,
  input: StatementTwinInput,
  ownerCtx: OwnerGateContext,
  stats: StatementTwinStats
): TransferPair[] {
  const { freshSide, priorSide, freshIdx, priorIdx } = twin;
  const freshFns = freshSide.fnsByStatement.get(freshIdx) ?? [];
  const freshBindings = freshSide.bindingsByStatement.get(freshIdx) ?? [];
  if (!hasPendingWork(freshFns, freshBindings, input.claimedOldNames)) {
    return [];
  }
  stats.candidates++;

  const priorFns = priorSide.fnsByStatement.get(priorIdx) ?? [];
  const priorBindings = priorSide.bindingsByStatement.get(priorIdx) ?? [];
  if (
    !calleeSetsAgree(
      statementCalleeEvidence(priorFns, priorBindings),
      statementCalleeEvidence(freshFns, freshBindings),
      input.fnMatches
    )
  ) {
    stats.vetoedCallee++;
    return [];
  }
  if (!declaredRolesAgree(priorBindings, freshBindings, input.fnMatches)) {
    stats.vetoedRole++;
    return [];
  }
  const pairs = bridgeTwinSlots(
    freshSide.statements[freshIdx],
    priorSide.statements[priorIdx],
    ownerCtx
  );
  if (pairs === null) {
    stats.vetoedStructural++;
    return [];
  }
  return pairs;
}

/**
 * Computes gated statement-twin transfer pairs. Must run while the prior
 * AST is alive; the result references only fresh-side Bindings.
 */
export function computeStatementTwinTransfers(
  input: StatementTwinInput
): StatementTwinTransfers {
  const result = emptyStatementTwinTransfers();
  const stats = result.stats;
  const freshSide = buildSideInventory(input.newGraph);
  const priorSide = buildSideInventory(input.priorGraph);
  stats.freshStatements = freshSide.statements.length;
  stats.priorStatements = priorSide.statements.length;
  if (freshSide.statements.length === 0 || priorSide.statements.length === 0) {
    return result;
  }

  const ownerCtx: OwnerGateContext = {
    fnByNode: new Map(
      graphNodes(input.newGraph).fns.map((fn) => [fn.path.node, fn])
    ),
    moduleNodeByName: new Map(
      graphNodes(input.newGraph).bindings.map((b) => [b.name, b])
    ),
    claimedOldNames: input.claimedOldNames,
    wrapperNode: input.newGraph.wrapperPath?.node ?? null
  };

  for (let i = 0; i < freshSide.statements.length; i++) {
    const hash = freshSide.hashes[i];
    if (freshSide.hashCounts.get(hash) !== 1) continue;
    const priorIdx = priorSide.uniqueIndex.get(hash);
    if (priorIdx === undefined) continue;
    stats.uniqueTwins++;
    const pairs = gateAndBridgeTwin(
      { freshSide, priorSide, freshIdx: i, priorIdx },
      input,
      ownerCtx,
      stats
    );
    if (pairs.length === 0) continue;
    stats.transferredTwins++;
    stats.pairs += pairs.length;
    result.pairs.push(...pairs);
  }

  debug.log(
    "prior-version",
    `statement-twin: ${stats.uniqueTwins} unique twins, ${stats.candidates} with pending work, ` +
      `${stats.transferredTwins} bridged (${stats.pairs} pairs); vetoes: ` +
      `callee=${stats.vetoedCallee} role=${stats.vetoedRole} structural=${stats.vetoedStructural}`
  );
  return result;
}
