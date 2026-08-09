/**
 * exp036 idea 8b — diff-objective family permutation (post-render pass).
 *
 * The combined ambiguity+diff pass, done with the EVIDENCE the rendered
 * artifact carries rather than a blind pool pick. The matcher is
 * ambiguity-aware but pre-render; the reconcile is diff-aware but its
 * diff-hunk framing and corpus gate miss whole-bucket family rotation.
 * This pass groups top-level bindings by the statement hash of their
 * declaration (structurally interchangeable candidates), then within a
 * bucket:
 *
 *   1. LOCKS every name present on BOTH sides — it round-trips, it is
 *      correct, it is never touched (skipping this is what made the
 *      naive v1 rename `getClaudeCodeOAuthToken` → `deviceActionMap`).
 *   2. Moves only the ORPHANS — a fresh mint / re-draw adopts a prior
 *      name that went dead — and only when their MASKED USAGE CONTEXTS
 *      (the reference lines with each side's own name blanked) match.
 *      That call-site agreement is the caller evidence, read straight
 *      out of the diff.
 *
 * Applied through the validated-rename path with the reconcile step's
 * pure-rename structural invariant as the backstop (violation ⇒ discard
 * the whole pass). Deterministic, no LLM; best-effort and self-contained
 * like the reconcile step.
 */

import type { GeneratorOptions } from "@babel/generator";
import type { Binding } from "@babel/traverse";
import * as t from "@babel/types";
import { findWrapperFunction } from "../analysis/wrapper-detection.js";
import { generate, parseSourceAst, traverse } from "../babel-utils.js";
import { debug } from "../debug.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant
} from "../output-validation.js";
import { statementHash } from "../split/statement-hash.js";
import { assignBucket, type BucketMember } from "./family-permute.js";
import type { IsEligibleFn } from "./rename-eligibility.js";
import { attemptValidatedRename } from "./validated-rename.js";

export interface FamilyPermuteOutcome {
  /** Bindings reassigned to a prior name — an orphan adopting a dead name
   * or a cross-placed name swapped back to its context-matched member. */
  applied: number;
  /** Buckets in which at least one move was applied. */
  buckets: number;
  skipped: number;
  /** Every rename that reached the shipped artifact — `applied` with the
   * names attached. */
  moves: AppliedMove[];
  code?: string;
  ast?: t.File;
}

/**
 * One rename this pass shipped, with the evidence that justified it.
 *
 * The pass rewrites names in the FINAL artifact, so a wrong move ships a
 * wrong name; v1's did exactly that (`getClaudeCodeOAuthToken →
 * deviceActionMap`) and only a human reading the diff caught it. Counting
 * moves cannot catch that class — reading them can, which is what this
 * carries. It is also the attribution instrument for the eval: a hop where
 * the trail is empty cannot have had its KPIs moved by this pass, however
 * they read.
 */
export interface AppliedMove {
  from: string;
  to: string;
  /** Masked usage-context lines shared with the prior name it adopted. */
  support: number;
  /** The statement-hash bucket — moves sharing one key permuted together. */
  bucket: string;
}

/** One top-level binding as a bucket member, plus (fresh side) its live
 * Binding for applying the rename. `declStart` is the declaration's source
 * offset — a stable ordering key (identical across a re-parse of identical
 * source) so `assignBucket`'s index tie-break is itself self-hop-stable. */
export interface MemberInfo extends BucketMember {
  hash: string;
  declStart: number;
  binding?: Binding;
}

const WORD = /[A-Za-z_$][\w$]*/g;

/** A reference line with every whole-word occurrence of `name` blanked
 * to `\x00`, so a fresh mint and the prior name it replaced produce the
 * SAME context when everything else round-trips. */
function maskName(line: string, name: string): string {
  return line.replace(WORD, (w) => (w === name ? "\x00" : w));
}

/** The declaring statement of a top-level binding, for its hash. */
function declaringStatement(binding: Binding): t.Statement | null {
  const stmt = binding.path.isStatement()
    ? binding.path
    : binding.path.getStatementParent();
  return (stmt?.node as t.Statement) ?? null;
}

/** Source line (1-based) → text, for masking reference contexts. */
function lineIndex(code: string): string[] {
  return code.split("\n");
}

/** Collect one side's top-level bindings as bucket members. Fresh side
 * carries the live Binding; both sides carry masked usage contexts. */
/**
 * Exported so a ceiling probe can ask the SHIPPED question — "does masked usage
 * context distinguish these same-hash members?" — instead of re-deriving it. A
 * probe that approximates this would be measuring its own approximation
 * (measurement-pitfalls rule 4).
 */
