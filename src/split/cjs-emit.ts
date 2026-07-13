/**
 * Runnable CommonJS emission for a stable-split tree (exp026/exp027).
 *
 * The default split (`emitFiles` in stable-split.ts) writes byte-exact
 * statement slices — the review artifact. That tree parses but is not a
 * standalone module graph: cross-file references are bare identifiers
 * bound in the original single scope. This module emits the RUNNABLE
 * form. What it guarantees:
 *
 *   - LIVE bindings: a file that reads/writes another file's binding does
 *     `const __decl = require("./decl.js")` once and every cross-file
 *     reference `x` is rewritten to `__decl.x` — reads, assignment and
 *     destructuring targets, update expressions, for-in/of heads. The
 *     declaring file exports each such binding as a live accessor
 *     (`get: () => x`, plus a setter when written cross-file).
 *   - Exact rewrites: byte-offset splices of the ORIGINAL parse's
 *     reference nodes — never a re-parse, never name matching — so
 *     shadowing locals are untouchable by construction and untouched
 *     statements stay byte-exact.
 *   - Semantics-preserving special forms: a bare cross-file callee becomes
 *     `(0, __decl.x)(...)` (callee `this` stays undefined/globalThis);
 *     sloppy-mode `delete x` on a binding becomes `false` (its value); a
 *     cross-file `var x = e` REdeclaration becomes the setter assignment
 *     `__decl.x = e`; wrapper prologue directives ("use strict") propagate
 *     to every emitted file, and an inert mid-body string statement is
 *     parenthesized rather than promoted to a directive.
 *   - ONE module context: references to the wrapper's parameters
 *     (exports, require, module, __filename, __dirname) and to top-level
 *     `this` route through an emitted `_bundle.js`, which `index.js`
 *     initializes with the ENTRY module's context — `module.exports = api`
 *     in any split file updates the real public surface, and relative
 *     requires resolve against the entry.
 *   - An `index.js` entry requires every file in the original bundle's
 *     first-statement order, so files nothing imports still execute.
 *   - Load-order safety is ENFORCED, not assumed: cross-file references
 *     that execute at load time (top level / top-level IIFE bodies) must
 *     form an acyclic file graph, or emission throws — mid-cycle a
 *     top-level read would silently observe partial exports.
 *
 * The emitter THROWS (with a reason) instead of silently degrading when
 * the input violates what it can faithfully represent: non-wrapper input,
 * a ledger that disagrees with the statement count, cross-file function
 * redeclaration (hoisting is unpreservable), redeclaration through a
 * destructuring declarator, a redeclared wrapper parameter, or a
 * load-time reference cycle. `tryEmitRunnableCjs` converts the throw into
 * a reported decline so callers fall back to the byte-exact review tree
 * loudly, never losing the stable split or its ledger.
 *
 * KNOWN LIMITS (inherent to per-file CJS execution, not checked here):
 * the original wrapper body interleaves statements across files, and
 * modules execute atomically — top-level side-effect ORDER across files
 * follows first-statement order, not the original interleaving; direct
 * `eval` that names module bindings cannot be detected; the wrapper's
 * `arguments` object is not routed. Emission is deterministic.
 * Naming-only law is not engaged — this is mechanical module extraction,
 * never LLM rewriting.
 */

import type { Binding, NodePath, Scope } from "@babel/traverse";
import * as t from "@babel/types";
import { findWrapperFunction } from "../analysis/wrapper-detection.js";
import { parseFileAst, violationWriteTargetPaths } from "../babel-utils.js";
import { computeRelativeImportPath } from "./emitter.js";
import type { StableSplitLedger } from "./stable-split.js";

interface CrossBinding {
  name: string;
  declFile: string;
  /** Written from a file other than declFile — needs a setter. */
  writable: boolean;
}

/** One byte-range replacement inside the original bundle text. */
interface Edit {
  start: number;
  end: number;
  text: string;
}

