/**
 * The arrow↔function-expression spelling-flip rule (exp094) — the ONE TS
 * owner of "is flipping this function's spelling semantics-preserving?".
 *
 * The same code twice-spelled (`(a, b) => { … }` vs `function (a, b) { … }`)
 * is NOT universally the same function: an arrow has no own `this`,
 * `arguments` or `new.target` (and no `prototype`), while a function
 * expression does. The flip is safe only under conditions, and this module
 * is where the TS instruments answer that question, so three consumers
 * cannot drift into three different answers:
 *
 * - the eval's soft-noise detector (037 diff-composition's
 *   `wrapperSpellingKey` — which statements charged to `real` are actually
 *   wrapper re-serializations);
 * - the match ground-truth canonicalizer (match-truth/canonical.ts —
 *   whether erasing the `function` head can manufacture a must-match);
 * - reference documentation for the PIPELINE's implementation of the same
 *   rule (Rust `hash::serialize`'s `arrow_serializes_as_function`, the
 *   MatchKey families' wrapper-spelling unification), which works over
 *   oxc's ESTree JSON rather than a babel AST — the node names differ
 *   (oxc merges object methods into `Property` and names class fields
 *   `PropertyDefinition`), the semantics are the same by design, and any
 *   change to one is a change to all three (docs/responsibility.md row).
 *
 * THE RULE. The flip `(a, b) => { … }` ↔ `function (a, b) { … }` is
 * semantics-preserving for a function iff:
 *
 * - it has no binding `id` and is no generator, and an arrow has a block
 *   body (a concise body is not re-spellable without restructuring);
 * - `async` is NOT a condition (both spellings carry it; it does not
 *   interact with `this`/`arguments`);
 * - the function's own lexical scope — its parameters (default values
 *   evaluate under its binding) and its body outside any nested lexical
 *   binder — references no `this`, no `arguments` (x.arguments member
 *   properties and non-computed object keys are names, not references),
 *   and no `new.target` (import.meta evaluates identically in both
 *   spellings); nested ARROW functions pass `this`/`arguments` through and
 *   so do not shield an occurrence.
 *
 * Lexical binders — everything that gives an occurrence its OWN
 * this/arguments: classic functions (incl. object methods and getters/
 * setters), class shells (methods, static blocks, and FIELD INITIALIZERS,
 * which run under the instance — the one deliberate widening over the
 * exp037 detector's original list), while a class's `extends` clause and
 * COMPUTED keys evaluate in the enclosing scope and do not shield.
 *
 * The hash never sees how a function value is USED, so one difference is
 * delegated to census rather than rule: `.prototype`/`new`-on-the-value
 * observability (a function expression has a `prototype` property and is
 * constructible; an arrow has neither). Measured over the flipped walk
 * population (experiments/094-wrapper-spelling): every flip is a call
 * argument to a receiver that only calls it, and no arrow in the walked
 * corpus is ever a `new` callee.
 */

import * as t from "@babel/types";

/** Everything that binds its own `this`/`arguments` (or runs under the
 * instance's, for field initializers and static blocks). Arrows are
 * deliberately absent — they pass both through. */
const LEXICAL_BINDERS = new Set([
  "FunctionDeclaration",
  "FunctionExpression",
  "ObjectMethod",
  "ClassMethod",
  "ClassPrivateMethod",
  "ClassDeclaration",
  "ClassExpression",
  "StaticBlock",
  "ClassProperty"
]);

/** Fields of `parentType` whose children evaluate in the OUTER scope: a
 * class's extends clause, and computed keys (method and field names are
 * computed at definition time, in the enclosing scope). */
function isOuterScopeField(
  parentType: string,
  key: string,
  computed: boolean
): boolean {
  switch (parentType) {
    case "ClassDeclaration":
    case "ClassExpression":
      return key === "superClass";
    case "ClassMethod":
    case "ClassPrivateMethod":
    case "ClassProperty":
    case "ObjectMethod":
    case "ObjectProperty":
      return computed && key === "key";
    default:
      return false;
  }
}

/** Fields that hold an `arguments` NON-reference: a non-computed member
 * property (`x.arguments`), a non-computed, non-shorthand object key
 * (shorthand's VALUE carries the reference), and label names. */
function isArgumentsNonReference(
  parentType: string,
  key: string,
  computed: boolean
): boolean {
  switch (parentType) {
    case "MemberExpression":
    case "OptionalMemberExpression":
      return key === "property" && !computed;
    case "ObjectProperty":
    case "ObjectMethod":
    case "ClassMethod":
    case "ClassProperty":
      return key === "key" && !computed;
    case "LabeledStatement":
    case "BreakStatement":
    case "ContinueStatement":
      return key === "label";
    default:
      return false;
  }
}