export function collectMembers(
  ast: t.File,
  code: string,
  withBindings: boolean
): MemberInfo[] {
  const lines = lineIndex(code);
  const wrapper = findWrapperFunction(ast);
  const scope =
    wrapper && t.isBlockStatement(wrapper.functionPath.node.body)
      ? wrapper.scope
      : null;
  const members: MemberInfo[] = [];
  const seen = new Set<Binding>();

  const consider = (name: string, binding: Binding) => {
    if (seen.has(binding)) return;
    seen.add(binding);
    const stmt = declaringStatement(binding);
    if (!stmt) return;
    const contexts: string[] = [];
    for (const ref of binding.referencePaths) {
      const ln = ref.node.loc?.start.line;
      if (ln && lines[ln - 1] !== undefined) {
        contexts.push(maskName(lines[ln - 1], name));
      }
    }
    members.push({
      name,
      contexts,
      hash: statementHash(stmt),
      declStart: stmt.start ?? 0,
      binding: withBindings ? binding : undefined
    });
  };

  const collectFromScope = (s: { bindings: Record<string, Binding> }) => {
    for (const [name, binding] of Object.entries(s.bindings)) {
      consider(name, binding);
    }
  };

  if (scope)
    collectFromScope(scope as unknown as { bindings: Record<string, Binding> });
  traverse(ast, {
    Program(path) {
      collectFromScope(
        path.scope as unknown as { bindings: Record<string, Binding> }
      );
      path.stop();
    }
  });
  return members;
}

/** Group members by declaration hash. */
function byHash(members: MemberInfo[]): Map<string, MemberInfo[]> {
  const map = new Map<string, MemberInfo[]>();
  for (const m of members) {
    const list = map.get(m.hash);
    if (list) list.push(m);
    else map.set(m.hash, [m]);
  }
  return map;
}

/** A bucket's members in stable declaration order, so `assignBucket`'s
 * index tie-break resolves identically on a re-parse of the same source
 * (the property the self-hop invariant checks). */
function byDeclOrder(members: MemberInfo[]): MemberInfo[] {
  return [...members].sort((a, b) => a.declStart - b.declStart);
}

/** One planned reassignment: give `binding` (currently named `from`) the
 * prior name `to`. Carries the move's evidence so a move that SHIPS can be
 * read back with the support and bucket that justified it. */
interface PlannedMove extends AppliedMove {
  binding: Binding;
}

/** Turn each hash bucket into concrete moves. A bucket needs ≥2 prior
 * members to be an interchangeable family; within it `assignBucket`
 * (fed stable declaration order) decides the moves, and we resolve each
 * fresh name back to its live binding. */
function planBucketMoves(
  freshByHash: Map<string, MemberInfo[]>,
  priorByHash: Map<string, MemberInfo[]>,
  isEligible: IsEligibleFn
): { toApply: PlannedMove[]; buckets: number } {
  const toApply: PlannedMove[] = [];
  let buckets = 0;
  for (const [hash, freshMembers] of freshByHash) {
    const priorMembers = priorByHash.get(hash);
    if (!priorMembers || priorMembers.length < 2) continue;
    const moves = assignBucket(
      byDeclOrder(freshMembers),
      byDeclOrder(priorMembers),
      isEligible
    );
    if (moves.length === 0) continue;
    buckets++;
    const byName = new Map(freshMembers.map((m) => [m.name, m.binding]));
    for (const move of moves) {
      const binding = byName.get(move.fromName);
      if (binding)
        toApply.push({
          binding,
          from: move.fromName,
          to: move.toName,
          support: move.support,
          bucket: hash
        });
    }
  }
  return { toApply, buckets };
}

/**
 * Apply a bucket rename plan that may be a PERMUTATION (A→B while B→A).
 * Applying moves one at a time would hit `target-in-scope` — a swap
 * target is still occupied by the other member. So vacate every source to
 * a unique temporary first, then fill each target. Buckets apply
 * independently: a fill that cannot land voids only its own bucket's
 * remainder, never another bucket's moves. Returns the moves that reached
 * their intended target — the ones that will ship.
 */
function applyPlan(plan: readonly PlannedMove[]): AppliedMove[] {
  const byBucket = new Map<string, PlannedMove[]>();
  for (const move of plan) {
    const list = byBucket.get(move.bucket) ?? [];
    list.push(move);
    byBucket.set(move.bucket, list);
  }
  const applied: AppliedMove[] = [];
  for (const bucketPlan of byBucket.values()) {
    applied.push(...applyBucketPlan(bucketPlan));
  }
  return applied;
}

/**
 * Drop moves whose target is held by a binding that is NOT itself moving
 * away — such a fill can never land, and its naive rollback can strand a
 * swap temp in the artifact when a chain-mate has already claimed its
 * vacated name (a shipped `__familyPermuteSwapN$` is invisible to the
 * name-blind structural invariant and absent from the move trail).
 * Iterated to a fixpoint: dropping a move re-parks its binding on its old
 * name, which can block a chain-mate's target in turn.
 */
function landableMoves(plan: readonly PlannedMove[]): PlannedMove[] {
  let live = [...plan];
  for (;;) {
    const movingAway = new Set(live.map((m) => m.binding));
    const next = live.filter((move) => {
      const holder = move.binding.scope.getBinding(move.to);
      return !holder || movingAway.has(holder);
    });
    if (next.length === live.length) return live;
    live = next;
  }
}