interface EmitPlan {
  statements: t.Statement[];
  ranges: Array<{ start: number; end: number }>;
  /** statement index → owning file (= ledger.order). */
  stmtFile: string[];
  /** name → cross-file binding record (only bindings that cross a file). */
  cross: Map<string, CrossBinding>;
  /** statement index → edits keyed by start offset (dedupes read+write
   * double-hits like `x++`). */
  editsByStmt: Map<number, Map<number, Edit>>;
  /** statement index → cross-file var-redeclaration declarator ids
   * (id start offset → binding name), resolved by composite surgery. */
  redeclByStmt: Map<number, Map<number, string>>;
  /** declFile → names to export via accessor. */
  exportsByFile: Map<string, Set<string>>;
  /** readerFile → declFiles it must require. */
  requiresByFile: Map<string, Set<string>>;
  /** file → collision-free namespace variable. */
  nsVars: Map<string, string>;
  /** Wrapper prologue directives (raw text, e.g. '"use strict";') —
   * propagated to every emitted file so strictness is preserved. */
  directives: string[];
  /** The wrapper function node (load-time classification boundary). */
  wrapperNode: t.Node;
  /** readerFile → declFiles it reads/writes WHILE LOADING (top-level).
   * Must be acyclic: mid-cycle a top-level read observes partial exports. */
  loadTimeEdges: Map<string, Set<string>>;
  /** Shared original-wrapper module context (exports/require/module/
   * __filename/__dirname/top-level this), routed through an emitted
   * runtime module. Null when the wrapper context is never referenced. */
  bundleContext: {
    varName: string;
    files: Set<string>;
    fileName: string;
  } | null;
}

function offsetOf(n: number | null | undefined): number {
  if (n == null) throw new Error("runnable emit: node missing byte offsets");
  return n;
}

function nsVarOf(plan: EmitPlan, file: string): string {
  const v = plan.nsVars.get(file);
  if (!v) throw new Error(`runnable emit: no namespace var for ${file}`);
  return v;
}

function crossOf(plan: EmitPlan, name: string): CrossBinding {
  const c = plan.cross.get(name);
  if (!c) throw new Error(`runnable emit: no cross record for ${name}`);
  return c;
}

/** Binary-search the top-level statement index containing a byte position. */
function stmtIndexOf(
  ranges: Array<{ start: number; end: number }>,
  pos: number
): number {
  let lo = 0;
  let hi = ranges.length - 1;
  while (lo <= hi) {
    const m = (lo + hi) >> 1;
    const r = ranges[m];
    if (pos < r.start) hi = m - 1;
    else if (pos >= r.end) lo = m + 1;
    else return m;
  }
  return -1;
}

function validateLedger(ledger: StableSplitLedger, stmtCount: number): void {
  if (ledger.order.length !== stmtCount) {
    throw new Error(
      `runnable emit: ledger.order has ${ledger.order.length} entries but ` +
        `the wrapper has ${stmtCount} statements`
    );
  }
  const known = new Set(ledger.files);
  for (const f of ledger.order) {
    if (!known.has(f)) {
      throw new Error(
        `runnable emit: ledger.order references "${f}" missing from ledger.files`
      );
    }
  }
}

/** Collision-free namespace variable per file: sanitized path, uniquified
 * against other files' vars and the bundle's own top-level bindings. */
function buildNsVars(files: string[], scope: Scope): Map<string, string> {
  const used = new Set<string>();
  const vars = new Map<string, string>();
  for (const file of files) {
    const base = `__${file.replace(/[^A-Za-z0-9_$]/g, "_")}`;
    let v = base;
    for (let n = 2; used.has(v) || scope.hasBinding(v); n++) {
      v = `${base}_${n}`;
    }
    used.add(v);
    vars.set(file, v);
  }
  return vars;
}

function addTo<K, V>(map: Map<K, Set<V>>, key: K, value: V): void {
  const set = map.get(key);
  if (set) set.add(value);
  else map.set(key, new Set([value]));
}

function addEdit(plan: EmitPlan, stmtIdx: number, edit: Edit): void {
  const edits = plan.editsByStmt.get(stmtIdx);
  if (!edits) {
    plan.editsByStmt.set(stmtIdx, new Map([[edit.start, edit]]));
  } else if (!edits.has(edit.start)) {
    edits.set(edit.start, edit);
  }
}

/** True when the site executes while its module LOADS (top-level, or in a
 * top-level IIFE body) as opposed to inside a function called later. */
function isLoadTimeSite(site: NodePath, wrapperNode: t.Node): boolean {
  let p: NodePath | null = site.parentPath;
  for (; p; p = p.parentPath) {
    if (p.node === wrapperNode) return true;
    if ((p.isClassProperty() || p.isClassPrivateProperty()) && !p.node.static) {
      return false;
    }
    if (p.isFunction() && !isIifeCallee(p)) return false;
  }
  return false;
}

function isIifeCallee(f: NodePath): boolean {
  return f.parentPath?.isCallExpression({ callee: f.node }) === true;
}

function recordCrossSite(
  plan: EmitPlan,
  stmtIdx: number,
  name: string,
  declFile: string,
  site: NodePath
): void {
  addTo(plan.exportsByFile, declFile, name);
  const reader = plan.stmtFile[stmtIdx];
  addTo(plan.requiresByFile, reader, declFile);
  if (isLoadTimeSite(site, plan.wrapperNode)) {
    addTo(plan.loadTimeEdges, reader, declFile);
  }
}