/** Does the node itself observe the flipped function's binding? */
function nodeUsesOuterBindings(node: t.Node): boolean {
  if (t.isThisExpression(node)) return true;
  if (t.isMetaProperty(node) && t.isIdentifier(node.meta, { name: "new" })) {
    return true;
  }
  return t.isIdentifier(node) && node.name === "arguments";
}

/** One pushed frame of the scope walk. */
interface Frame {
  node: t.Node;
  parentType: string;
  key: string;
  computed: boolean;
  barrier: boolean;
}

function pushFrame(
  stack: Frame[],
  node: t.Node,
  frame: Omit<Frame, "node">
): void {
  const binder = LEXICAL_BINDERS.has(node.type);
  const outer = isOuterScopeField(frame.parentType, frame.key, frame.computed);
  const barrier = outer ? false : binder ? true : frame.barrier;
  stack.push({ ...frame, node, barrier });
}

function pushChildren(
  stack: Frame[],
  child: unknown,
  frame: Omit<Frame, "node">
): void {
  if (Array.isArray(child)) {
    for (const c of child) {
      pushChildren(stack, c, frame);
    }
    return;
  }
  if (
    typeof child === "object" &&
    child !== null &&
    typeof (child as { type?: unknown }).type === "string"
  ) {
    pushFrame(stack, child as t.Node, frame);
  }
}

/**
 * Any `this`/`arguments`/`new.target` the flip would rebind — an occurrence
 * in the function's own lexical scope. Iterative (explicit stack) like
 * statementHash, for the multi-thousand-line wrapper bodies real bundles
 * contain.
 */
function ownsOuterBindingUse(fn: t.Function): boolean {
  const stack: Frame[] = [];
  // The function's own params (their default values evaluate under its
  // binding) and body start in its own scope; a binding id is a self-name,
  // not an occurrence.
  for (const key of ["params", "body"] as const) {
    pushChildren(stack, (fn as unknown as Record<string, unknown>)[key], {
      parentType: fn.type,
      key,
      computed: false,
      barrier: false
    });
  }
  while (stack.length > 0) {
    const frame = stack.pop() as Frame;
    if (!frame.barrier && nodeUsesOuterBindings(frame.node)) {
      return true;
    }
    const keys = t.VISITOR_KEYS[frame.node.type] ?? [];
    const computed =
      (frame.node as unknown as { computed?: boolean }).computed ?? false;
    for (const k of keys) {
      if (isArgumentsNonReference(frame.node.type, k, computed)) {
        continue;
      }
      pushChildren(
        stack,
        (frame.node as unknown as Record<string, unknown>)[k],
        {
          parentType: frame.node.type,
          key: k,
          computed,
          barrier: frame.barrier
        }
      );
    }
  }
  return false;
}

/** `new.target` anywhere in the subtree (a `new` call on the function VALUE
 * is a use-site question the census answers, not this walk). */
function usesNewTarget(fn: t.Function): boolean {
  const stack: t.Node[] = [fn];
  while (stack.length > 0) {
    const node = stack.pop() as t.Node;
    if (t.isMetaProperty(node) && t.isIdentifier(node.meta, { name: "new" })) {
      return true;
    }
    for (const k of t.VISITOR_KEYS[node.type] ?? []) {
      const child = (node as unknown as Record<string, unknown>)[k];
      const push = (c: unknown): void => {
        if (Array.isArray(c)) {
          for (const cc of c) push(cc);
        } else if (
          typeof c === "object" &&
          c !== null &&
          typeof (c as { type?: unknown }).type === "string"
        ) {
          stack.push(c as t.Node);
        }
      };
      push(child);
    }
  }
  return false;
}

/**
 * Is flipping this function's arrow↔function spelling semantics-preserving?
 * (The exp094 rule — see the module doc. `false` keeps the two spellings
 * apart wherever they are compared.)
 */
export function wrapperFlipIsSemanticsPreserving(fn: t.Function): boolean {
  if (t.isFunctionExpression(fn) && (fn.id || fn.generator)) {
    return false;
  }
  if (t.isArrowFunctionExpression(fn) && !t.isBlockStatement(fn.body)) {
    return false;
  }
  return !ownsOuterBindingUse(fn) && !usesNewTarget(fn);
}
