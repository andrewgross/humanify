/**
 * Carry each function's library classification across beautify (finding #32).
 *
 * Banners are found in the RAW file text; the rename pass works on the
 * BEAUTIFIED text, which is a different coordinate space: beautify expands
 * minified code (an app function before a banner lands past its raw offset)
 * and drops every comment (a library function after a banner slides before
 * it). Comparing the two was the bug.
 *
 * The exact bridge is the beautify transform's own output tree. The
 * beautifier never creates or deletes a function node — it only moves
 * existing nodes into new statements — so every function in the transformed
 * tree is a node the RAW parse produced and still carries its RAW `start`.
 * Classifying there compares raw against raw. The result is recorded per
 * function ORDINAL in one fixed pre-order walk; the rename pass re-parses
 * the printed text, and a printer's defining property is that
 * parse(print(tree)) is the same tree, so the same walk over the re-parsed
 * tree visits the same functions in the same order. The node TYPE of every
 * ordinal is carried too and checked on the far side: a misalignment fails
 * loud instead of freezing the wrong code.
 */
import * as t from "@babel/types";
import { type CommentRegion, libraryAtOffset } from "./comment-regions.js";

/** Per-ordinal library (null = app code) plus the node type as a checksum. */
export interface FunctionLibraryCarry {
  libraries: (string | null)[];
  types: string[];
}

/** Push a node's children in reverse VISITOR_KEYS order (for a LIFO walk). */
function pushChildren(node: t.Node, stack: t.Node[]): void {
  const keys = t.VISITOR_KEYS[node.type];
  if (!keys) return;
  const record = node as unknown as Record<string, unknown>;
  for (let k = keys.length - 1; k >= 0; k--) {
    const value = record[keys[k]];
    if (Array.isArray(value)) {
      for (let i = value.length - 1; i >= 0; i--) {
        const child = value[i] as t.Node | null;
        if (child && typeof child.type === "string") stack.push(child);
      }
    } else if (value && typeof (value as t.Node).type === "string") {
      stack.push(value as t.Node);
    }
  }
}

/**
 * Every function node under `root`, in pre-order with children in
 * VISITOR_KEYS order. The ONE walk both sides of the carry use — the
 * ordinal is only an identity because the walk is shared.
 */
export function functionsInTreeOrder(root: t.Node): t.Function[] {
  const out: t.Function[] = [];
  const stack: t.Node[] = [root];
  for (let node = stack.pop(); node; node = stack.pop()) {
    if (t.isFunction(node)) out.push(node);
    pushChildren(node, stack);
  }
  return out;
}

/**
 * Classify every function of the beautify transform's OUTPUT tree by its RAW
 * start. Throws on a function without a raw start: the beautifier creating
 * a function would break the premise this carry rests on.
 */
export function carryFunctionLibraries(
  transformed: t.Node,
  regions: CommentRegion[]
): FunctionLibraryCarry {
  const libraries: (string | null)[] = [];
  const types: string[] = [];
  for (const fn of functionsInTreeOrder(transformed)) {
    if (fn.start == null) {
      throw new Error(
        `library carry: a ${fn.type} in the beautified tree has no raw start — beautify synthesized a function, so raw offsets no longer identify it`
      );
    }
    libraries.push(libraryAtOffset(regions, fn.start));
    types.push(fn.type);
  }
  return { libraries, types };
}

/**
 * Resolve a carry against the re-parsed beautified tree: the library name of
 * every library function node. Throws if the walk does not line up with the
 * carry (count or per-ordinal type).
 */
export function resolveFunctionLibraries(
  reparsed: t.Node,
  carry: FunctionLibraryCarry
): Map<t.Function, string> {
  const fns = functionsInTreeOrder(reparsed);
  if (fns.length !== carry.libraries.length) {
    throw new Error(
      `library carry: ${carry.libraries.length} functions carried across beautify, ${fns.length} found in the re-parsed text`
    );
  }
  const out = new Map<t.Function, string>();
  for (let i = 0; i < fns.length; i++) {
    if (fns[i].type !== carry.types[i]) {
      throw new Error(
        `library carry: function #${i} is a ${carry.types[i]} before re-parse and a ${fns[i].type} after`
      );
    }
    const library = carry.libraries[i];
    if (library !== null) out.set(fns[i], library);
  }
  return out;
}