function isBareCalleePos(parent: t.Node, node: t.Node): boolean {
  return (
    ((t.isCallExpression(parent) || t.isOptionalCallExpression(parent)) &&
      parent.callee === node) ||
    (t.isTaggedTemplateExpression(parent) && parent.tag === node)
  );
}

function isDeleteArg(ref: NodePath<t.Identifier>): boolean {
  return (
    t.isUnaryExpression(ref.parent) &&
    ref.parent.operator === "delete" &&
    ref.parent.argument === ref.node
  );
}

/** `delete x` on a binding is a sloppy-mode no-op yielding false;
 * deleting the accessor would destroy the export — emit its value. */
function deleteNeutralizedEdit(ref: NodePath<t.Identifier>): Edit {
  return {
    start: offsetOf(ref.parent.start),
    end: offsetOf(ref.parent.end),
    text: "false"
  };
}

/** The splice for one cross-file READ of `target` (a member expression
 * string), form-aware so the rewrite is semantics-preserving in special
 * expression positions. */
function editForTarget(ref: NodePath<t.Identifier>, target: string): Edit {
  const node = ref.node;
  const parent = ref.parent;
  const start = offsetOf(node.start);
  const end = offsetOf(node.end);
  if (t.isObjectProperty(parent) && parent.shorthand && parent.value === node) {
    return { start, end, text: `${node.name}: ${target}` };
  }
  if (isBareCalleePos(parent, node)) {
    return { start, end, text: `(0, ${target})` };
  }
  return { start, end, text: target };
}

/** The splice for one cross-file WRITE target (assignment, destructuring
 * pattern element, update expression, for-in/of head). */
function editForWriteTarget(
  idPath: NodePath<t.Identifier>,
  target: string
): Edit {
  const node = idPath.node;
  const parent = idPath.parent;
  const start = offsetOf(node.start);
  const end = offsetOf(node.end);
  if (t.isObjectProperty(parent) && parent.shorthand && parent.value === node) {
    return { start, end, text: `${node.name}: ${target}` };
  }
  return { start, end, text: target };
}

type WriteKind = "write" | "var-redecl" | "fn-redecl" | "pattern-redecl";

/** Whether a write-target id sits INSIDE a declarator's binding pattern
 * (`var { x } = ...`) as opposed to being the declarator id itself. */
function insideDeclaratorPattern(idPath: NodePath<t.Identifier>): boolean {
  let p: NodePath | null = idPath.parentPath;
  for (; p && !p.isStatement(); p = p.parentPath) {
    if (p.isVariableDeclarator()) {
      const id = p.node.id;
      return (
        offsetOf(idPath.node.start) >= offsetOf(id.start) &&
        offsetOf(idPath.node.end) <= offsetOf(id.end)
      );
    }
  }
  return false;
}

function classifyWrite(idPath: NodePath<t.Identifier>): WriteKind {
  const parent = idPath.parentPath;
  if (parent?.isVariableDeclarator() && parent.node.id === idPath.node) {
    return "var-redecl";
  }
  if (parent?.isFunctionDeclaration() && parent.node.id === idPath.node) {
    return "fn-redecl";
  }
  if (insideDeclaratorPattern(idPath)) return "pattern-redecl";
  return "write";
}

interface PlanContext {
  plan: EmitPlan;
  declFile: string;
  ns: string;
}

/** Fold one write target into the plan. Returns true when it crossed. */
function planWriteTarget(
  ctx: PlanContext,
  name: string,
  idPath: NodePath<t.Identifier>
): boolean {
  const stmtIdx = stmtIndexOf(ctx.plan.ranges, offsetOf(idPath.node.start));
  const file = stmtIdx >= 0 ? ctx.plan.stmtFile[stmtIdx] : undefined;
  if (!file || file === ctx.declFile) return false;
  if (
    t.isUnaryExpression(idPath.parent) &&
    idPath.parent.operator === "delete"
  ) {
    // `delete x` is neutralized to `false` by the read rewrite; it never
    // writes, so it neither needs an edit here nor a setter.
    return false;
  }
  switch (classifyWrite(idPath)) {
    case "fn-redecl":
      throw new Error(
        `runnable emit: cross-file function redeclaration of "${name}" ` +
          "cannot preserve hoisting"
      );
    case "pattern-redecl":
      throw new Error(
        `runnable emit: cross-file redeclaration of "${name}" through a ` +
          "destructuring declarator is not supported"
      );
    case "var-redecl": {
      const redecls = ctx.plan.redeclByStmt.get(stmtIdx) ?? new Map();
      redecls.set(offsetOf(idPath.node.start), name);
      ctx.plan.redeclByStmt.set(stmtIdx, redecls);
      break;
    }
    case "write":
      addEdit(
        ctx.plan,
        stmtIdx,
        editForWriteTarget(idPath, `${ctx.ns}.${name}`)
      );
      break;
  }
  recordCrossSite(ctx.plan, stmtIdx, name, ctx.declFile, idPath);
  return true;
}

