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
import {
  computeStructuralSignature,
  hashPathWithMapping
} from "../analysis/structural-hash.js";
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

/**
 * Recursion budget for descending into changed container statements. Each
 * else-if link of a chain costs one level (if → alternate-if → ...), and
 * real bundles nest transport-style chains inside try blocks 5-6 branches
 * deep — the old budget of 4 stopped exactly there, so locals in the tail
 * branches were never anchored and the LLM re-named them every version
 * hop. Descent only follows type-unique remainders, so a deep budget adds
 * no ambiguity, and each level hashes a strictly smaller subtree.
 */
const MAX_ALIGN_DEPTH = 16;

/**
 * Unaligned-remainder pairs whose statement node type appears exactly
 * once on each side — the only pairing that is unambiguous without
 * content evidence. Two changed same-type siblings (e.g. two edited if
 * statements) stay unpaired: positional pairing there would be a guess,
 * and a wrong container pair could align generic same-hash inner
 * statements across unrelated code.
 */
function typeUniquePairs(
  restPrior: HashedStatement[],
  restNext: HashedStatement[]
): Array<[HashedStatement, HashedStatement]> {
  const byType = (units: HashedStatement[]) => {
    const map = new Map<string, HashedStatement[]>();
    for (const u of units) {
      const list = map.get(u.path.node.type) ?? [];
      list.push(u);
      map.set(u.path.node.type, list);
    }
    return map;
  };
  const priorByType = byType(restPrior);
  const nextByType = byType(restNext);
  const pairs: Array<[HashedStatement, HashedStatement]> = [];
  for (const [type, priorList] of priorByType) {
    const nextList = nextByType.get(type);
    if (priorList.length === 1 && nextList?.length === 1) {
      pairs.push([priorList[0], nextList[0]]);
    }
  }
  return pairs;
}

/**
 * Aligns two unit lists, then descends into unaligned container
 * statements that pair unambiguously by node type — an edit nested
 * inside an if/try/loop/switch leaves the container's hash changed while
 * its untouched inner statements still align. A tentative pair that is
 * actually unrelated aligns nothing inside and contributes nothing.
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

  for (const [priorUnit, nextUnit] of typeUniquePairs(restPrior, restNext)) {
    const blockPairs = correspondingBlocks(priorUnit.path, nextUnit.path);
    for (const [priorBlock, nextBlock] of blockPairs) {
      pairs.push(
        ...collectAlignedPairs(
          hashUnits(priorBlock),
          hashUnits(nextBlock),
          depth + 1
        )
      );
    }
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
  // Walks dotted keys one segment at a time: `get("handler.body")` throws
  // inside Babel when `handler` is null (a try/finally with no catch).
  const child = (path: NodePath, key: string): NodePath | null => {
    let current: NodePath | null = path;
    for (const part of key.split(".")) {
      if (!current?.node) return null;
      const got = current.get(part);
      current = Array.isArray(got) ? null : (got as NodePath);
    }
    return current?.node ? current : null;
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
  if (prior.isSwitchStatement() && next.isSwitchStatement()) {
    return switchCasePairs(prior, next);
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

/**
 * Case-body unit lists of two switch statements. Cases pair positionally,
 * gated on an exact-literal test match (or both default) — the length-
 * normalized statement hash would treat `case "open"` and `case "data"`
 * as equal, so a reordered case could cross-pair bodies without the
 * literal-preserving signature. A mismatched position pairs nothing.
 */
function switchCasePairs(
  prior: NodePath,
  next: NodePath
): Array<[NodePath[], NodePath[]]> {
  const priorCases = prior.get("cases");
  const nextCases = next.get("cases");
  if (
    !Array.isArray(priorCases) ||
    !Array.isArray(nextCases) ||
    priorCases.length !== nextCases.length
  ) {
    return [];
  }
  const testSignature = (casePath: NodePath): string | null => {
    const test = casePath.get("test");
    if (Array.isArray(test) || !test.node) return null; // default:
    return computeStructuralSignature(test);
  };
  const pairs: Array<[NodePath[], NodePath[]]> = [];
  for (let i = 0; i < priorCases.length; i++) {
    if (testSignature(priorCases[i]) !== testSignature(nextCases[i])) continue;
    pairs.push([
      priorCases[i].get("consequent") as NodePath[],
      nextCases[i].get("consequent") as NodePath[]
    ]);
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
