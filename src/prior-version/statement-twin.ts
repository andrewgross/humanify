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
import * as t from "@babel/types";
import {
  hashPathWithMapping,
  serializePathTokens
} from "../analysis/structural-hash.js";
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
  /** non-unique-bucket members paired by matched-reference identity */
  bucketTwins: number;
  /** outer-reference vote pairs emitted from bridged statements */
  outerRefs: number;
  /** private ids transferred from masked-equal twins */
  privateRenames: number;
  /** cascade-claimed heads whose gated twin disagrees on the name — the
   * literal-blind cascade rotated same-shape family members that the
   * literal-preserving twin pairing can see are crossed */
  cascadeConflicts: number;
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

/** One private id's transfer: every fresh PrivateName node carrying it. */
export interface PrivateRenameSet {
  oldName: string;
  newName: string;
  nodes: t.PrivateName[];
}

export interface StatementTwinTransfers {
  /** Gated transfer pairs; every binding is the fresh-side resolved Binding. */
  pairs: TransferPair[];
  /**
   * Private-name transfers from masked-equal twins. Privates are not scope
   * bindings — application mutates the PrivateName nodes directly, gated on
   * unique in-statement declaration and a collision-free target.
   */
  privateRenames: PrivateRenameSet[];
  /**
   * Prior names observed for OUTER bindings referenced from bridged
   * statements (declared elsewhere). Never applied directly — they feed
   * the external-reference vote propagation with exact-grade testimony,
   * which is what names content-free roots (`var cache;`) that no hash
   * or fingerprint can match.
   */
  outerRefs: TransferPair[];
  stats: StatementTwinStats;
}

export function emptyStatementTwinTransfers(): StatementTwinTransfers {
  return {
    pairs: [],
    privateRenames: [],
    outerRefs: [],
    stats: {
      freshStatements: 0,
      priorStatements: 0,
      uniqueTwins: 0,
      bucketTwins: 0,
      outerRefs: 0,
      privateRenames: 0,
      cascadeConflicts: 0,
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
  /** oldName → cascade's chosen prior name, for conflict diagnostics. */
  cascadeNameByOld: ReadonlyMap<string, string>;
  conflicts: CascadeConflict[];
}

/** Pending owners always transfer; exact-matched ("transferred") owners
 * do too — the twin tier applies BEFORE exact transfers, so a same-
 * statement match just produces the same names (the exact pair then drops
 * stale) while a cross-paired match gets repaired. Frozen owners
 * (wrapper / library / eval-taint) and every other settled state never
 * transfer. */
function pendingOrExactMatched(fn: FunctionNode): boolean {
  return isPending(fn) || fn.state.kind === "transferred";
}

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
    return ownerFn ? pendingOrExactMatched(ownerFn) : false;
  }
  if (ctx.claimedOldNames.has(oldName)) return false;
  const moduleNode = ctx.moduleNodeByName.get(oldName);
  if (moduleNode) return isPending(moduleNode);
  if (binding.path.isFunctionDeclaration()) {
    const declaredFn = ctx.fnByNode.get(binding.path.node);
    return declaredFn ? pendingOrExactMatched(declaredFn) : false;
  }
  return false;
}

/** Token streams equal once `P=#name` tokens are blinded — the twins
 * differ only in private-name spellings. */
function privateMaskedStreamsEqual(
  freshStmt: NodePath,
  priorStmt: NodePath
): boolean {
  const blind = (tok: string) => (tok.startsWith("P=#") ? "P=#" : tok);
  const fresh = serializePathTokens(freshStmt);
  const prior = serializePathTokens(priorStmt);
  if (fresh.length !== prior.length) return false;
  for (let i = 0; i < fresh.length; i++) {
    if (blind(fresh[i]) !== blind(prior[i])) return false;
  }
  return true;
}

