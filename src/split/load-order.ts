/**
 * Load-time dependency model for a module's top-level statements (exp038).
 *
 * Emission order within a file is a diff-noise lever: when upstream reshuffles
 * the bundle, files full of byte-identical code churn. Lever B (exp037) may only
 * reposition FUNCTION DECLARATIONS, because reordering top-level statements is
 * not generally safe — an earlier attempt moved `defineModuleExports(m, {...})`
 * above `var m = {}` and crashed the tree on boot. The blanket rule leaves the
 * majority of the residual churn pinned.
 *
 * This module replaces the blanket rule with what actually constrains order:
 * what each statement READS, WRITES and whether it has an observable EFFECT
 * *while the module loads*. Function/arrow BODIES are excluded — they run later,
 * so what they reference imposes no load-time edge. That is the whole unlock:
 * `var x = lazyInitializer(() => {...})` touches nothing at load time.
 *
 * Soundness rests on three deliberately conservative rules:
 *
 *  1. **Effect statements are barriers.** Anything that can observably do
 *     something at load time (an unverified call, `new`, `await`, a write
 *     through a member, control flow) pins every non-hoisted statement on its
 *     side of it. A call can reach module state through a function body we do
 *     not model, so nothing is allowed to cross one.
 *  2. **Hoisted function declarations are unconstrained.** They are initialized
 *     before any statement runs, so their textual position has no runtime
 *     meaning — the property Lever B already ships on.
 *  3. **Every edge points forward in bundle order.** The constraint graph is a
 *     DAG whose topological orders include the bundle order itself, so a legal
 *     answer always exists and the pass can never fail.
 *
 * Purity of a call is NOT decided here and never by name-matching: the caller
 * verifies a helper's shape structurally (see `identifyBunLazyInit`) and passes
 * the resulting name in via `pureCallNames`.
 */
import * as t from "@babel/types";
import { identifyBunLazyInit } from "../shared/bun-helpers.js";

/** What one top-level statement does while the module is loading. */
export interface LoadOrderFacts {
  /** Hoisted (function declaration): initialized before any statement runs, so
   * its textual position has no runtime meaning and nothing constrains it. */
  hoisted: boolean;
  /** Module bindings it assigns at load time. */
  writes: readonly string[];
  /** Module bindings it reads at load time (initializers and bare expressions —
   * never a function/arrow body, which runs later). */
  reads: readonly string[];
  /** It can observably do something at load time, so it is an order barrier. */
  effects: boolean;
}

export interface LoadOrderOptions {
  /** Terminal callee names the caller has verified — structurally, not by
   * name — to capture their arguments and return with no observable load-time
   * effect (Bun's lazy-init wrapper). Matched against `f(…)`, `ns.f(…)` and the
   * emitted `(0, ns.f)(…)` form. */
  pureCallNames?: ReadonlySet<string>;

  /**
   * Terminal callee names the caller has verified — structurally — to do nothing
   * at load time EXCEPT write their first argument. The bundler's export
   * registrar is the case: it installs lazy getters (`get: source[k]`) over a
   * literal of arrow thunks, so nothing it is handed is evaluated, but it does
   * mutate the target object.
   *
   * Deliberately NOT folded into `pureCallNames`. `pure` records the target as a
   * READ, and two reads carry no dependence edge, so a load-time read of
   * `exportsObj.foo` could be scheduled BEFORE the registration. Recording a
   * WRITE gives read-after-write and write-after-write for free — which is also
   * what keeps the call from floating above `var exportsObj = {}`, the exact
   * reordering that crashed the runnable tree in exp037.
   */
  targetWritingCallNames?: ReadonlySet<string>;
}

interface Ctx {
  reads: Set<string>;
  writes: Set<string>;
  effects: boolean;
  pure: ReadonlySet<string>;
  targetWriting: ReadonlySet<string>;
}