/** Fold one binding's reads into the plan. Returns true when any crossed. */
function planReads(ctx: PlanContext, name: string, binding: Binding): boolean {
  let crosses = false;
  for (const ref of binding.referencePaths) {
    if (!ref.isIdentifier()) continue;
    const stmtIdx = stmtIndexOf(ctx.plan.ranges, offsetOf(ref.node.start));
    const file = stmtIdx >= 0 ? ctx.plan.stmtFile[stmtIdx] : undefined;
    if (!file || file === ctx.declFile) continue;
    if (isDeleteArg(ref)) {
      // Neutralized in place — no namespace reference, no require edge.
      addEdit(ctx.plan, stmtIdx, deleteNeutralizedEdit(ref));
      continue;
    }
    addEdit(ctx.plan, stmtIdx, editForTarget(ref, `${ctx.ns}.${name}`));
    recordCrossSite(ctx.plan, stmtIdx, name, ctx.declFile, ref);
    crosses = true;
  }
  return crosses;
}

/** Fold one module binding into the plan, if it crosses a file boundary. */
function planBinding(plan: EmitPlan, name: string, binding: Binding): void {
  const declStmt = stmtIndexOf(plan.ranges, offsetOf(binding.identifier.start));
  if (declStmt < 0) return;
  const declFile = plan.stmtFile[declStmt];
  const ctx: PlanContext = { plan, declFile, ns: nsVarOf(plan, declFile) };
  const readCrosses = planReads(ctx, name, binding);
  let writable = false;
  for (const violation of binding.constantViolations) {
    for (const idPath of violationWriteTargetPaths(violation, name)) {
      if (planWriteTarget(ctx, name, idPath)) writable = true;
    }
  }
  if (readCrosses || writable) {
    plan.cross.set(name, { name, declFile, writable });
  }
}

/** Build the cross-file reference plan from the parsed wrapper + ledger. */
function buildPlan(
  statements: t.Statement[],
  scope: Scope,
  ledger: StableSplitLedger,
  directives: string[],
  wrapperNode: t.Node
): EmitPlan {
  const plan: EmitPlan = {
    statements,
    ranges: statements.map((s) => ({
      start: offsetOf(s.start),
      end: offsetOf(s.end)
    })),
    stmtFile: ledger.order,
    directives,
    wrapperNode,
    loadTimeEdges: new Map(),
    bundleContext: null,
    cross: new Map(),
    editsByStmt: new Map(),
    redeclByStmt: new Map(),
    exportsByFile: new Map(),
    requiresByFile: new Map(),
    nsVars: buildNsVars(ledger.files, scope)
  };
  for (const name of Object.keys(scope.bindings)) {
    const binding = scope.bindings[name];
    if (binding.kind === "param") continue;
    planBinding(plan, name, binding);
  }
  return plan;
}

/** Apply non-overlapping byte-range edits to a slice of the bundle. */
function applyEdits(src: string, base: number, edits: Edit[]): string {
  const sorted = [...edits].sort((a, b) => b.start - a.start);
  let out = src;
  let prevStart = Number.POSITIVE_INFINITY;
  for (const e of sorted) {
    if (e.end > prevStart) {
      throw new Error("runnable emit: overlapping rewrites (internal)");
    }
    prevStart = e.start;
    out = out.slice(0, e.start - base) + e.text + out.slice(e.end - base);
  }
  return out;
}

/** Remove and return the pool edits inside [start, end). */
function takeWithin(pool: Edit[], start: number, end: number): Edit[] {
  const taken: Edit[] = [];
  for (let i = pool.length - 1; i >= 0; i--) {
    if (pool[i].start >= start && pool[i].end <= end) {
      taken.push(...pool.splice(i, 1));
    }
  }
  return taken;
}

function assignmentFor(
  plan: EmitPlan,
  name: string,
  init: t.Expression | null | undefined,
  pool: Edit[],
  code: string
): string | null {
  if (!init) return null; // bare `var x;` redeclaration writes nothing
  const start = offsetOf(init.start);
  const end = offsetOf(init.end);
  const initText = applyEdits(
    code.slice(start, end),
    start,
    takeWithin(pool, start, end)
  );
  const cross = crossOf(plan, name);
  return `${nsVarOf(plan, cross.declFile)}.${name} = ${initText}`;
}