function pushChildNodes(stack: t.Node[], child: unknown): void {
  if (Array.isArray(child)) {
    for (let i = child.length - 1; i >= 0; i--) pushChildNodes(stack, child[i]);
    return;
  }
  if (
    typeof child === "object" &&
    child !== null &&
    typeof (child as { type?: unknown }).type === "string"
  ) {
    stack.push(child as t.Node);
  }
}

/** Positional PrivateName pairs from two isomorphic subtrees (guaranteed
 * by masked-stream equality — same walk, same node sequence). */
function collectPrivatePairs(
  freshRoot: t.Node,
  priorRoot: t.Node
): Array<{ node: t.PrivateName; priorName: string }> {
  const out: Array<{ node: t.PrivateName; priorName: string }> = [];
  const freshStack: t.Node[] = [freshRoot];
  const priorStack: t.Node[] = [priorRoot];
  while (freshStack.length > 0 && priorStack.length > 0) {
    const fresh = freshStack.pop() as t.Node;
    const prior = priorStack.pop() as t.Node;
    if (t.isPrivateName(fresh) && t.isPrivateName(prior)) {
      out.push({ node: fresh, priorName: prior.id.name });
    }
    const keys = t.VISITOR_KEYS[fresh.type] ?? [];
    for (let k = keys.length - 1; k >= 0; k--) {
      pushChildNodes(
        freshStack,
        (fresh as unknown as Record<string, unknown>)[keys[k]]
      );
      pushChildNodes(
        priorStack,
        (prior as unknown as Record<string, unknown>)[keys[k]]
      );
    }
  }
  return out;
}

/** Private ids declared by class members inside the statement, mapped to
 * the set of declaring class nodes (an id in >1 class is ambiguous). */
function declaredPrivateClasses(root: t.Node): Map<string, Set<t.Node>> {
  const byId = new Map<string, Set<t.Node>>();
  t.traverseFast(root, (node) => {
    if (!t.isClass(node)) return;
    for (const member of node.body.body) {
      const key = (member as { key?: t.Node }).key;
      if (key && t.isPrivateName(key)) {
        let set = byId.get(key.id.name);
        if (!set) {
          set = new Set();
          byId.set(key.id.name, set);
        }
        set.add(node);
      }
    }
  });
  return byId;
}

/**
 * Gate positional private pairs into safe rename sets: consistent mapping
 * per id, id declared by exactly ONE class in the statement, target not
 * colliding with any surviving fresh id, and 1:1 (no two ids sharing a
 * target). Everything else abstains.
 */
interface PrivateEvidence {
  priorNames: Set<string>;
  nodes: t.PrivateName[];
}

/** Fold raw positional pairs into per-fresh-id evidence. */
function accumulatePrivateEvidence(
  raw: Array<{ node: t.PrivateName; priorName: string }>
): Map<string, PrivateEvidence> {
  const byId = new Map<string, PrivateEvidence>();
  for (const { node, priorName } of raw) {
    let entry = byId.get(node.id.name);
    if (!entry) {
      entry = { priorNames: new Set(), nodes: [] };
      byId.set(node.id.name, entry);
    }
    entry.priorNames.add(priorName);
    entry.nodes.push(node);
  }
  return byId;
}

/** How many fresh ids map (consistently) onto each target id. */
function privateTargetCounts(
  byId: Map<string, PrivateEvidence>
): Map<string, number> {
  const counts = new Map<string, number>();
  for (const [, entry] of byId) {
    if (entry.priorNames.size !== 1) continue;
    const target = [...entry.priorNames][0];
    counts.set(target, (counts.get(target) ?? 0) + 1);
  }
  return counts;
}