/** Terminal callee name of `f(…)`, `ns.f(…)` or `(0, ns.f)(…)`. */
function calleeName(callee: t.Node): string | null {
  if (t.isIdentifier(callee)) return callee.name;
  if (
    (t.isMemberExpression(callee) || t.isOptionalMemberExpression(callee)) &&
    !callee.computed &&
    t.isIdentifier(callee.property)
  ) {
    return callee.property.name;
  }
  if (t.isSequenceExpression(callee) && callee.expressions.length > 0) {
    return calleeName(callee.expressions[callee.expressions.length - 1]);
  }
  return null;
}

function walkChildren(node: t.Node, ctx: Ctx): void {
  for (const key of t.VISITOR_KEYS[node.type] ?? []) {
    const child = (node as unknown as Record<string, unknown>)[key];
    if (Array.isArray(child)) {
      for (const c of child) walk(c as t.Node | null, ctx);
    } else if (child) {
      walk(child as t.Node, ctx);
    }
  }
}

/** A property read records the base binding; the read itself is treated as
 * effect-free (a getter with an observable side effect is the accepted
 * residual risk — the emitted accessors this pipeline generates are `() => x`). */
function walkMember(
  node: t.MemberExpression | t.OptionalMemberExpression,
  ctx: Ctx
): void {
  walk(node.object, ctx);
  if (node.computed) walk(node.property, ctx);
}

function walkCall(
  node: t.CallExpression | t.OptionalCallExpression | t.NewExpression,
  ctx: Ctx
): void {
  const name = t.isNewExpression(node) ? null : calleeName(node.callee);
  if (name !== null && ctx.targetWriting.has(name)) {
    // Writes its first argument and nothing else observable. An argument that is
    // not a plain identifier leaves us unable to name the target, so stay
    // conservative rather than guess.
    const target = node.arguments[0];
    if (t.isIdentifier(target)) ctx.writes.add(target.name);
    else ctx.effects = true;
    walk(node.callee, ctx);
    for (let i = 1; i < node.arguments.length; i++)
      walk(node.arguments[i], ctx);
    return;
  }
  if (name === null || !ctx.pure.has(name)) ctx.effects = true;
  walk(node.callee, ctx);
  for (const arg of node.arguments) walk(arg, ctx);
}

function walkAssignment(node: t.AssignmentExpression, ctx: Ctx): void {
  if (t.isIdentifier(node.left)) {
    ctx.writes.add(node.left.name);
    if (node.operator !== "=") ctx.reads.add(node.left.name);
  } else {
    ctx.effects = true; // writing through a member or pattern
    walk(node.left, ctx);
  }
  walk(node.right, ctx);
}

function walkUpdate(node: t.UpdateExpression, ctx: Ctx): void {
  if (t.isIdentifier(node.argument)) {
    ctx.reads.add(node.argument.name);
    ctx.writes.add(node.argument.name);
    return;
  }
  ctx.effects = true;
  walk(node.argument, ctx);
}

function walkClassMember(el: t.Node, ctx: Ctx): void {
  if (t.isStaticBlock(el)) {
    ctx.effects = true; // runs at class definition
    return;
  }
  if (
    "decorators" in el &&
    Array.isArray(el.decorators) &&
    el.decorators.length
  )
    ctx.effects = true;
  if ("computed" in el && el.computed && "key" in el)
    walk(el.key as t.Node, ctx);
  // Only STATIC property values run at definition; instance fields run at
  // construction, and method bodies run when called.
  if (
    (t.isClassProperty(el) || t.isClassPrivateProperty(el)) &&
    (el as t.ClassProperty).static
  ) {
    walk(el.value, ctx);
  }
}

function walkClass(node: t.Class, ctx: Ctx): void {
  if (node.decorators?.length) ctx.effects = true;
  walk(node.superClass, ctx);
  for (const el of node.body.body) walkClassMember(el, ctx);
}

/** Reference positions: only some children of these nodes are evaluated. */
function walkReference(node: t.Node, ctx: Ctx): boolean {
  if (t.isIdentifier(node)) {
    ctx.reads.add(node.name);
    return true;
  }
  if (t.isMemberExpression(node) || t.isOptionalMemberExpression(node)) {
    walkMember(node, ctx);
    return true;
  }
  if (t.isObjectProperty(node)) {
    if (node.computed) walk(node.key, ctx);
    walk(node.value, ctx);
    return true;
  }
  return false;
}