/** Rewrite a top-level `var` statement containing cross-file
 * redeclarations: cross declarators become setter assignments, local
 * declarators stay declarations, original order preserved. */
function declStatementComposite(
  plan: EmitPlan,
  decl: t.VariableDeclaration,
  redecls: Map<number, string>,
  pool: Edit[],
  code: string
): Edit {
  const lines: string[] = [];
  for (const d of decl.declarations) {
    const name = t.isIdentifier(d.id)
      ? redecls.get(offsetOf(d.id.start))
      : undefined;
    if (name) {
      const assign = assignmentFor(plan, name, d.init, pool, code);
      if (assign) lines.push(`${assign};`);
    } else {
      const start = offsetOf(d.start);
      const end = offsetOf(d.end);
      const text = applyEdits(
        code.slice(start, end),
        start,
        takeWithin(pool, start, end)
      );
      lines.push(`${decl.kind} ${text};`);
    }
  }
  return {
    start: offsetOf(decl.start),
    end: offsetOf(decl.end),
    text: lines.length > 0 ? lines.join("\n") : ";"
  };
}

/** Rewrite a `for (var x = ...;;)` init whose declarators are cross-file
 * redeclarations into comma-joined setter assignments. */
function forInitComposite(
  plan: EmitPlan,
  init: t.VariableDeclaration,
  redecls: Map<number, string>,
  pool: Edit[],
  code: string
): Edit {
  const parts: string[] = [];
  for (const d of init.declarations) {
    const name = t.isIdentifier(d.id)
      ? redecls.get(offsetOf(d.id.start))
      : undefined;
    if (!name) {
      throw new Error(
        "runnable emit: for-init mixes a cross-file var redeclaration with local declarators"
      );
    }
    const assign = assignmentFor(plan, name, d.init, pool, code);
    if (assign) parts.push(assign);
  }
  return {
    start: offsetOf(init.start),
    end: offsetOf(init.end),
    text: parts.join(", ")
  };
}

/** Rewrite `for (var x of/in ...)` heads: the redeclared target becomes a
 * namespace member (valid in for-head position without `var`). */
function forHeadComposite(
  plan: EmitPlan,
  left: t.VariableDeclaration,
  redecls: Map<number, string>
): Edit {
  const name = [...redecls.values()][0];
  const cross = crossOf(plan, name);
  return {
    start: offsetOf(left.start),
    end: offsetOf(left.end),
    text: `${nsVarOf(plan, cross.declFile)}.${name}`
  };
}

function allWithin(starts: number[], node: t.Node): boolean {
  return starts.every(
    (s) => s >= offsetOf(node.start) && s < offsetOf(node.end)
  );
}

/** Convert a statement's cross-file var redeclarations into composite
 * edits (which absorb the atomic edits inside their ranges). */
function withRedeclComposites(
  plan: EmitPlan,
  stmt: t.Statement,
  redecls: Map<number, string>,
  pool: Edit[],
  code: string
): Edit[] {
  const starts = [...redecls.keys()];
  if (t.isVariableDeclaration(stmt)) {
    return [declStatementComposite(plan, stmt, redecls, pool, code), ...pool];
  }
  if (
    t.isForStatement(stmt) &&
    t.isVariableDeclaration(stmt.init) &&
    allWithin(starts, stmt.init)
  ) {
    return [forInitComposite(plan, stmt.init, redecls, pool, code), ...pool];
  }
  if (
    (t.isForOfStatement(stmt) || t.isForInStatement(stmt)) &&
    t.isVariableDeclaration(stmt.left) &&
    allWithin(starts, stmt.left)
  ) {
    return [forHeadComposite(plan, stmt.left, redecls), ...pool];
  }
  const names = [...new Set(redecls.values())].join(", ");
  throw new Error(
    `runnable emit: cross-file var redeclaration of "${names}" in an unsupported position`
  );
}

/** Positional wrapper parameters — the CJS module context the bundler
 * injected. `__filename`/`__dirname` route to friendlier property names. */
const CONTEXT_PROPS = ["exports", "require", "module", "filename", "dirname"];

function reserveBundleVar(plan: EmitPlan, scope: Scope): string {
  const taken = new Set(plan.nsVars.values());
  let v = "__bundle";
  for (let n = 2; taken.has(v) || scope.hasBinding(v); n++) {
    v = `__bundle_${n}`;
  }
  return v;
}

