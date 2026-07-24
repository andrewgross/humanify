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
  code?: string;
  ast?: t.File;
}

/** One top-level binding as a bucket member, plus (fresh side) its live
 * Binding for applying the rename. `declStart` is the declaration's source
 * offset — a stable ordering key (identical across a re-parse of identical
 * source) so `assignBucket`'s index tie-break is itself self-hop-stable. */
interface MemberInfo extends BucketMember {
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
function collectMembers(
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
 * prior name `to`. */
interface PlannedMove {
  binding: Binding;
  from: string;
  to: string;
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
        toApply.push({ binding, from: move.fromName, to: move.toName });
    }
  }
  return { toApply, buckets };
}

/**
 * Apply a bucket rename plan that may be a PERMUTATION (A→B while B→A).
 * Applying moves one at a time would hit `target-in-scope` — a swap
 * target is still occupied by the other member. So vacate every source to
 * a unique temporary first, then fill each target. A fill that cannot land
 * (its name is held by a binding OUTSIDE the plan) is rolled back to the
 * original name rather than shipped as a temp. Returns the count that
 * reached its intended target.
 */
function applyPlan(plan: readonly PlannedMove[]): number {
  const staged: Array<{
    binding: Binding;
    temp: string;
    from: string;
    to: string;
  }> = [];
  plan.forEach((move, i) => {
    const temp = `__familyPermuteSwap${i}$`;
    if (attemptValidatedRename(move.binding.scope, move.from, temp).applied) {
      staged.push({
        binding: move.binding,
        temp,
        from: move.from,
        to: move.to
      });
    }
  });
  let applied = 0;
  for (const s of staged) {
    if (attemptValidatedRename(s.binding.scope, s.temp, s.to).applied)
      applied++;
    else attemptValidatedRename(s.binding.scope, s.temp, s.from);
  }
  return applied;
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
  if (toApply.length === 0) return { applied: 0, buckets, skipped: 0 };

  const applied = applyPlan(toApply);
  if (applied === 0) return { applied: 0, buckets, skipped: toApply.length };

  const failure = checkStructuralInvariant(ast, baseline);
  if (failure) {
    debug.log(
      "family-permute",
      `discarded: pure-rename invariant violated (${failure.message})`
    );
    return undefined;
  }
  return {
    applied,
    buckets,
    skipped: toApply.length - applied,
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