function gatePrivateRenames(
  raw: Array<{ node: t.PrivateName; priorName: string }>,
  freshRoot: t.Node
): PrivateRenameSet[] {
  if (raw.length === 0) return [];
  const byId = accumulatePrivateEvidence(raw);
  const declared = declaredPrivateClasses(freshRoot);
  const declaredIds = new Set(declared.keys());
  const targetCounts = privateTargetCounts(byId);
  const sets: PrivateRenameSet[] = [];
  for (const [oldName, entry] of byId) {
    if (entry.priorNames.size !== 1) continue; // inconsistent — abstain
    const newName = [...entry.priorNames][0];
    if (newName === oldName) continue;
    if (declared.get(oldName)?.size !== 1) continue; // multi/un-declared
    if (declaredIds.has(newName)) continue; // collision (incl. swaps)
    if ((targetCounts.get(newName) ?? 0) !== 1) continue; // two ids → one
    sets.push({ oldName, newName, nodes: entry.nodes });
  }
  return sets;
}

/** Note when the cascade claimed this head under a DIFFERENT prior name
 * than the gated twin pairing derives — family-rotation diagnostics.
 * Identity-guarded: minified names are reused across scopes, so the slot
 * must resolve to the SAME binding the cascade's module node holds — a
 * name-string match alone would count unrelated statement-locals. */
function recordCascadeConflict(
  ctx: OwnerGateContext,
  binding: babelTraverse.Binding,
  oldName: string,
  twinName: string
): boolean {
  const cascadeName = ctx.cascadeNameByOld.get(oldName);
  if (cascadeName === undefined || cascadeName === twinName) return false;
  const moduleBinding = ctx.moduleNodeByName
    .get(oldName)
    ?.scope.getBinding(oldName);
  if (moduleBinding !== binding) return false;
  ctx.conflicts.push({ oldName, cascadeName, twinName });
  return true;
}

interface BridgedSlots {
  pairs: TransferPair[];
  privateRenames: PrivateRenameSet[];
  outerRefs: TransferPair[];
}

/** Bridge one twin pair's slots into gated transfer pairs plus outer-
 * reference vote material (names of bindings declared elsewhere). */
function bridgeTwinSlots(
  freshStmt: NodePath,
  priorStmt: NodePath,
  ctx: OwnerGateContext
): BridgedSlots | null {
  const fresh = hashPathWithMapping(freshStmt);
  const prior = hashPathWithMapping(priorStmt);
  // Property names and free identifiers are verbatim in this hash; equal
  // hashes also guarantee the slot walks align (same serialization). When
  // they differ ONLY in private-name spellings (un-renamed minified ids
  // the input minifier re-lettered), the masked stream reconciles — the
  // walks still align, and the private ids themselves become transfers.
  let privateRenames: PrivateRenameSet[] = [];
  if (fresh.hash !== prior.hash) {
    if (!privateMaskedStreamsEqual(freshStmt, priorStmt)) return null;
    privateRenames = gatePrivateRenames(
      collectPrivatePairs(freshStmt.node, priorStmt.node),
      freshStmt.node
    );
  }
  if (fresh.mapping.size !== prior.mapping.size) return null;

  const pairs: TransferPair[] = [];
  const outerRefs: TransferPair[] = [];
  for (const [slot, freshName] of fresh.mapping) {
    const priorName = prior.mapping.get(slot);
    if (!priorName || priorName === freshName) continue;
    const binding = fresh.bindings.get(slot);
    if (!binding) continue;
    bridgeOneSlot(freshStmt, binding, freshName, priorName, ctx, {
      pairs,
      outerRefs
    });
  }
  return { pairs, privateRenames, outerRefs };
}

/** Route one aligned slot: outer-reference vote, gated transfer pair, or
 * abstention. A conflicting cascade claim on an identity-confirmed head
 * emits the twin's pair anyway — the twin pairing sees the literals the
 * cascade cannot, and this tier applies first so the crossed cascade
 * rename drops stale. An AGREEING claim still defers to the cascade. */
