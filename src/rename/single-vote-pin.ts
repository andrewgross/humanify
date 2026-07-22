/**
 * Single-vote name pinning — the shared precision ladder for inheriting a
 * prior name recovered by exactly ONE exact-matched function's slot
 * testimony. Two propagation targets sit below the 2-vote agreement
 * floor and use it: unmatched module bindings and cold function
 * declaration heads (tiny same-shaped functions whose family defeats the
 * match cascade, so the LLM re-invents their names every hop).
 *
 * Precision gates, in order — testimony strength (exact slot pairs
 * only), injectivity (the name must have exactly one claimant across all
 * vote maps), role corroboration (content agreement + callee veto),
 * validated rename (collisions reject, no retry: a held token means the
 * name has a better owner elsewhere).
 */
import type { Scope } from "@babel/traverse";
import {
  type BindingRole,
  bindingRolesAgree
} from "../prior-version/binding-role.js";
import { attemptValidatedRename } from "./validated-rename.js";

/** Per-name vote tally, exact-slot-sourced votes tracked separately. */
export interface VoteCount {
  total: number;
  exact: number;
}

export interface SingleVotePinRequest {
  /** All name votes recorded for this binding. */
  votes: Map<string, VoteCount>;
  /** Distinct bindings each proposed name has votes on (all vote maps). */
  nameClaimants: Map<string, number>;
  /** Role evidence for unconsumed prior names, keyed by prior name. */
  priorRoles: ReadonlyMap<string, BindingRole>;
  /** prior→new function session ids, for the role callee veto. */
  fnMatches: ReadonlyMap<string, string>;
  /** Fresh-side role, computed lazily once the vote gates pass. */
  freshRole: () => BindingRole;
  scope: Scope;
  oldName: string;
}

export type SingleVotePinResult =
  | { pinned: true; name: string; roleReason: string }
  | { pinned: false; blocked?: string };

/**
 * Run the ladder and apply the rename on success. `blocked` is unset only
 * when there was no single-name vote to consider.
 */
export function trySingleVotePin(
  req: SingleVotePinRequest
): SingleVotePinResult {
  if (req.votes.size !== 1) return { pinned: false };
  const [name, count] = [...req.votes][0];
  if (count.total !== 1 || count.exact !== 1) {
    return { pinned: false, blocked: "non-exact-source" };
  }
  if (req.nameClaimants.get(name) !== 1) {
    return { pinned: false, blocked: "name-conflict" };
  }
  const priorRole = req.priorRoles.get(name);
  if (!priorRole) return { pinned: false, blocked: "no-prior-role" };
  const agreement = bindingRolesAgree(
    priorRole,
    req.freshRole(),
    req.fnMatches
  );
  if (!agreement.agrees) {
    return { pinned: false, blocked: `role-mismatch:${agreement.reason}` };
  }
  const attempt = attemptValidatedRename(req.scope, req.oldName, name);
  if (!attempt.applied) {
    return { pinned: false, blocked: `validation:${attempt.reason}` };
  }
  return { pinned: true, name, roleReason: agreement.reason };
}
