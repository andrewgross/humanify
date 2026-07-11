/**
 * Runnable CommonJS emission for a stable-split tree (exp026/exp027).
 *
 * The default split (`emitFiles` in stable-split.ts) writes byte-exact
 * statement slices — the review artifact. That tree parses but is not a
 * standalone module graph: cross-file references are bare identifiers
 * bound in the original single scope. This module emits the RUNNABLE
 * form: every cross-file reference goes through the declaring module via
 * LIVE namespace bindings, so the graph executes correctly under circular
 * requires and cross-file mutation.
 *
 *   - A file that reads/writes another file's binding does
 *     `const __decl = require("./decl.js")` once and every cross-file
 *     reference `x` is rewritten to `__decl.x` (reads and write targets).
 *     A namespace property access is a live read, so a value captured
 *     mid-cycle is never stale (reads execute after load), and writes
 *     propagate.
 *   - The declaring file exports each cross-file binding as a live
 *     accessor: `Object.defineProperty(module.exports, "x", { get: () => x
 *     })`, plus `set: v => { x = v }` when the binding is written from
 *     another file.
 *
 * Statements with no cross-file reference stay byte-exact; only the ones
 * that reach across a file boundary are regenerated from the rewritten
 * AST. Emission is deterministic. Naming-only law is not engaged — this
 * is mechanical module extraction, never LLM rewriting.
 */

import { parseSync } from "@babel/core";
import type { Binding, NodePath, Scope } from "@babel/traverse";
import * as t from "@babel/types";
import { generate, traverse } from "../babel-utils.js";
import type { StableSplitLedger } from "./stable-split.js";

interface CrossBinding {
  name: string;
  declFile: string;
  /** Written from a file other than declFile — needs a setter. */
  writable: boolean;
}

interface EmitPlan {
  statements: t.Statement[];
  stmtFile: string[];
  /** name → cross-file binding record (only bindings that cross a file). */
  cross: Map<string, CrossBinding>;
  /** statement index → the cross-file names it references. */
  namesByStmt: Map<number, Set<string>>;
  /** declFile → names to export via accessor. */
  exportsByFile: Map<string, Set<string>>;
  /** readerFile → declFiles it must require. */
  requiresByFile: Map<string, Set<string>>;
}

function wrapperOf(ast: t.File): t.FunctionExpression | null {
  const first = ast.program.body[0];
  if (!t.isExpressionStatement(first)) return null;
  let expr = first.expression;
  if (t.isCallExpression(expr)) expr = expr.callee as t.Expression;
  if (!t.isFunctionExpression(expr) || !t.isBlockStatement(expr.body)) {
    return null;
  }
  return expr;
}