function planContextReads(
  plan: EmitPlan,
  binding: Binding,
  target: string,
  ctxFiles: Set<string>
): void {
  for (const ref of binding.referencePaths) {
    if (!ref.isIdentifier()) continue;
    const stmtIdx = stmtIndexOf(plan.ranges, offsetOf(ref.node.start));
    if (stmtIdx < 0) continue;
    if (isDeleteArg(ref)) {
      addEdit(plan, stmtIdx, deleteNeutralizedEdit(ref));
      continue;
    }
    addEdit(plan, stmtIdx, editForTarget(ref, target));
    ctxFiles.add(plan.stmtFile[stmtIdx]);
  }
}

function planContextWrites(
  plan: EmitPlan,
  binding: Binding,
  paramName: string,
  target: string,
  ctxFiles: Set<string>
): void {
  for (const violation of binding.constantViolations) {
    for (const idPath of violationWriteTargetPaths(violation, paramName)) {
      const stmtIdx = stmtIndexOf(plan.ranges, offsetOf(idPath.node.start));
      if (stmtIdx < 0) continue;
      if (idPath.parentPath?.isVariableDeclarator()) {
        throw new Error(
          `runnable emit: wrapper parameter "${paramName}" is redeclared`
        );
      }
      addEdit(plan, stmtIdx, editForWriteTarget(idPath, target));
      ctxFiles.add(plan.stmtFile[stmtIdx]);
    }
  }
}

/** Route every reference to one wrapper parameter through the shared
 * bundle-context module. */
function planContextBinding(
  plan: EmitPlan,
  scope: Scope,
  paramName: string,
  target: string,
  ctxFiles: Set<string>
): void {
  const binding = scope.getBinding(paramName);
  if (!binding || binding.kind !== "param") return;
  planContextReads(plan, binding, target, ctxFiles);
  planContextWrites(plan, binding, paramName, target, ctxFiles);
}

/** `this` at the wrapper's top level (arrows included — they inherit it)
 * is the ONE shared receiver of the original wrapper invocation. Walking
 * up, the FIRST `this`-rebinding boundary decides ownership: a non-arrow
 * function binds its own `this`; a class field/static initializer or
 * static block binds the instance/class `this` — neither is the wrapper's,
 * so `this` inside them must NOT be routed (getFunctionParent alone skips
 * class-element boundaries and would mis-capture instance `this`). */
function thisBelongsToWrapper(p: NodePath, wrapperNode: t.Node): boolean {
  for (let cur: NodePath | null = p.parentPath; cur; cur = cur.parentPath) {
    if (cur.node === wrapperNode) return true;
    if (cur.isFunction() && !cur.isArrowFunctionExpression()) return false;
    if (
      cur.isClassProperty() ||
      cur.isClassPrivateProperty() ||
      cur.isStaticBlock()
    ) {
      return false;
    }
  }
  return false;
}

function planTopLevelThis(
  plan: EmitPlan,
  wrapperPath: NodePath<t.Function>,
  varName: string,
  ctxFiles: Set<string>
): void {
  const target = `${varName}.thisArg`;
  wrapperPath.traverse({
    ThisExpression: (p) => {
      if (!thisBelongsToWrapper(p, wrapperPath.node)) return;
      const stmtIdx = stmtIndexOf(plan.ranges, offsetOf(p.node.start));
      if (stmtIdx < 0) return;
      const text = isBareCalleePos(p.parent, p.node)
        ? `(0, ${target})`
        : target;
      addEdit(plan, stmtIdx, {
        start: offsetOf(p.node.start),
        end: offsetOf(p.node.end),
        text
      });
      ctxFiles.add(plan.stmtFile[stmtIdx]);
    }
  });
}

/** Fold the wrapper's own module context (exports, require, module,
 * __filename, __dirname, top-level this) into the plan: original code had
 * ONE shared context; the split routes it through _bundle.js, which the
 * entry initializes with ITS context. */
function planWrapperContext(
  plan: EmitPlan,
  wrapperPath: NodePath<t.Function>,
  scope: Scope
): void {
  const varName = reserveBundleVar(plan, scope);
  const ctxFiles = new Set<string>();
  wrapperPath.node.params.forEach((param, i) => {
    const prop = CONTEXT_PROPS[i];
    if (!prop || !t.isIdentifier(param)) return;
    planContextBinding(plan, scope, param.name, `${varName}.${prop}`, ctxFiles);
  });
  planTopLevelThis(plan, wrapperPath, varName, ctxFiles);
  if (ctxFiles.size > 0) {
    plan.bundleContext = { varName, files: ctxFiles, fileName: "" };
  }
}