function bridgeOneSlot(
  freshStmt: NodePath,
  binding: babelTraverse.Binding,
  freshName: string,
  priorName: string,
  ctx: OwnerGateContext,
  out: { pairs: TransferPair[]; outerRefs: TransferPair[] }
): void {
  const declPath = binding.path as NodePath;
  const anchored =
    declPath.node === freshStmt.node || declPath.isDescendant(freshStmt);
  if (!anchored) {
    // Declared elsewhere — this statement's testimony about its name
    // goes to vote propagation, never applied directly.
    out.outerRefs.push({ oldName: freshName, newName: priorName, binding });
    return;
  }
  const conflicted = recordCascadeConflict(ctx, binding, freshName, priorName);
  if (!conflicted && !ownerAllowsTransfer(binding, freshName, ctx)) return;
  out.pairs.push({ oldName: freshName, newName: priorName, binding });
}

export interface CascadeConflict {
  oldName: string;
  cascadeName: string;
  twinName: string;
}

export interface StatementTwinInput {
  priorGraph: UnifiedGraph;
  newGraph: UnifiedGraph;
  /** Function matches, prior session id → new session id. */
  fnMatches: ReadonlyMap<string, string>;
  /** Module-binding old names already claimed by the cascade / var-name
   *  transfers — the finer tiers win; twins fill only the residue. */
  claimedOldNames: ReadonlySet<string>;
  /** The binding cascade's matches (fresh minified oldName → prior name),
   *  identity evidence for pairing non-unique statement buckets. */
  bindingIdentityPairs: ReadonlyArray<{ oldName: string; newName: string }>;
}

/** Cross-version bookkeeping for spotting cross-paired exact matches. */
interface CrossPairContext {
  /** new fn session id → prior fn session id (inverted fnMatches) */
  newToPriorFnId: Map<string, string>;
  /** prior fn session id → its top-level statement index */
  priorStmtIdxByFnId: Map<string, number>;
}

function buildCrossPairContext(
  fnMatches: ReadonlyMap<string, string>,
  priorSide: SideInventory
): CrossPairContext {
  const newToPriorFnId = new Map<string, string>();
  for (const [priorId, newId] of fnMatches) newToPriorFnId.set(newId, priorId);
  const priorStmtIdxByFnId = new Map<string, number>();
  for (const [idx, fns] of priorSide.fnsByStatement) {
    for (const fn of fns) priorStmtIdxByFnId.set(fn.sessionId, idx);
  }
  return { newToPriorFnId, priorStmtIdxByFnId };
}

/**
 * An exact-matched fresh function whose matched PRIOR function lives in a
 * DIFFERENT statement than the prior twin — the ordinal/identity tiers
 * cross-paired same-shaped siblings (the 85→86 bundle-shuffle failure),
 * and the exact transfer is about to write the sibling's names here. The
 * unique statement twin is the stronger evidence and repairs it.
 */
function isCrossPaired(
  fn: FunctionNode,
  priorTwinIdx: number,
  cross: CrossPairContext
): boolean {
  if (fn.state.kind !== "transferred") return false;
  const priorId = cross.newToPriorFnId.get(fn.sessionId);
  if (!priorId) return false;
  const priorIdx = cross.priorStmtIdxByFnId.get(priorId);
  return priorIdx !== undefined && priorIdx !== priorTwinIdx;
}

/** True when the statement still contains work this tier should bridge:
 * pending functions, an unclaimed pending module binding, or an exact
 * match that cross-paired into a different prior statement. */
function needsBridging(
  fns: FunctionNode[],
  bindings: ModuleBindingNode[],
  claimedOldNames: ReadonlySet<string>,
  priorTwinIdx: number,
  cross: CrossPairContext
): boolean {
  return (
    fns.some(isPending) ||
    bindings.some((b) => isPending(b) && !claimedOldNames.has(b.name)) ||
    fns.some((fn) => isCrossPaired(fn, priorTwinIdx, cross))
  );
}

/** The binding that HOLDS a function's value (declaration name binding or
 * the var declarator it is assigned to), identity-guarded. Local copy of
 * prior-version.ts's holdingBinding — importing it would be a cycle. */