/** Subtrees whose body runs LATER — the heart of the model. */
function walkDeferred(node: t.Node, ctx: Ctx): boolean {
  if (t.isObjectMethod(node) || t.isClassMethod(node)) {
    if (node.computed) walk(node.key, ctx); // the key is evaluated now
    return true;
  }
  if (t.isFunction(node)) return true; // closure creation only
  if (t.isClass(node)) {
    walkClass(node, ctx);
    return true;
  }
  return false;
}

/** Operators that read, write or invoke. */
function walkOperator(node: t.Node, ctx: Ctx): boolean {
  if (
    t.isCallExpression(node) ||
    t.isOptionalCallExpression(node) ||
    t.isNewExpression(node)
  ) {
    walkCall(node, ctx);
    return true;
  }
  if (t.isAssignmentExpression(node)) {
    walkAssignment(node, ctx);
    return true;
  }
  if (t.isUpdateExpression(node)) {
    walkUpdate(node, ctx);
    return true;
  }
  return false;
}

/** Accumulate the load-time reads/writes/effects of an expression or statement
 * subtree. Deferred subtrees (function and arrow bodies) are not entered. */
function walk(node: t.Node | null | undefined, ctx: Ctx): void {
  if (!node) return;
  if (
    walkReference(node, ctx) ||
    walkDeferred(node, ctx) ||
    walkOperator(node, ctx)
  ) {
    return;
  }
  if (isEffectNode(node)) ctx.effects = true;
  walkChildren(node, ctx);
}

/** Node kinds whose mere evaluation is observable. */
function isEffectNode(node: t.Node): boolean {
  return (
    t.isTaggedTemplateExpression(node) ||
    t.isAwaitExpression(node) ||
    t.isYieldExpression(node) ||
    t.isSpreadElement(node) || // runs the iterator / copies getters
    (t.isUnaryExpression(node) && node.operator === "delete")
  );
}

function analyzeVariableDeclaration(
  decl: t.VariableDeclaration,
  ctx: Ctx
): void {
  for (const d of decl.declarations) {
    if (!t.isIdentifier(d.id)) {
      // Destructuring runs the iterator/getter protocol at load time.
      ctx.effects = true;
    }
    // A bare `var x;` has no runtime effect at all — the binding is hoisted, so
    // the declaration is free to sit anywhere. `let`/`const` have a TDZ, which
    // makes the declaration's position observable, so it counts as a write.
    if (d.init || decl.kind !== "var") {
      for (const name of Object.keys(t.getBindingIdentifiers(d.id))) {
        ctx.writes.add(name);
      }
    }
    walk(d.init, ctx);
  }
}

function analyzeStatement(
  stmt: t.Statement,
  pure: ReadonlySet<string>,
  targetWriting: ReadonlySet<string>
): LoadOrderFacts {
  if (t.isFunctionDeclaration(stmt)) {
    return { hoisted: true, writes: [], reads: [], effects: false };
  }
  const ctx: Ctx = {
    reads: new Set(),
    writes: new Set(),
    effects: false,
    pure,
    targetWriting
  };
  if (t.isVariableDeclaration(stmt)) analyzeVariableDeclaration(stmt, ctx);
  else if (t.isClassDeclaration(stmt)) {
    if (stmt.id) ctx.writes.add(stmt.id.name);
    walkClass(stmt, ctx);
  } else if (t.isExpressionStatement(stmt)) walk(stmt.expression, ctx);
  else {
    // Control flow, `throw`, labelled blocks, module syntax: not modelled in
    // detail — pinned as a barrier.
    ctx.effects = true;
    walk(stmt, ctx);
  }
  return {
    hoisted: false,
    writes: [...ctx.writes],
    reads: [...ctx.reads],
    effects: ctx.effects
  };
}