/** Enforce at emit time what exp025 verified offline: no file may READ
 * another file's binding at load time inside a require cycle — mid-cycle
 * the accessor footer has not run and the read silently sees undefined. */
function assertLoadTimeAcyclic(plan: EmitPlan): void {
  const colors = new Map<string, "gray" | "black">();
  const stack: string[] = [];
  const visit = (file: string): void => {
    colors.set(file, "gray");
    stack.push(file);
    for (const dep of plan.loadTimeEdges.get(file) ?? []) {
      const color = colors.get(dep);
      if (color === "gray") {
        const cycle = [...stack.slice(stack.indexOf(dep)), dep].join(" -> ");
        throw new Error(`runnable emit: load-time reference cycle: ${cycle}`);
      }
      if (!color) visit(dep);
    }
    stack.pop();
    colors.set(file, "black");
  };
  for (const file of plan.loadTimeEdges.keys()) {
    if (!colors.has(file)) visit(file);
  }
}

function pickFreeFile(name: string, taken: ReadonlySet<string>): string {
  let v = name;
  while (taken.has(v)) {
    v = `_${v}`;
  }
  return v;
}

const BUNDLE_RUNTIME = `"use strict";

// Shared original-wrapper module context. index.js initializes it with
// the ENTRY module's require/module/__filename/__dirname/this, so every
// split file sees the single context the original bundle had. \`exports\`
// captures mod.exports at init time — matching the wrapper's \`exports\`
// parameter, which never retargets when module.exports is reassigned.
module.exports = {
  module: null,
  exports: null,
  require: null,
  filename: "",
  dirname: "",
  thisArg: null,
  init(mod, req, filename, dirname, thisArg) {
    this.module = mod;
    this.exports = mod.exports;
    this.require = req;
    this.filename = filename;
    this.dirname = dirname;
    this.thisArg = thisArg;
  }
};
`;

/** The entry module: initializes the shared bundle context, then loads
 * every file in the original bundle's first-statement order (so files
 * nothing requires still execute their top-level statements). */
function entrySource(
  plan: EmitPlan,
  ledger: StableSplitLedger,
  entryName: string
): string {
  const lines: string[] = [
    ...plan.directives,
    "// Entry for the runnable split tree: loads every module in the",
    "// original bundle's first-statement order."
  ];
  const bc = plan.bundleContext;
  if (bc) {
    lines.push(
      `const ${bc.varName} = require("${computeRelativeImportPath(entryName, bc.fileName)}");`,
      `${bc.varName}.init(module, require, __filename, __dirname, this);`,
      ""
    );
  }
  const seen = new Set<string>();
  for (const file of [...plan.stmtFile, ...ledger.files]) {
    if (seen.has(file)) continue;
    seen.add(file);
    lines.push(`require("${computeRelativeImportPath(entryName, file)}");`);
  }
  return `${lines.join("\n")}\n`;
}

/** One statement's emitted text: byte-exact when untouched, spliced when
 * it holds cross-file references, composite-rewritten on redeclarations. */
function stmtText(plan: EmitPlan, code: string, idx: number): string {
  const range = plan.ranges[idx];
  const edits = plan.editsByStmt.get(idx);
  const redecls = plan.redeclByStmt.get(idx);
  if (!edits && !redecls) return code.slice(range.start, range.end);
  const pool = [...(edits?.values() ?? [])];
  const finalEdits = redecls
    ? withRedeclComposites(plan, plan.statements[idx], redecls, pool, code)
    : pool;
  return applyEdits(
    code.slice(range.start, range.end),
    range.start,
    finalEdits
  );
}

function accessorLine(plan: EmitPlan, name: string): string {
  const cross = crossOf(plan, name);
  const body = cross.writable
    ? `{ get: () => ${name}, set: v => { ${name} = v; }, enumerable: true, configurable: true }`
    : `{ get: () => ${name}, enumerable: true, configurable: true }`;
  return `Object.defineProperty(module.exports, "${name}", ${body});`;
}

/** A file with no header whose first statement is a bare string literal
 * would promote it into an ACTIVE directive prologue; parenthesize the
 * expression so it stays the inert statement it was mid-wrapper. */
function neutralizeLeadingString(
  plan: EmitPlan,
  code: string,
  stmtIdxs: number[],
  body: string[]
): void {
  if (stmtIdxs.length === 0) return;
  const first = plan.statements[stmtIdxs[0]];
  if (t.isExpressionStatement(first) && t.isStringLiteral(first.expression)) {
    const expr = first.expression;
    body[0] = `(${code.slice(offsetOf(expr.start), offsetOf(expr.end))});`;
  }
}