function holdingBindingOf(fn: FunctionNode): babelTraverse.Binding | null {
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

/**
 * Identity ids for bindings on the FRESH side, in the PRIOR id namespace:
 * matched functions' holder bindings → `fn:<priorId>`, cascade-matched
 * module bindings → `bind:<priorName>`. Unmatched things are absent —
 * a reference to them contributes no identity.
 */
function freshIdentityByBinding(
  newGraph: UnifiedGraph,
  fnMatches: ReadonlyMap<string, string>,
  bindingIdentityPairs: ReadonlyArray<{ oldName: string; newName: string }>
): Map<babelTraverse.Binding, string> {
  const inverse = new Map<string, string>();
  for (const [priorId, newId] of fnMatches) inverse.set(newId, priorId);
  const map = new Map<babelTraverse.Binding, string>();
  const { fns, bindings } = graphNodes(newGraph);
  for (const fn of fns) {
    const priorId = inverse.get(fn.sessionId);
    if (!priorId) continue;
    const holder = holdingBindingOf(fn);
    if (holder) map.set(holder, `fn:${priorId}`);
  }
  const byName = new Map(bindings.map((b) => [b.name, b]));
  for (const pair of bindingIdentityPairs) {
    const node = byName.get(pair.oldName);
    const binding = node?.scope.getBinding(pair.oldName);
    if (binding) map.set(binding, `bind:${pair.newName}`);
  }
  return map;
}

/**
 * Identity ids for PRIOR bindings — restricted to nodes that MATCHED
 * across versions, mirroring the fresh side exactly. Symmetry is
 * load-bearing: with full prior keys against fresh matched-subset keys,
 * a fresh statement whose unmatched refs are invisible could uniquely
 * claim the WRONG prior member (its true partner's full key differs by
 * an unmatched ref) — the v4 feature-pair regression. When both sides
 * drop unmatched refs, such coincidences collide into non-1:1 keys and
 * abstain.
 */
function priorIdentityByBinding(
  priorGraph: UnifiedGraph,
  fnMatches: ReadonlyMap<string, string>,
  matchedPriorBindingNames: ReadonlySet<string>
): Map<babelTraverse.Binding, string> {
  const map = new Map<babelTraverse.Binding, string>();
  const { fns, bindings } = graphNodes(priorGraph);
  for (const fn of fns) {
    if (!fnMatches.has(fn.sessionId)) continue;
    const holder = holdingBindingOf(fn);
    if (holder) map.set(holder, `fn:${fn.sessionId}`);
  }
  for (const b of bindings) {
    if (!matchedPriorBindingNames.has(b.name)) continue;
    const binding = b.scope.getBinding(b.name);
    if (binding) map.set(binding, `bind:${b.name}`);
  }
  return map;
}

/**
 * A statement's reference-identity key: the sorted set of identity ids of
 * OUTER bindings it references that have a cross-version identity. Null
 * when it references none — no evidence, no pairing. References to
 * unmatched bindings are tolerated (absent from the key): bucket-level
 * bijection plus the structural/callee/role gates carry the precision.
 */
function statementRefKey(
  stmtPath: NodePath,
  identityByBinding: Map<babelTraverse.Binding, string>
): string | null {
  const ids = new Set<string>();
  const declCache = new Map<babelTraverse.Binding, boolean>();
  stmtPath.traverse({
    Identifier(idPath) {
      if (!idPath.isReferencedIdentifier()) return;
      const binding = idPath.scope.getBinding(idPath.node.name);
      if (!binding) return;
      const id = identityByBinding.get(binding);
      if (!id) return;
      let outer = declCache.get(binding);
      if (outer === undefined) {
        const declPath = binding.path as NodePath;
        outer = !(
          declPath.node === stmtPath.node || declPath.isDescendant(stmtPath)
        );
        declCache.set(binding, outer);
      }
      if (outer) ids.add(id);
    }
  });
  if (ids.size === 0) return null;
  return [...ids].sort().join("|");
}

/**
 * Hashes eligible for bucket pairing: present on both sides with EQUAL
 * counts above one. Unequal counts mean an insertion or removal landed in
 * the bucket — a prior member whose true successor changed (left the
 * bucket) could then be claimed by a genuinely-new same-shape statement
 * whose matched-ref key coincides (the orphan-claim that regressed the
 * big-feature eval pairs). Equal counts are the same discipline the
 * statement-align and enclosing-statement tiers use.
 */
function sharedNonUniqueHashes(
  freshSide: SideInventory,
  priorSide: SideInventory
): Set<string> {
  const shared = new Set<string>();
  for (const [hash, count] of freshSide.hashCounts) {
    if (count === 1) continue; // unique-twin path (or absent from prior)
    if (priorSide.hashCounts.get(hash) !== count) continue;
    shared.add(hash);
  }
  return shared;
}

/** hash → refKey → member indices, for one side's shared bucket members. */
function collectBucketKeys(
  side: SideInventory,
  sharedHashes: ReadonlySet<string>,
  identity: Map<babelTraverse.Binding, string>
): Map<string, Map<string, number[]>> {
  const out = new Map<string, Map<string, number[]>>();
  for (let i = 0; i < side.statements.length; i++) {
    const hash = side.hashes[i];
    if (!sharedHashes.has(hash)) continue;
    const key = statementRefKey(side.statements[i], identity);
    if (key === null) continue;
    let byKey = out.get(hash);
    if (!byKey) {
      byKey = new Map();
      out.set(hash, byKey);
    }
    let list = byKey.get(key);
    if (!list) {
      list = [];
      byKey.set(key, list);
    }
    list.push(i);
  }
  return out;
}

/**
 * Pair non-unique bucket members across sides by reference-identity key:
 * a key claimed by exactly ONE member on each side is an unambiguous
 * correspondence. Everything else abstains.
 */
function pairBucketsByRefKey(
  freshSide: SideInventory,
  priorSide: SideInventory,
  freshIdentity: Map<babelTraverse.Binding, string>,
  priorIdentity: Map<babelTraverse.Binding, string>
): Array<{ freshIdx: number; priorIdx: number }> {
  const sharedHashes = sharedNonUniqueHashes(freshSide, priorSide);
  if (sharedHashes.size === 0) return [];
  const freshByHashKey = collectBucketKeys(
    freshSide,
    sharedHashes,
    freshIdentity
  );
  const priorByHashKey = collectBucketKeys(
    priorSide,
    sharedHashes,
    priorIdentity
  );

  const pairs: Array<{ freshIdx: number; priorIdx: number }> = [];
  for (const [hash, freshKeys] of freshByHashKey) {
    const priorKeys = priorByHashKey.get(hash);
    if (!priorKeys) continue;
    for (const [key, freshIdxs] of freshKeys) {
      const priorIdxs = priorKeys.get(key);
      if (freshIdxs.length !== 1 || priorIdxs?.length !== 1) continue;
      pairs.push({ freshIdx: freshIdxs[0], priorIdx: priorIdxs[0] });
    }
  }
  return pairs;
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
  cross: CrossPairContext,
  stats: StatementTwinStats
): BridgedSlots {
  const { freshSide, priorSide, freshIdx, priorIdx } = twin;
  const freshFns = freshSide.fnsByStatement.get(freshIdx) ?? [];
  const freshBindings = freshSide.bindingsByStatement.get(freshIdx) ?? [];
  const none: BridgedSlots = { pairs: [], privateRenames: [], outerRefs: [] };
  if (
    !needsBridging(
      freshFns,
      freshBindings,
      input.claimedOldNames,
      priorIdx,
      cross
    )
  ) {
    return none;
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
    return none;
  }
  if (!declaredRolesAgree(priorBindings, freshBindings, input.fnMatches)) {
    stats.vetoedRole++;
    return none;
  }
  const bridged = bridgeTwinSlots(
    freshSide.statements[freshIdx],
    priorSide.statements[priorIdx],
    ownerCtx
  );
  if (bridged === null) {
    stats.vetoedStructural++;
    return none;
  }
  return bridged;
}

/**
 * Computes gated statement-twin transfer pairs. Must run while the prior
 * AST is alive; the result references only fresh-side Bindings.
 */
/** Probe toggles for eval ablation — flip per experiment run. */
const ENABLE_BUCKET_PAIRING = true;
const EMIT_OUTER_REF_VOTES = true;

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
    wrapperNode: input.newGraph.wrapperPath?.node ?? null,
    cascadeNameByOld: new Map(
      input.bindingIdentityPairs.map((p) => [p.oldName, p.newName])
    ),
    conflicts: []
  };
  const cross = buildCrossPairContext(input.fnMatches, priorSide);

  const takeBridged = (bridged: BridgedSlots) => {
    if (bridged.pairs.length > 0) {
      stats.transferredTwins++;
      stats.pairs += bridged.pairs.length;
      result.pairs.push(...bridged.pairs);
    }
    if (bridged.privateRenames.length > 0) {
      stats.privateRenames += bridged.privateRenames.length;
      result.privateRenames.push(...bridged.privateRenames);
    }
    if (EMIT_OUTER_REF_VOTES && bridged.outerRefs.length > 0) {
      stats.outerRefs += bridged.outerRefs.length;
      result.outerRefs.push(...bridged.outerRefs);
    }
  };

  for (let i = 0; i < freshSide.statements.length; i++) {
    const hash = freshSide.hashes[i];
    if (freshSide.hashCounts.get(hash) !== 1) continue;
    const priorIdx = priorSide.uniqueIndex.get(hash);
    if (priorIdx === undefined) continue;
    stats.uniqueTwins++;
    takeBridged(
      gateAndBridgeTwin(
        { freshSide, priorSide, freshIdx: i, priorIdx },
        input,
        ownerCtx,
        cross,
        stats
      )
    );
  }

  // Non-unique buckets: members paired by matched-reference identity —
  // the initializeApp-family case, where hundreds of same-shaped lazy
  // statements differ only in WHICH bound helper they reference.
  const bucketPairs = !ENABLE_BUCKET_PAIRING
    ? []
    : pairBucketsByRefKey(
        freshSide,
        priorSide,
        freshIdentityByBinding(
          input.newGraph,
          input.fnMatches,
          input.bindingIdentityPairs
        ),
        priorIdentityByBinding(
          input.priorGraph,
          input.fnMatches,
          new Set(input.bindingIdentityPairs.map((p) => p.newName))
        )
      );
  for (const { freshIdx, priorIdx } of bucketPairs) {
    stats.bucketTwins++;
    takeBridged(
      gateAndBridgeTwin(
        { freshSide, priorSide, freshIdx, priorIdx },
        input,
        ownerCtx,
        cross,
        stats
      )
    );
  }

  stats.cascadeConflicts = ownerCtx.conflicts.length;
  for (const c of ownerCtx.conflicts.slice(0, 12)) {
    debug.log(
      "prior-version",
      `statement-twin: cascade conflict ${c.oldName}: cascade=${c.cascadeName} twin=${c.twinName}`
    );
  }
  debug.log(
    "prior-version",
    `statement-twin: ${stats.uniqueTwins} unique twins + ${stats.bucketTwins} bucket-identity pairs, ` +
      `${stats.candidates} with pending work, ${stats.transferredTwins} bridged ` +
      `(${stats.pairs} pairs, ${stats.outerRefs} outer-ref votes, ${stats.privateRenames} private ids, ` +
      `${stats.cascadeConflicts} cascade conflicts); vetoes: ` +
      `callee=${stats.vetoedCallee} role=${stats.vetoedRole} structural=${stats.vetoedStructural}`
  );
  return result;
}