/** Load-time facts for each top-level statement, parallel to `stmts`. */
export function analyzeLoadOrder(
  stmts: readonly t.Statement[],
  options: LoadOrderOptions = {}
): LoadOrderFacts[] {
  const pure = options.pureCallNames ?? new Set<string>();
  const targetWriting = options.targetWritingCallNames ?? new Set<string>();
  return stmts.map((s) => analyzeStatement(s, pure, targetWriting));
}

/** The two identifier params of a `(a, b) => …` arrow, or null. */
function twoIdentParams(
  init: t.Node | null | undefined
): { target: string; source: string } | null {
  if (!t.isArrowFunctionExpression(init) || init.params.length !== 2)
    return null;
  const [a, b] = init.params;
  if (!t.isIdentifier(a) || !t.isIdentifier(b)) return null;
  return { target: a.name, source: b.name };
}

/** The `for (var k in source)` loop in an arrow body, or null. */
function forInOverSource(
  init: t.ArrowFunctionExpression,
  source: string
): t.ForInStatement | null {
  const body = t.isBlockStatement(init.body) ? init.body.body : [init.body];
  for (const b of body) {
    if (t.isForInStatement(b) && t.isIdentifier(b.right, { name: source })) {
      return b;
    }
  }
  return null;
}

/** The loop variable name of a for-in head. */
function forInKey(forIn: t.ForInStatement): string | null {
  if (t.isVariableDeclaration(forIn.left)) {
    const id = forIn.left.declarations[0]?.id;
    return t.isIdentifier(id) ? id.name : null;
  }
  return t.isIdentifier(forIn.left) ? forIn.left.name : null;
}

/** The single call expression a for-in body consists of. */
function soleCall(forIn: t.ForInStatement): t.CallExpression | null {
  const inner = t.isExpressionStatement(forIn.body)
    ? forIn.body.expression
    : t.isBlockStatement(forIn.body) &&
        t.isExpressionStatement(forIn.body.body[0])
      ? forIn.body.body[0].expression
      : null;
  return t.isCallExpression(inner) ? inner : null;
}

/**
 * `get: source[key]` — the lazy-getter certificate, and the load-bearing part of
 * this whole detection: it proves the values are INSTALLED rather than evaluated,
 * so calling the helper touches nothing but the target. A helper that eagerly
 * read `source[key]` would not match.
 */
function installsLazyGetter(
  descriptor: t.Node,
  source: string,
  key: string
): boolean {
  if (!t.isObjectExpression(descriptor)) return false;
  return descriptor.properties.some(
    (prop) =>
      t.isObjectProperty(prop) &&
      t.isIdentifier(prop.key, { name: "get" }) &&
      t.isMemberExpression(prop.value) &&
      t.isIdentifier(prop.value.object, { name: source }) &&
      t.isIdentifier(prop.value.property, { name: key })
  );
}

/** Does this declarator define the export registrar? Returns its name. */
function registrarName(d: t.VariableDeclarator): string | null {
  if (!t.isIdentifier(d.id)) return null;
  const params = twoIdentParams(d.init);
  if (!params || !t.isArrowFunctionExpression(d.init)) return null;
  const forIn = forInOverSource(d.init, params.source);
  if (!forIn) return null;
  const key = forInKey(forIn);
  if (!key) return null;
  const call = soleCall(forIn);
  if (!call || call.arguments.length !== 3) return null;
  if (!t.isIdentifier(call.arguments[0], { name: params.target })) return null;
  if (!t.isIdentifier(call.arguments[1], { name: key })) return null;
  return installsLazyGetter(call.arguments[2], params.source, key)
    ? d.id.name
    : null;
}

/**
 * The bundler's export registrar, identified STRUCTURALLY — never by name, since
 * every name in this pipeline is LLM-chosen and differs run to run.
 *
 * The shape, as the bundle emits it:
 *
 *     var NAME = (target, source) => {
 *       for (var k in source) defineProperty(target, k, {
 *         get: source[k], enumerable: true, configurable: true, set: ...
 *       });
 *     };
 */