/** Assemble one file: directive + require header, statements, accessor
 * footer. */
function assembleFile(
  plan: EmitPlan,
  code: string,
  file: string,
  stmtIdxs: number[]
): string {
  const header: string[] = [...plan.directives];
  const bc = plan.bundleContext;
  if (bc?.files.has(file)) {
    header.push(
      `const ${bc.varName} = require("${computeRelativeImportPath(file, bc.fileName)}");`
    );
  }
  const reqs = plan.requiresByFile.get(file);
  if (reqs) {
    for (const decl of [...reqs].sort()) {
      header.push(
        `const ${nsVarOf(plan, decl)} = require("${computeRelativeImportPath(file, decl)}");`
      );
    }
  }
  const body = stmtIdxs.map((idx) => stmtText(plan, code, idx));
  if (header.length === 0) neutralizeLeadingString(plan, code, stmtIdxs, body);
  const footer: string[] = [];
  const exps = plan.exportsByFile.get(file);
  if (exps) {
    for (const name of [...exps].sort()) {
      footer.push(accessorLine(plan, name));
    }
  }
  const sections = [header, body, footer].filter((s) => s.length > 0);
  return `${sections.map((s) => s.join("\n")).join("\n\n")}\n`;
}

function groupIndexesByFile(stmtFile: string[]): Map<string, number[]> {
  const map = new Map<string, number[]>();
  stmtFile.forEach((file, idx) => {
    const list = map.get(file);
    if (list) list.push(idx);
    else map.set(file, [idx]);
  });
  return map;
}

/**
 * Emit the runnable CommonJS module graph for a stable-split tree.
 * Returns a map of relative path → content. Throws with a reason when the
 * input cannot be represented faithfully (see module doc); use
 * `tryEmitRunnableCjs` to convert that into a reported decline.
 */
export function emitRunnableCjs(
  code: string,
  ledger: StableSplitLedger
): Map<string, string> {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("runnable emit: bundle failed to parse");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) {
    throw new Error(
      "runnable emit: input is not a single-wrapper bundle (wrapper detection declined)"
    );
  }
  const body = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(body)) {
    throw new Error("runnable emit: wrapper body is not a block statement");
  }
  validateLedger(ledger, body.body.length);
  const directives = body.directives.map((d) =>
    code.slice(offsetOf(d.start), offsetOf(d.end))
  );
  const plan = buildPlan(
    body.body,
    wrapper.scope,
    ledger,
    directives,
    wrapper.functionPath.node
  );
  planWrapperContext(plan, wrapper.functionPath, wrapper.scope);
  assertLoadTimeAcyclic(plan);
  return assembleTree(plan, code, ledger);
}

/** Assemble the full output tree: the ledger's files plus the generated
 * bundle-context runtime (when used) and the index.js entry. */
function assembleTree(
  plan: EmitPlan,
  code: string,
  ledger: StableSplitLedger
): Map<string, string> {
  const taken = new Set(ledger.files);
  if (plan.bundleContext) {
    plan.bundleContext.fileName = pickFreeFile("_bundle.js", taken);
    taken.add(plan.bundleContext.fileName);
  }
  const entryName = pickFreeFile("index.js", taken);

  const stmtIdxsByFile = groupIndexesByFile(plan.stmtFile);
  const out = new Map<string, string>();
  for (const file of ledger.files) {
    out.set(
      file,
      assembleFile(plan, code, file, stmtIdxsByFile.get(file) ?? [])
    );
  }
  if (plan.bundleContext) {
    out.set(plan.bundleContext.fileName, BUNDLE_RUNTIME);
  }
  out.set(entryName, entrySource(plan, ledger, entryName));
  return out;
}

/** The entry file within an emitted runnable tree — `index.js`, or a
 * `_`-prefixed variant if a split/factory file already claimed that name
 * (see pickFreeFile). Used by the runnable scaffolding to point its runner
 * at the right module. */
export function runnableEntryFile(files: Map<string, string>): string {
  for (const name of files.keys()) {
    if (/^_*index\.js$/.test(name)) return name;
  }
  return "index.js";
}

/**
 * emitRunnableCjs with the throw converted into a reported decline: the
 * reason is passed to `onDecline` and null is returned so the caller
 * falls back to the byte-exact review tree without losing the run.
 */
export function tryEmitRunnableCjs(
  code: string,
  ledger: StableSplitLedger,
  onDecline: (reason: string) => void
): Map<string, string> | null {
  try {
    return emitRunnableCjs(code, ledger);
  } catch (err) {
    onDecline(err instanceof Error ? err.message : String(err));
    return null;
  }
}