/** Locate the wrapper scope (the module scope of the split). */
function wrapperScopeOf(ast: t.File): Scope | undefined {
  let scope: Scope | undefined;
  traverse(ast, {
    FunctionExpression(p) {
      scope = p.scope;
      p.stop();
    }
  });
  return scope;
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

/** The write-target identifier nodes of one constant violation named `name`. */
function violationWriteIds(violation: NodePath, name: string): t.Identifier[] {
  const out: t.Identifier[] = [];
  const ids = t.getBindingIdentifiers(violation.node, true);
  for (const entry of Object.values(ids)) {
    for (const id of Array.isArray(entry) ? entry : [entry]) {
      if (id.name === name) out.push(id);
    }
  }
  return out;
}

/** Every identifier node that reads or writes `binding`. */
function bindingSites(binding: Binding): t.Identifier[] {
  const sites: t.Identifier[] = [];
  for (const ref of binding.referencePaths) {
    if (ref.isIdentifier()) sites.push(ref.node);
  }
  for (const violation of binding.constantViolations) {
    sites.push(...violationWriteIds(violation, binding.identifier.name));
  }
  return sites;
}

type FileResolver = (pos: number) => string | undefined;

/** Classify a binding's cross-file status: whether any site crosses the
 * declaring file, and whether any cross-file write exists (needs a setter). */
function crossStatus(
  binding: Binding,
  sites: t.Identifier[],
  declFile: string,
  fileOfPos: FileResolver
): { crosses: boolean; writable: boolean } {
  let crosses = false;
  let writable = false;
  for (const id of sites) {
    const siteFile = fileOfPos(id.start ?? -1);
    if (siteFile && siteFile !== declFile) crosses = true;
  }
  for (const violation of binding.constantViolations) {
    if (fileOfPos(violation.node.start ?? -1) !== declFile) writable = true;
  }
  return { crosses, writable };
}

/** Fold one module binding into the plan, if it crosses a file boundary. */
function planBinding(
  plan: EmitPlan,
  ranges: Array<{ start: number; end: number }>,
  fileOfPos: FileResolver,
  name: string,
  binding: Binding
): void {
  const declFile = fileOfPos(binding.identifier.start ?? -1);
  if (!declFile) return;
  const sites = bindingSites(binding);
  const { crosses, writable } = crossStatus(
    binding,
    sites,
    declFile,
    fileOfPos
  );
  if (!crosses) return;
  plan.cross.set(name, { name, declFile, writable });
  recordSites(plan, ranges, sites, name, declFile);
}

/** Build the cross-file reference plan from the parsed bundle + ledger. */
function buildPlan(ast: t.File, ledger: StableSplitLedger): EmitPlan | null {
  const wrapper = wrapperOf(ast);
  const scope = wrapperScopeOf(ast);
  if (!wrapper || !scope) return null;
  const statements = wrapper.body.body as t.Statement[];
  const ranges = statements.map((s) => ({
    start: s.start ?? 0,
    end: s.end ?? 0
  }));
  const fileOfPos: FileResolver = (pos) =>
    ledger.order[stmtIndexOf(ranges, pos)];

  const plan: EmitPlan = {
    statements,
    stmtFile: ledger.order,
    cross: new Map(),
    namesByStmt: new Map(),
    exportsByFile: new Map(),
    requiresByFile: new Map()
  };
  for (const name of Object.keys(scope.bindings)) {
    planBinding(plan, ranges, fileOfPos, name, scope.bindings[name]);
  }
  return plan;
}

/** Register each cross-file site under its statement, and the resulting
 * require/export edges. */
function recordSites(
  plan: EmitPlan,
  ranges: Array<{ start: number; end: number }>,
  sites: t.Identifier[],
  name: string,
  declFile: string
): void {
  for (const id of sites) {
    const idx = stmtIndexOf(ranges, id.start ?? -1);
    if (idx < 0) continue;
    const readerFile = plan.stmtFile[idx];
    if (readerFile === declFile) continue;
    addTo(plan.namesByStmt, idx, name);
    addTo(plan.exportsByFile, declFile, name);
    addTo(plan.requiresByFile, readerFile, declFile);
  }
}

function addTo<K, V>(map: Map<K, Set<V>>, key: K, value: V): void {
  const set = map.get(key);
  if (set) set.add(value);
  else map.set(key, new Set([value]));
}

function baseVar(file: string): string {
  return `__${file.replace(/[^A-Za-z0-9_$]/g, "_")}`;
}

function relRequire(from: string, to: string): string {
  const fromDir = from.includes("/")
    ? from.slice(0, from.lastIndexOf("/"))
    : "";
  const fromParts = fromDir ? fromDir.split("/") : [];
  const toParts = to.split("/");
  let i = 0;
  while (
    i < fromParts.length &&
    i < toParts.length - 1 &&
    fromParts[i] === toParts[i]
  ) {
    i++;
  }
  const up = fromParts.slice(i).map(() => "..");
  const down = toParts.slice(i);
  const rel = [...up, ...down].join("/");
  return rel.startsWith(".") ? rel : `./${rel}`;
}

/** Rewrite a single statement's cross-file references to namespace
 * accesses and regenerate it. Skips shadowing locals of the same name. */
function rewriteStatement(
  plan: EmitPlan,
  src: string,
  names: Set<string>
): string {
  const stmtAst = parseSync(src, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File | null;
  if (!stmtAst) return src;
  traverse(stmtAst, {
    Identifier(p: NodePath<t.Identifier>) {
      if (!names.has(p.node.name)) return;
      if (!p.isReferencedIdentifier() && !isWriteTarget(p)) return;
      const cross = plan.cross.get(p.node.name);
      if (!cross) return;
      // A binding resolved inside this statement is a shadowing local —
      // leave it; only the free (module-scope) reference is rewritten.
      const bound = p.scope.getBinding(p.node.name);
      if (bound && bound.scope.block !== stmtAst.program) return;
      p.replaceWith(
        t.memberExpression(
          t.identifier(baseVar(cross.declFile)),
          t.identifier(p.node.name)
        )
      );
      p.skip();
    }
  });
  return generate(stmtAst.program.body[0] as t.Node).code;
}

function isWriteTarget(p: NodePath<t.Identifier>): boolean {
  const parent = p.parent;
  return (
    (t.isAssignmentExpression(parent) && parent.left === p.node) ||
    (t.isUpdateExpression(parent) && parent.argument === p.node)
  );
}

function accessorLine(cross: CrossBinding): string {
  const body = cross.writable
    ? `{ get: () => ${cross.name}, set: v => { ${cross.name} = v; }, enumerable: true, configurable: true }`
    : `{ get: () => ${cross.name}, enumerable: true, configurable: true }`;
  return `Object.defineProperty(module.exports, "${cross.name}", ${body});`;
}

/** Assemble one file: require header, (rewritten) statements, accessor
 * exports footer. */
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
        `const ${baseVar(decl)} = require("${relRequire(file, decl)}");`
      );
    }
    lines.push("");
  }
  for (const idx of stmtIdxs) {
    const stmt = plan.statements[idx];
    const src = code.slice(stmt.start ?? 0, stmt.end ?? 0);
    const names = plan.namesByStmt.get(idx);
    lines.push(names ? rewriteStatement(plan, src, names) : src);
  }
  const exps = plan.exportsByFile.get(file);
  if (exps) {
    lines.push("");
    for (const name of [...exps].sort()) {
      const cross = plan.cross.get(name);
      if (cross) lines.push(accessorLine(cross));
    }
  }
  return `${lines.join("\n")}\n`;
}

/**
 * Emit the runnable CommonJS module graph for a stable-split tree. Returns
 * a map of relative path → content, or null when the code is not a
 * single-wrapper bundle (caller falls back to the byte-exact tree).
 */
export function emitRunnableCjs(
  code: string,
  ledger: StableSplitLedger
): Map<string, string> | null {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File | null;
  if (!ast) return null;
  const plan = buildPlan(ast, ledger);
  if (!plan) return null;

  const stmtIdxsByFile = new Map<string, number[]>();
  plan.stmtFile.forEach((file, idx) => {
    const list = stmtIdxsByFile.get(file);
    if (list) list.push(idx);
    else stmtIdxsByFile.set(file, [idx]);
  });

  const out = new Map<string, string>();
  for (const file of ledger.files) {
    out.set(
      file,
      assembleFile(plan, code, file, stmtIdxsByFile.get(file) ?? [])
    );
  }
  return out;
}
