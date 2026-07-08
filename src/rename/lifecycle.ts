import type { Binding } from "@babel/traverse";

/**
 * Explicit lifecycle state for graph nodes (functions and module bindings).
 *
 * Replaces the four ad-hoc "done" encodings that used to drift out of sync:
 * FunctionNode.status, the renameMapping-presence sentinel, the preDone list,
 * and processUnified's doneIds seed. A node is handled exactly once, so every
 * legal transition is pending → settled; anything else is a bug and throws.
 *
 * States:
 * - pending      — not yet handled; still eligible for dispatch.
 * - transferred  — a prior-version exact match; `names` are applied by the
 *                  transfer pass (the one settled state whose data is read back).
 * - llm-done     — processed by the LLM batch pass; `names` is the applied map
 *                  (write-mostly — only tests read it; output is renameReport).
 * - skipped      — frozen without the LLM: library / wrapper IIFE / eval-with
 *                  taint / nothing-to-rename, or a prior-version-matched binding.
 * - failed       — an internal pipeline error while processing the node.
 */
export type LifecycleState =
  | { readonly kind: "pending" }
  | { readonly kind: "transferred"; readonly transfers: TransferPair[] }
  | { readonly kind: "llm-done"; readonly names: Record<string, string> }
  | { readonly kind: "skipped"; readonly reason: string }
  | { readonly kind: "failed"; readonly error: string };

/**
 * One prior-version name transfer. `binding` is the exact Binding the pair
 * targets (resolved through placeholder slots at match time) — renames must
 * apply to it, not to whatever a name lookup finds: two distinct bindings
 * can share a minified name (a catch param shadowing a function-scope
 * binding), and name-string resolution picks the wrong one. A null binding
 * means the pair was aligned positionally (close-match name/param
 * transfers) and the applier falls back to owned-binding-map resolution.
 */
export interface TransferPair {
  oldName: string;
  newName: string;
  binding: Binding | null;
}

/** A graph node that carries lifecycle state. */
export interface Stateful {
  sessionId: string;
  state: LifecycleState;
}

/** The shared initial state. Nodes are born pending at graph build. */
export const PENDING: LifecycleState = { kind: "pending" };

/** True while the node may still be dispatched. */
export function isPending(node: Stateful): boolean {
  return node.state.kind === "pending";
}

/** True once the node has reached any terminal state. */
export function isSettled(node: Stateful): boolean {
  return node.state.kind !== "pending";
}

/**
 * Apply a transition, asserting it is legal. A node is settled exactly once,
 * so the only legal source is `pending`; a second write is a double-handling
 * bug we want to surface loudly rather than silently overwrite.
 */
export function transition(node: Stateful, to: LifecycleState): void {
  if (node.state.kind !== "pending") {
    throw new Error(
      `illegal lifecycle transition for ${node.sessionId}: ` +
        `${node.state.kind} -> ${to.kind} (only pending -> settled is allowed)`
    );
  }
  node.state = to;
}

/** Prior-version exact match: record the pairs the transfer pass will apply. */
export function markTransferred(
  node: Stateful,
  transfers: TransferPair[]
): void {
  transition(node, { kind: "transferred", transfers });
}

/** Processed by the LLM batch pass. */
export function markLlmDone(
  node: Stateful,
  names: Record<string, string> = {}
): void {
  transition(node, { kind: "llm-done", names });
}

/** Frozen without the LLM (library / wrapper / taint / nothing-to-rename). */
export function markSkipped(node: Stateful, reason: string): void {
  transition(node, { kind: "skipped", reason });
}

/** An internal error occurred while processing the node. */
export function markFailed(node: Stateful, error: string): void {
  transition(node, { kind: "failed", error });
}