function identifyExportRegistrar(stmts: readonly t.Statement[]): string | null {
  for (const stmt of stmts) {
    if (!t.isVariableDeclaration(stmt)) continue;
    for (const d of stmt.declarations) {
      const name = registrarName(d);
      if (name) return name;
    }
  }
  return null;
}

/**
 * Load-time facts for a bundle's top-level statements, admitting the bundler's
 * lazy-init wrapper as pure.
 *
 * The wrapper is identified STRUCTURALLY — by the `x && (y = x(x = 0))` shape
 * `identifyBunLazyInit` scans for, the same detection module splitting already
 * uses — never by matching an identifier. Calling it captures the generator in a
 * closure and returns; nothing observable happens until the returned thunk is
 * invoked. Since the bundle is one scope, the helper's name identifies exactly
 * one binding, so admitting calls by that name admits calls to that binding.
 * These registrations are the largest pinned block of reorder churn.
 */
export function bundleLoadOrderFacts(
  stmts: readonly t.Statement[],
  code: string
): LoadOrderFacts[] {
  const lazyInit = identifyBunLazyInit(code);
  // `HUMANIFY_NO_REGISTRAR_EXEMPTION=1` restores the pre-049 behaviour, where
  // every export registration is an opaque barrier. It exists because admitting
  // them unpins 580 of the bundle's 588 barriers, and a change with that blast
  // radius has to be A/B-able against a byte-identical control without a rebuild
  // (exp044's alias reservation had a clean scoping argument and still cost
  // +3,742 lines through second-order effects).
  const registrar = process.env.HUMANIFY_NO_REGISTRAR_EXEMPTION
    ? null
    : identifyExportRegistrar(stmts);
  return analyzeLoadOrder(stmts, {
    pureCallNames: lazyInit ? new Set([lazyInit]) : undefined,
    targetWritingCallNames: registrar ? new Set([registrar]) : undefined
  });
}

/**
 * Predecessors of each slot: statements that must be emitted before it.
 *
 * One pass in bundle order, so every edge points forward and the graph is a DAG
 * with the bundle order as a feasible solution. Edges are the classic
 * dependence set — read-after-write, write-after-read, write-after-write — plus
 * the effect barrier.
 */
type AddEdge = (from: number, to: number) => void;

/** Running dependence state across one pass in bundle order. */
interface DepState {
  lastWriter: Map<string, number>;
  readers: Map<string, number[]>;
  lastBarrier: number | null;
  sinceBarrier: number[];
}

/** Nothing crosses an effect-bearing statement: link it to every non-hoisted
 * statement in the segment before it, and to everything after via `lastBarrier`. */
function addBarrierEdges(
  s: number,
  f: LoadOrderFacts,
  st: DepState,
  add: AddEdge
): void {
  if (st.lastBarrier !== null) add(st.lastBarrier, s);
  if (!f.effects) {
    st.sinceBarrier.push(s);
    return;
  }
  for (const p of st.sinceBarrier) add(p, s);
  st.sinceBarrier = [];
  st.lastBarrier = s;
}

/** Read-after-write, write-after-read and write-after-write on module bindings. */
function addDataEdges(
  s: number,
  f: LoadOrderFacts,
  st: DepState,
  add: AddEdge
): void {
  for (const n of f.reads) {
    const w = st.lastWriter.get(n);
    if (w !== undefined) add(w, s);
    const list = st.readers.get(n) ?? [];
    list.push(s);
    st.readers.set(n, list);
  }
  for (const n of f.writes) {
    const w = st.lastWriter.get(n);
    if (w !== undefined) add(w, s);
    for (const r of st.readers.get(n) ?? []) add(r, s);
    st.readers.set(n, []);
    st.lastWriter.set(n, s);
  }
}

function buildDependencies(
  slots: readonly number[],
  facts: readonly LoadOrderFacts[]
): Map<number, number[]> {
  const preds = new Map<number, number[]>(slots.map((s) => [s, []]));
  const add: AddEdge = (from, to) => {
    if (from !== to) (preds.get(to) as number[]).push(from);
  };
  const st: DepState = {
    lastWriter: new Map(),
    readers: new Map(),
    lastBarrier: null,
    sinceBarrier: []
  };
  for (const s of slots) {
    const f = facts[s];
    if (f.hoisted) continue; // unconstrained
    addBarrierEdges(s, f, st, add);
    addDataEdges(s, f, st, add);
  }
  return preds;
}

