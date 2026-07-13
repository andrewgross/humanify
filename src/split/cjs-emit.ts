/**
 * Runnable CommonJS emission for a stable-split tree (exp026/exp027).
 *
 * The default split (`emitFiles` in stable-split.ts) writes byte-exact
 * statement slices — the review artifact. That tree parses but is not a
 * standalone module graph: cross-file references are bare identifiers
 * bound in the original single scope. This module emits the RUNNABLE
 * form: every cross-file reference goes through the declaring module via
 * LIVE namespace bindings.
 *
 *   - A file that reads/writes another file's binding does
 *     `const __decl = require("./decl.js")` once and every cross-file
 *     reference `x` is rewritten to `__decl.x` (reads, write targets,
 *     destructuring targets, update expressions, for-in/of heads).
 *   - The declaring file exports each cross-file binding as a live
 *     accessor: `Object.defineProperty(module.exports, "x", { get: () => x
 *     })`, plus `set: v => { x = v }` when the binding is written from
 *     another file.
 *   - Rewrites are byte-offset splices of the ORIGINAL parse's reference
 *     nodes — never a re-parse, never name matching — so shadowing locals
 *     are untouchable by construction and statements with no cross-file
 *     reference stay byte-exact.
 *   - Semantics-preserving special forms: a bare cross-file callee becomes
 *     `(0, __decl.x)(...)` (callee `this` stays undefined/globalThis);
 *     sloppy-mode `delete x` on a binding becomes `false` (its original
 *     value); a cross-file `var x = e` REdeclaration becomes the setter
 *     assignment `__decl.x = e`.
 *
 * The emitter THROWS (with a reason) instead of silently degrading when
 * the input violates what it can faithfully represent — non-wrapper
 * input, a ledger that disagrees with the statement count, cross-file
 * function redeclaration (hoisting is unpreservable), or redeclaration
 * through a destructuring declarator. `tryEmitRunnableCjs` converts a
 * throw into a reported decline so callers fall back to the byte-exact
 * review tree loudly, never losing the stable split or its ledger.
 * Emission is deterministic. Naming-only law is not engaged — this is
 * mechanical module extraction, never LLM rewriting.
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

function recordCrossSite(
  plan: EmitPlan,
  stmtIdx: number,
  name: string,
  declFile: string
): void {
  addTo(plan.exportsByFile, declFile, name);
  addTo(plan.requiresByFile, plan.stmtFile[stmtIdx], declFile);
}

function isBareCalleePos(parent: t.Node, node: t.Identifier): boolean {
  return (
    ((t.isCallExpression(parent) || t.isOptionalCallExpression(parent)) &&
      parent.callee === node) ||
    (t.isTaggedTemplateExpression(parent) && parent.tag === node)
  );
}

/** The splice for one cross-file READ, form-aware so the rewrite is
 * semantics-preserving in special expression positions. */
function editForRead(ref: NodePath<t.Identifier>, ns: string): Edit {
  const node = ref.node;
  const parent = ref.parent;
  const start = offsetOf(node.start);
  const end = offsetOf(node.end);
  if (t.isObjectProperty(parent) && parent.shorthand && parent.value === node) {
    return { start, end, text: `${node.name}: ${ns}.${node.name}` };
  }
  if (isBareCalleePos(parent, node)) {
    return { start, end, text: `(0, ${ns}.${node.name})` };
  }
  if (
    t.isUnaryExpression(parent) &&
    parent.operator === "delete" &&
    parent.argument === node
  ) {
    // `delete x` on a binding is a sloppy-mode no-op yielding false;
    // deleting the accessor would destroy the export.
    return {
      start: offsetOf(parent.start),
      end: offsetOf(parent.end),
      text: "false"
    };
  }
  return { start, end, text: `${ns}.${node.name}` };
}

/** The splice for one cross-file WRITE target (assignment, destructuring
 * pattern element, update expression, for-in/of head). */
function editForWrite(idPath: NodePath<t.Identifier>, ns: string): Edit {
  const node = idPath.node;
  const parent = idPath.parent;
  const start = offsetOf(node.start);
  const end = offsetOf(node.end);
  if (t.isObjectProperty(parent) && parent.shorthand && parent.value === node) {
    return { start, end, text: `${node.name}: ${ns}.${node.name}` };
  }
  return { start, end, text: `${ns}.${node.name}` };
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
      addEdit(ctx.plan, stmtIdx, editForWrite(idPath, ctx.ns));
      break;
  }
  recordCrossSite(ctx.plan, stmtIdx, name, ctx.declFile);
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
    addEdit(ctx.plan, stmtIdx, editForRead(ref, ctx.ns));
    recordCrossSite(ctx.plan, stmtIdx, name, ctx.declFile);
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
  ledger: StableSplitLedger
): EmitPlan {
  const plan: EmitPlan = {
    statements,
    ranges: statements.map((s) => ({
      start: offsetOf(s.start),
      end: offsetOf(s.end)
    })),
    stmtFile: ledger.order,
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

/** Assemble one file: require header, statements, accessor footer. */
function assembleFile(
  plan: EmitPlan,
  code: string,
  file: string,
  stmtIdxs: number[]
): string {
  const lines: string[] = [];
  const reqs = plan.requiresByFile.get(file);
  if (reqs) {
    for (const decl of [...reqs].sort()) {
      lines.push(
        `const ${nsVarOf(plan, decl)} = require("${computeRelativeImportPath(file, decl)}");`
      );
    }
    lines.push("");
  }
  for (const idx of stmtIdxs) {
    lines.push(stmtText(plan, code, idx));
  }
  const exps = plan.exportsByFile.get(file);
  if (exps) {
    lines.push("");
    for (const name of [...exps].sort()) {
      lines.push(accessorLine(plan, name));
    }
  }
  return `${lines.join("\n")}\n`;
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
  const plan = buildPlan(body.body, wrapper.scope, ledger);

  const stmtIdxsByFile = groupIndexesByFile(plan.stmtFile);
  const out = new Map<string, string>();
  for (const file of ledger.files) {
    out.set(
      file,
      assembleFile(plan, code, file, stmtIdxsByFile.get(file) ?? [])
    );
  }
  return out;
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
