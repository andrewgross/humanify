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
import type { TransferPair } from "../rename/lifecycle.js";

interface HashedStatement {
  path: NodePath;
  hash: string;
  /** placeholder slot → identifier name, binding slots only */
  mapping: Map<string, string>;
  /** placeholder slot → resolved Binding (same walk as `mapping`) */
  bindings: Map<string, babelTraverse.Binding>;
}

/**
 * The paths a body aligns on. An arrow expression body and a block that
 * is exactly `return <expr>;` are the same function written two ways —
 * both normalize to the returned expression, so a style change between
 * versions doesn't zero out the pair's alignment evidence.
 */
function alignmentUnits(fn: FunctionNode): NodePath[] {
  const body = fn.path.get("body");
  if (Array.isArray(body)) return [];
  if (!body.isBlockStatement()) return [body]; // expression body
  const statements = body.get("body") as NodePath[];
  if (statements.length === 1 && statements[0].isReturnStatement()) {
    const argument = statements[0].get("argument");
    if (!Array.isArray(argument) && argument.node) {
      return [argument as NodePath];
    }
  }
  return statements;
}

/** Hash every alignment unit of a function body. */
function hashBodyStatements(fn: FunctionNode): HashedStatement[] {
  return hashUnits(alignmentUnits(fn));
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

/** Recursion budget for descending into changed container statements. */
const MAX_ALIGN_DEPTH = 4;

/**
 * Aligns two unit lists, then descends into the ONE remaining unaligned
 * statement pair when both sides have exactly one of the same node type —
 * an edit nested inside a lone if/try/loop leaves the container's hash
 * changed while its untouched inner statements still align. Recursing
 * only on an unambiguous single-pair remainder keeps the pairing sound;
 * a tentative pair that is actually unrelated aligns nothing inside and
 * contributes nothing.
 */
function collectAlignedPairs(
  prior: HashedStatement[],
  next: HashedStatement[],
  depth: number
): AlignedPair[] {
  const pairs = alignStatements(prior, next);
  if (depth >= MAX_ALIGN_DEPTH) return pairs;

  const alignedPrior = new Set(pairs.map((p) => p.prior));
  const alignedNext = new Set(pairs.map((p) => p.next));
  const restPrior = prior.filter((u) => !alignedPrior.has(u));
  const restNext = next.filter((u) => !alignedNext.has(u));
  if (
    restPrior.length !== 1 ||
    restNext.length !== 1 ||
    restPrior[0].path.node.type !== restNext[0].path.node.type
  ) {
    return pairs;
  }

  const blockPairs = correspondingBlocks(restPrior[0].path, restNext[0].path);
  for (const [priorBlock, nextBlock] of blockPairs) {
    pairs.push(
      ...collectAlignedPairs(
        hashUnits(priorBlock),
        hashUnits(nextBlock),
        depth + 1
      )
    );
  }
  return pairs;
}

/** Child statement lists of a block path, or the single statement itself. */
function unitsOf(path: NodePath | null | undefined): NodePath[] {
  if (!path?.node) return [];
  if (path.isBlockStatement()) return path.get("body") as NodePath[];
  return [path];
}

function hashUnits(paths: NodePath[]): HashedStatement[] {
  return paths.map((path) => {
    const { hash, mapping, bindings } = hashPathWithMapping(path);
    return { path, hash, mapping, bindings };
  });
}

/**
 * The corresponding child-block unit lists of two same-typed container
 * statements (if/else branches, loop bodies, try/catch/finally blocks).
 * Non-container statements yield nothing.
 */
function correspondingBlocks(
  prior: NodePath,
  next: NodePath
): Array<[NodePath[], NodePath[]]> {
  const zip = (
    a: NodePath | null | undefined,
    b: NodePath | null | undefined
  ) => [unitsOf(a), unitsOf(b)] as [NodePath[], NodePath[]];
  const child = (path: NodePath, key: string): NodePath | null => {
    const got = path.get(key);
    return Array.isArray(got) ? null : (got as NodePath);
  };

  if (prior.isIfStatement() && next.isIfStatement()) {
    return [
      zip(child(prior, "consequent"), child(next, "consequent")),
      zip(child(prior, "alternate"), child(next, "alternate"))
    ];
  }
  if (prior.isTryStatement() && next.isTryStatement()) {
    return [
      zip(child(prior, "block"), child(next, "block")),
      zip(child(prior, "handler.body"), child(next, "handler.body")),
      zip(child(prior, "finalizer"), child(next, "finalizer"))
    ];
  }
  if (
    (prior.isForStatement() ||
      prior.isForOfStatement() ||
      prior.isForInStatement() ||
      prior.isWhileStatement() ||
      prior.isDoWhileStatement() ||
      prior.isBlockStatement() ||
      prior.isLabeledStatement()) &&
    next.node.type === prior.node.type
  ) {
    const key = prior.isBlockStatement() ? null : "body";
    if (key === null) return [zip(prior, next)];
    return [zip(child(prior, key), child(next, key))];
  }
  return [];
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

interface BindingEvidence {
  newName: string;
  priorNames: Set<string>;
}

/**
 * Record one slot pair's evidence, keyed by the slot's resolved BINDING.
 * Name-keyed evidence collapsed same-named sibling bindings onto one key
 * — their (different) prior names then failed unanimity and ALL of them
 * were dropped. The slot walk already resolved the exact binding, which
 * also covers block-scoped declarations inside aligned container
 * statements that a scope lookup from the statement path cannot see.
 */
function recordSlotEvidence(
  evidence: Map<babelTraverse.Binding, BindingEvidence>,
  pair: AlignedPair,
  slot: string,
  newName: string,
  fn: FunctionNode
): void {
  const priorName = pair.prior.mapping.get(slot);
  if (!priorName || priorName === newName) return;

  const binding = pair.next.bindings.get(slot);
  if (!binding) return;
  const kind = classifyOccurrence(binding, fn, pair.next.path);
  if (kind === "skip") return;

  let entry = evidence.get(binding);
  if (!entry) {
    entry = { newName, priorNames: new Set() };
    evidence.set(binding, entry);
  }
  entry.priorNames.add(priorName);
}

export interface BodyAlignment {
  /** Binding-carried pairs for anchored, per-binding-unanimous names */
  transfers: TransferPair[];
  /** Content-aligned statement pairs — the pair's corroboration evidence */
  alignedStatements: number;
  /** Top-level statements in the NEW body (denominator for coverage) */
  totalNewStatements: number;
}

/**
 * Computes body-local name transfers for a close-matched pair by aligning
 * statements on rename-invariant content and bridging placeholder slots
 * within each aligned pair. Transfers cover names that are
 * declaration-anchored (or outer-binding candidates) and unanimous across
 * all aligned statements. The aligned-statement count doubles as the
 * pair's content corroboration — a close pair sharing ZERO identical
 * normalized statements is a shape coincidence, and callers must not
 * transfer anything for it.
 */
export function computeBodyLocalTransfers(
  priorFn: FunctionNode,
  newFn: FunctionNode
): BodyAlignment {
  const nextStatements = hashBodyStatements(newFn);
  const pairs = collectAlignedPairs(
    hashBodyStatements(priorFn),
    nextStatements,
    0
  );
  const result: BodyAlignment = {
    transfers: [],
    alignedStatements: pairs.length,
    totalNewStatements: nextStatements.length
  };
  if (pairs.length === 0) return result;

  const evidence = new Map<babelTraverse.Binding, BindingEvidence>();
  for (const pair of pairs) {
    // Equal statement hashes guarantee aligned slot sets — same
    // serialization walk, ordinals by first occurrence.
    for (const [slot, newName] of pair.next.mapping) {
      recordSlotEvidence(evidence, pair, slot, newName, newFn);
    }
  }

  for (const [binding, entry] of evidence) {
    if (entry.priorNames.size !== 1) continue; // conflicting evidence — drop
    const priorName = [...entry.priorNames][0];
    result.transfers.push({
      oldName: entry.newName,
      newName: priorName,
      binding
    });
  }
  return result;
}