/** Binary min-heap over slots, keyed by desired rank then bundle index. */
class RankHeap {
  private readonly items: number[] = [];
  constructor(private readonly rank: Map<number, number>) {}
  private less(a: number, b: number): boolean {
    const ra = this.rank.get(a) as number;
    const rb = this.rank.get(b) as number;
    return ra !== rb ? ra < rb : a < b;
  }
  push(v: number): void {
    this.items.push(v);
    let i = this.items.length - 1;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (!this.less(this.items[i], this.items[p])) break;
      [this.items[i], this.items[p]] = [this.items[p], this.items[i]];
      i = p;
    }
  }
  pop(): number | undefined {
    if (this.items.length === 0) return undefined;
    const top = this.items[0];
    const last = this.items.pop() as number;
    if (this.items.length === 0) return top;
    this.items[0] = last;
    let i = 0;
    for (;;) {
      const l = 2 * i + 1;
      const r = l + 1;
      let m = i;
      if (l < this.items.length && this.less(this.items[l], this.items[m]))
        m = l;
      if (r < this.items.length && this.less(this.items[r], this.items[m]))
        m = r;
      if (m === i) break;
      [this.items[i], this.items[m]] = [this.items[m], this.items[i]];
      i = m;
    }
    return top;
  }
  get size(): number {
    return this.items.length;
  }
}

/** Position of each slot in the desired order; anything the desired order omits
 * sorts after everything it names. */
function rankOf(
  slots: readonly number[],
  desired: readonly number[]
): Map<number, number> {
  const rank = new Map<number, number>();
  for (let i = 0; i < desired.length; i++) rank.set(desired[i], i);
  for (const s of slots) if (!rank.has(s)) rank.set(s, rank.size);
  return rank;
}

/** Indegree and successor lists for the greedy topological pass. */
function schedule(
  slots: readonly number[],
  preds: Map<number, number[]>
): { indegree: Map<number, number>; successors: Map<number, number[]> } {
  const indegree = new Map<number, number>(slots.map((s) => [s, 0]));
  const successors = new Map<number, number[]>(slots.map((s) => [s, []]));
  for (const [to, list] of preds) {
    for (const from of list) {
      (successors.get(from) as number[]).push(to);
      indegree.set(to, (indegree.get(to) as number) + 1);
    }
  }
  return { indegree, successors };
}

/**
 * Order `slots` as close to `desired` as the load-time dependencies allow.
 *
 * Greedy topological scheduling: among the statements whose predecessors are all
 * emitted, take the one the desired order wants soonest. Deterministic, always a
 * permutation of `slots`, and never violates an edge — so the emitted module
 * behaves exactly as the bundle order would.
 *
 * `facts` is indexed by statement index (parallel to the file's hash array);
 * `desired` is a permutation of `slots`.
 */
export function orderRespectingLoadOrder(
  slots: readonly number[],
  desired: readonly number[],
  facts: readonly LoadOrderFacts[]
): number[] {
  if (slots.length < 2) return [...slots];
  const rank = rankOf(slots, desired);
  const { indegree, successors } = schedule(
    slots,
    buildDependencies(slots, facts)
  );
  const heap = new RankHeap(rank);
  for (const s of slots) if (indegree.get(s) === 0) heap.push(s);
  const out: number[] = [];
  while (heap.size > 0) {
    const s = heap.pop() as number;
    out.push(s);
    for (const nxt of successors.get(s) as number[]) {
      const left = (indegree.get(nxt) as number) - 1;
      indegree.set(nxt, left);
      if (left === 0) heap.push(nxt);
    }
  }
  // Unreachable for a forward-only edge set, but never emit a partial file.
  return out.length === slots.length ? out : [...slots];
}
