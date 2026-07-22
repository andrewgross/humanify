/**
 * Binding role evidence for single-vote name pinning.
 *
 * A module binding whose recovered prior name has only ONE agreeing vote
 * (below the 2-vote propagation floor) may still inherit it — but only
 * when the prior and new bindings provably play the same role. The role
 * is compact plain data (no AST references) so the prior side can be
 * computed while the prior AST is alive and carried past its release.
 *
 * Content shingles are slot-BLIND: binding-slot ordinals are assigned in
 * walk order, so one inserted declaration inside a large initializer
 * would renumber every later slot and crater the similarity of otherwise
 * unchanged content. Blinding the ordinal (keeping the fact that a slot
 * sits there) makes the shingles insertion-robust while literals,
 * property keys, and free identifiers stay verbatim — the discriminating
 * content the structural hash also preserves.
 */
import type { FunctionNode, ModuleBindingNode } from "../analysis/types.js";
import { resolveBindingContentPath } from "../analysis/function-graph.js";
import { jaccardSimilarity } from "../analysis/function-fingerprint.js";
import { serializePathTokens } from "../analysis/structural-hash.js";

export interface BindingRole {
  /** Content hash from the binding's fingerprint; null when unhashable. */
  structuralHash: string | null;
  /** Slot-blind, literal-preserving k-gram shingles of the binding's
   *  content; null when the binding has no init and no assignment. */
  contentShingles: Set<string> | null;
  /** Session ids of FUNCTION callees referenced by the initializer. */
  fnCalleeIds: string[];
  /** True when the initializer also references module bindings — the
   *  callee comparison is then inconclusive and must not veto. */
  hasBindingCallees: boolean;
}

/** Tokens per shingle k-gram. */
const SHINGLE_K = 4;
/** Deterministic cap on shingles kept per binding (walk order). */
const SHINGLE_CAP = 2048;
/** Minimum shingle overlap for two roles to count as the same binding. */
export const SINGLE_VOTE_CONTENT_FLOOR = 0.5;

/** Binding-slot (`$3`) and label-slot (`L1`) tokens, ordinal-blinded. */
const SLOT_TOKEN = /^\$\d+$/;
const LABEL_TOKEN = /^L\d+$/;

function blindSlotOrdinal(token: string): string {
  if (SLOT_TOKEN.test(token)) return "$";
  if (LABEL_TOKEN.test(token)) return "L";
  return token;
}

/**
 * Slot-blind k-gram shingles over a path's serialized token stream.
 * Streams shorter than k yield one shingle of the whole stream, so tiny
 * contents (`null`, a single literal) still compare.
 */
export function computeContentShingles(
  path: import("@babel/traverse").NodePath
): Set<string> {
  const tokens = serializePathTokens(path, { preserveLiterals: true }).map(
    blindSlotOrdinal
  );
  const shingles = new Set<string>();
  if (tokens.length <= SHINGLE_K) {
    shingles.add(tokens.join("\u0000"));
    return shingles;
  }
  for (let i = 0; i + SHINGLE_K <= tokens.length; i++) {
    shingles.add(tokens.slice(i, i + SHINGLE_K).join("\u0000"));
    if (shingles.size >= SHINGLE_CAP) break;
  }
  return shingles;
}

/**
 * Computes the compact role evidence for a module binding. Works on
 * either side of a cross-version pair — the prior side is computed while
 * the prior AST is alive; the result holds no AST references.
 */
export function computeBindingRole(node: ModuleBindingNode): BindingRole {
  const callees = splitCallees(node.internalCallees);

  const babelBinding = node.scope.bindings[node.name];
  const contentPath = babelBinding
    ? resolveBindingContentPath(babelBinding)
    : null;

  return {
    structuralHash: node.fingerprint?.structuralHash ?? null,
    contentShingles: contentPath ? computeContentShingles(contentPath) : null,
    ...callees
  };
}

/**
 * Role evidence for a function declaration head — same shape as module
 * bindings, so the shared single-vote ladder can gate cold function
 * heads (tiny same-shaped functions whose family defeats the cascade).
 */
export function computeFunctionRole(fn: FunctionNode): BindingRole {
  return {
    structuralHash: fn.fingerprint?.structuralHash ?? null,
    contentShingles: computeContentShingles(fn.path),
    ...splitCallees(fn.internalCallees)
  };
}

/** Split callees into function ids and a module-binding-presence flag. */
function splitCallees(
  internalCallees: Iterable<{ sessionId: string }>
): Pick<BindingRole, "fnCalleeIds" | "hasBindingCallees"> {
  const fnCalleeIds: string[] = [];
  let hasBindingCallees = false;
  for (const callee of internalCallees) {
    if (callee.sessionId.startsWith("module:")) {
      hasBindingCallees = true;
    } else {
      fnCalleeIds.push(callee.sessionId);
    }
  }
  return { fnCalleeIds, hasBindingCallees };
}

/** Verdict with a log-friendly reason. */
export interface RoleAgreement {
  agrees: boolean;
  reason: string;
}

/**
 * Whether two roles are the same binding across versions. Content must
 * positively corroborate (equal non-null hashes, or shingle overlap at
 * the floor) — missing evidence is a refusal, not agreement. When both
 * sides have pure-function callee sets, the prior's callees mapped
 * through the function matches must equal the new side's — structurally
 * identical wrappers around DIFFERENT functions (twins) hash equal, and
 * the callee identity is the one signal that tells them apart. A prior
 * callee with no match is inconclusive (no veto), as is any module
 * binding among the callees.
 */
export function bindingRolesAgree(
  prior: BindingRole,
  next: BindingRole,
  priorToNewFnIds: ReadonlyMap<string, string>
): RoleAgreement {
  const content = contentAgreement(prior, next);
  if (!content.agrees) return content;

  const veto = calleeVeto(prior, next, priorToNewFnIds);
  if (veto) return veto;

  return content;
}

/** Positive content corroboration: hash equality or shingle overlap. */
function contentAgreement(
  prior: BindingRole,
  next: BindingRole
): RoleAgreement {
  if (
    prior.structuralHash !== null &&
    prior.structuralHash === next.structuralHash
  ) {
    return { agrees: true, reason: "hash-equal" };
  }
  if (
    prior.contentShingles?.size &&
    next.contentShingles?.size &&
    jaccardSimilarity(prior.contentShingles, next.contentShingles) >=
      SINGLE_VOTE_CONTENT_FLOOR
  ) {
    return { agrees: true, reason: "shingle-overlap" };
  }
  if (!prior.contentShingles?.size || !next.contentShingles?.size) {
    return { agrees: false, reason: "no-content-evidence" };
  }
  return { agrees: false, reason: "content-below-floor" };
}

/** Callee-identity veto, or null when inconclusive/agreeing. */
function calleeVeto(
  prior: BindingRole,
  next: BindingRole,
  priorToNewFnIds: ReadonlyMap<string, string>
): RoleAgreement | null {
  if (prior.hasBindingCallees || next.hasBindingCallees) return null;
  if (prior.fnCalleeIds.length === 0 || next.fnCalleeIds.length === 0) {
    return null;
  }
  const mapped: string[] = [];
  for (const priorId of prior.fnCalleeIds) {
    const newId = priorToNewFnIds.get(priorId);
    if (!newId) return null; // unmatched prior callee — inconclusive
    mapped.push(newId);
  }
  const expected = [...new Set(mapped)].sort().join("|");
  const actual = [...new Set(next.fnCalleeIds)].sort().join("|");
  if (expected !== actual) {
    return { agrees: false, reason: "callee-mismatch" };
  }
  return null;
}