/** Vacate-then-fill for ONE bucket. Every fill target is provably free
 * after the pre-drop, so a failing fill is an invariant breach — the
 * whole bucket reverts to its original names rather than ship a temp. */
function applyBucketPlan(plan: readonly PlannedMove[]): AppliedMove[] {
  const staged: Array<PlannedMove & { temp: string }> = [];
  landableMoves(plan).forEach((move, i) => {
    const temp = `__familyPermuteSwap${i}$${move.bucket.slice(0, 8)}`;
    if (attemptValidatedRename(move.binding.scope, move.from, temp).applied) {
      staged.push({ ...move, temp });
    }
  });
  const filled: Array<PlannedMove & { temp: string }> = [];
  for (const s of staged) {
    if (attemptValidatedRename(s.binding.scope, s.temp, s.to).applied) {
      filled.push(s);
    } else {
      revertBucket(staged, filled);
      return [];
    }
  }
  return filled.map((s) => ({
    from: s.from,
    to: s.to,
    support: s.support,
    bucket: s.bucket
  }));
}

/** Undo a bucket mid-apply: filled members step back to their temps, then
 * every staged member restores its original name. Each step targets a
 * name this bucket itself just freed, so restoration cannot collide. */
function revertBucket(
  staged: ReadonlyArray<PlannedMove & { temp: string }>,
  filled: ReadonlyArray<PlannedMove & { temp: string }>
): void {
  for (const s of filled) {
    attemptValidatedRename(s.binding.scope, s.to, s.temp);
  }
  for (const s of staged) {
    attemptValidatedRename(s.binding.scope, s.temp, s.from);
  }
}

/**
 * Collect the prior side's bucket members, then let the multi-GB prior AST
 * go BEFORE any fresh-side work. Prior members are plain data — no live
 * bindings pin the prior graph — so once this returns the prior AST is
 * collectable. This is the release discipline every other prior-touching
 * pass follows (the matcher's `clearBabelCacheAfterPriorMatch`, the split's
 * `renameResult.ast = undefined`); without it the prior graph coexists with
 * the fresh AST and survives into the split phase, OOMing the largest
 * bundle (measured: 2.1.216 splits at 14GB with this pass off, OOMs with it
 * on).
 */
function collectPriorByHash(
  priorCode: string
): Map<string, MemberInfo[]> | undefined {
  const priorAst = parseSourceAst(priorCode);
  if (!priorAst) return undefined;
  return byHash(collectMembers(priorAst, priorCode, false));
}

function familyPermuteInternal(
  code: string,
  priorCode: string,
  isEligible: IsEligibleFn,
  genOpts: GeneratorOptions
): FamilyPermuteOutcome | undefined {
  // Prior first, and released, so the two big ASTs never coexist.
  const priorByHash = collectPriorByHash(priorCode);
  if (!priorByHash) return undefined;

  const ast = parseSourceAst(code);
  if (!ast) return undefined;
  const baseline = captureSemanticBaseline(ast);
  const freshByHash = byHash(collectMembers(ast, code, true));

  const { toApply, buckets } = planBucketMoves(
    freshByHash,
    priorByHash,
    isEligible
  );
  if (toApply.length === 0)
    return { applied: 0, buckets, skipped: 0, moves: [] };

  const moves = applyPlan(toApply);
  if (moves.length === 0)
    return { applied: 0, buckets, skipped: toApply.length, moves: [] };

  const failure = checkStructuralInvariant(ast, baseline);
  if (failure) {
    debug.log(
      "family-permute",
      `discarded: pure-rename invariant violated (${failure.message})`
    );
    return undefined;
  }
  // One line per SHIPPED rename, because this pass is the one that rewrites
  // names in the final artifact and a count cannot show a wrong one. `-vv
  // --log-file` puts these in the run log, where the eval reads them back.
  for (const m of moves) {
    debug.log(
      "family-permute",
      `move ${m.from} -> ${m.to} (support ${m.support}, bucket ${m.bucket})`
    );
  }
  return {
    applied: moves.length,
    buckets,
    skipped: toApply.length - moves.length,
    moves,
    code: generate(ast, genOpts).code,
    ast
  };
}

/**
 * Best-effort post-render family permutation. Returns undefined when the
 * pass could not run, applied nothing that survived validation, or hit
 * any error (never throws — an optional pass must not abort a completed
 * run).
 */
export function runFamilyPermute(
  code: string,
  priorCode: string,
  isEligible: IsEligibleFn,
  genOpts: GeneratorOptions
): FamilyPermuteOutcome | undefined {
  try {
    return familyPermuteInternal(code, priorCode, isEligible, genOpts);
  } catch (err) {
    debug.log(
      "family-permute",
      `skipped: ${err instanceof Error ? err.message : String(err)}`
    );
    return undefined;
  }
}
