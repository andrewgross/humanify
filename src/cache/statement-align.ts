/**
 * Statement-level content alignment for close-matched function pairs.
 *
 * Close matches transfer only function name + params through signature
 * position; their body locals were the largest remaining rename-noise
 * population. This module aligns top-level body statements between the
 * prior and new versions on rename-invariant content and bridges names
 * through per-statement placeholder slots — the same mechanism exact
 * matches use, applied per statement.
 *
 * Precision gates (a wrong transfer is worse than a missed one):
 * - Statements pair only within equal-count same-hash groups (unique
 *   hashes are the size-1 case). An unequal group means an edit landed
 *   inside it, so the whole group is skipped.
 * - Declaration anchor: a function-owned binding transfers only when its
 *   declaration lives inside an aligned statement — its defining content
 *   is provably unchanged. Use-sites alone never carry a name.
 * - Unanimity: conflicting prior names for one identifier drop it.
 * - Bindings owned by nested functions are skipped entirely (the nested
 *   function has its own match); outer bindings pass through as candidate
 *   pairs for vote propagation, which applies its own binding-identity
 *   gate and vote floor.
 */

import type { NodePath } from "@babel/core";
import type * as babelTraverse from "@babel/traverse";
import { hashPathWithMapping } from "../analysis/structural-hash.js";
import type { FunctionNode } from "../analysis/types.js";

interface HashedStatement {
  path: NodePath;
  hash: string;
  /** placeholder slot → identifier name, binding slots only */
  mapping: Map<string, string>;
}

/** Hash every top-level body statement of a function. */
function hashBodyStatements(fn: FunctionNode): HashedStatement[] {
  const body = fn.path.get("body");
  if (Array.isArray(body)) return [];
  const statementPaths: NodePath[] = body.isBlockStatement()
    ? (body.get("body") as NodePath[])
    : [body]; // arrow expression body: one pseudo-statement
  return statementPaths.map((path) => {
    const { hash, mapping } = hashPathWithMapping(path);
    return { path, hash, mapping };
  });
}

interface AlignedPair {
  prior: HashedStatement;
  next: HashedStatement;
}

function groupByHash(
  statements: HashedStatement[]
): Map<string, HashedStatement[]> {
  const groups = new Map<string, HashedStatement[]>();
  for (const statement of statements) {
    const list = groups.get(statement.hash) ?? [];
    list.push(statement);
    groups.set(statement.hash, list);
  }
  return groups;
}

/**
 * Content-aligns statements: same-hash groups with equal counts on both
 * sides pair by ordinal (source order). Unequal counts mean an insertion
 * or removal landed inside the group and every pairing after it would
 * shift — skip the group.
 */
function alignStatements(
  prior: HashedStatement[],
  next: HashedStatement[]
): AlignedPair[] {
  const priorGroups = groupByHash(prior);
  const nextGroups = groupByHash(next);
  const pairs: AlignedPair[] = [];
  for (const [hash, priorList] of priorGroups) {
    const nextList = nextGroups.get(hash);
    if (!nextList || nextList.length !== priorList.length) continue;
    for (let i = 0; i < priorList.length; i++) {
      pairs.push({ prior: priorList[i], next: nextList[i] });
    }
  }
  return pairs;
}

type OccurrenceKind = "anchored" | "outer" | "skip";

/**
 * Classifies what a slot occurrence may carry:
 * - anchored: binding owned by THIS function (not a nested one) and
 *   declared inside the aligned statement
 * - outer: binding declared outside the function — candidate for vote
 *   propagation downstream
 * - skip: use-site of a binding declared elsewhere in the function
 *   (params/fn name belong to the signature transfer; locals need their
 *   own declaration anchor), or a nested function's binding
 */
function classifyOccurrence(
  binding: babelTraverse.Binding,
  fn: FunctionNode,
  statementPath: NodePath
): OccurrenceKind {
  const declPath = binding.path as NodePath;
  if (!declPath.isDescendant(fn.path)) return "outer";
  const owningFnScope = binding.scope.getFunctionParent();
  if (owningFnScope !== fn.path.scope) return "skip"; // nested-function-owned
  if (declPath === statementPath || declPath.isDescendant(statementPath)) {
    return "anchored";
  }
  return "skip";
}

interface NameEvidence {
  priorNames: Set<string>;
  transferable: boolean;
}

/** Record one slot pair's evidence for a new-side identifier. */
function recordSlotEvidence(
  evidence: Map<string, NameEvidence>,
  pair: AlignedPair,
  slot: string,
  newName: string,
  fn: FunctionNode
): void {
  const priorName = pair.prior.mapping.get(slot);
  if (!priorName || priorName === newName) return;

  const binding = pair.next.path.scope.getBinding(newName);
  if (!binding) return;
  const kind = classifyOccurrence(binding, fn, pair.next.path);
  if (kind === "skip") return;

  let entry = evidence.get(newName);
  if (!entry) {
    entry = { priorNames: new Set(), transferable: false };
    evidence.set(newName, entry);
  }
  entry.priorNames.add(priorName);
  entry.transferable = true;
}

/**
 * Computes body-local name transfers for a close-matched pair by aligning
 * statements on rename-invariant content and bridging placeholder slots
 * within each aligned pair. Returns { minifiedName → priorName } for
 * names that are declaration-anchored (or outer-binding candidates) and
 * unanimous across all aligned statements.
 */
export function computeBodyLocalTransfers(
  priorFn: FunctionNode,
  newFn: FunctionNode
): Record<string, string> {
  const pairs = alignStatements(
    hashBodyStatements(priorFn),
    hashBodyStatements(newFn)
  );
  if (pairs.length === 0) return {};

  const evidence = new Map<string, NameEvidence>();
  for (const pair of pairs) {
    // Equal statement hashes guarantee aligned slot sets — same
    // serialization walk, ordinals by first occurrence.
    for (const [slot, newName] of pair.next.mapping) {
      recordSlotEvidence(evidence, pair, slot, newName, newFn);
    }
  }

  const transfers: Record<string, string> = {};
  for (const [newName, entry] of evidence) {
    if (!entry.transferable) continue;
    if (entry.priorNames.size !== 1) continue; // conflicting evidence — drop
    const priorName = [...entry.priorNames][0];
    transfers[newName] = priorName;
  }
  return transfers;
}
