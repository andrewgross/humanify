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
import { assignByContext, type BucketMember } from "./family-permute.js";
import type { IsEligibleFn } from "./rename-eligibility.js";
import { attemptValidatedRename } from "./validated-rename.js";

export interface FamilyPermuteOutcome {
  /** Orphan bindings whose names were adopted from a dead prior name. */
  applied: number;
  /** Buckets with orphans considered. */
  buckets: number;
  skipped: number;
  code?: string;
  ast?: t.File;
}

/** One top-level binding as a bucket member, plus (fresh side) its live
 * Binding for applying the rename. */
interface MemberInfo extends BucketMember {
  hash: string;
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

function familyPermuteInternal(
  code: string,
  priorCode: string,
  isEligible: IsEligibleFn,
  genOpts: GeneratorOptions
): FamilyPermuteOutcome | undefined {
  const ast = parseSourceAst(code);
  if (!ast) return undefined;
  const baseline = captureSemanticBaseline(ast);

  const priorAst = parseSourceAst(priorCode);
  if (!priorAst) return undefined;

  const freshByHash = byHash(collectMembers(ast, code, true));
  const priorByHash = byHash(collectMembers(priorAst, priorCode, false));

  const toApply: Array<{ binding: Binding; from: string; to: string }> = [];
  let buckets = 0;
  for (const [hash, freshMembers] of freshByHash) {
    const priorMembers = priorByHash.get(hash);
    if (!priorMembers || priorMembers.length < 2) continue;
    const moves = assignByContext(freshMembers, priorMembers, isEligible);
    if (moves.length === 0) continue;
    buckets++;
    const byName = new Map(freshMembers.map((m) => [m.name, m.binding]));
    for (const move of moves) {
      const binding = byName.get(move.fromName);
      if (binding) {
        toApply.push({ binding, from: move.fromName, to: move.toName });
      }
    }
  }
  if (toApply.length === 0) return { applied: 0, buckets, skipped: 0 };

  let applied = 0;
  for (const { binding, from, to } of toApply) {
    const attempt = attemptValidatedRename(binding.scope, from, to);
    if (attempt.applied) applied++;
  }
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
