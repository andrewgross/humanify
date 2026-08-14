/**
 * 068 Task 1 — apply the fossil grammar (SPEC.md) read-only to a saved
 * humanified bundle.
 *
 *   npx tsx experiments/068-module-fossils/census.ts <humanified.js> [label]
 *
 * Reports: init-function count, exports-object count, per-segment
 * statement attribution (write-set + contiguity), the unattributable
 * eager zone, and the init-call import graph's shape. The bundle is
 * post-rename; the grammar is rename-blind (it keys on `__esm`,
 * `__export`, `__commonJS` helper calls, which the pipeline preserves).
 */
import * as fs from "node:fs";
import { parse } from "@babel/parser";
import type * as t from "@babel/types";

const [BUNDLE, LABEL = ""] = process.argv.slice(2);
if (!BUNDLE) {
  console.error("usage: census.ts <humanified.js> [label]");
  process.exit(1);
}

const code = fs.readFileSync(BUNDLE, "utf8");
const ast = parse(code, { sourceType: "unambiguous", errorRecovery: false });

// Find the wrapper: the largest BlockStatement body in the file.
let body: t.Statement[] = [];
(function walk(node: unknown): void {
  if (!node || typeof node !== "object") return;
  if (Array.isArray(node)) {
    for (const c of node) walk(c);
    return;
  }
  const n = node as { type?: string; body?: unknown };
  if (
    n.type === "BlockStatement" &&
    Array.isArray(n.body) &&
    (n.body as t.Statement[]).length > body.length
  ) {
    body = n.body as t.Statement[];
  }
  for (const k of Object.keys(n)) {
    if (k === "loc") continue;
    walk((n as Record<string, unknown>)[k]);
  }
})(ast.program);

interface InitDef {
  index: number;
  name: string;
  writes: string[];
  leadingInitCalls: string[];
}

function calleeName(e: t.Expression | t.V8IntrinsicIdentifier): string | null {
  return e.type === "Identifier" ? e.name : null;
}

// ── helper identification by SHAPE (names are pipeline-drawn per run) ──────
// __esm:      (a, b) => () => (a && (b = a(a = 0)), b)
// __commonJS: (cb, m) => () => (m || cb((m = {exports:{}}).exports, m), m.exports)
// __export:   (target, all) => { for (var k in all) defineProperty(...get...) }
function isArrow2ReturningThunk(
  e: t.Expression | null | undefined
): t.ArrowFunctionExpression | null {
  if (
    !e ||
    e.type !== "ArrowFunctionExpression" ||
    e.params.length !== 2 ||
    e.body.type !== "ArrowFunctionExpression" ||
    e.body.params.length !== 0
  )
    return null;
  return e.body;
}

function classifyHelper(
  d: t.VariableDeclarator
): "esm" | "commonjs" | "export" | null {
  if (d.id.type !== "Identifier") return null;
  const init = d.init;
  if (!init) return null;
  const thunk = isArrow2ReturningThunk(init);
  if (thunk && thunk.body.type === "SequenceExpression") {
    const last = thunk.body.expressions[thunk.body.expressions.length - 1];
    if (last.type === "Identifier") return "esm";
    if (last.type === "MemberExpression") return "commonjs";
  }
  if (
    init.type === "ArrowFunctionExpression" &&
    init.params.length === 2 &&
    init.body.type === "BlockStatement" &&
    init.body.body.some((s) => s.type === "ForInStatement")
  )
    return "export";
  return null;
}

// Pass 1: find the helper names by shape.
const esmHelpers = new Set<string>();
const commonJSHelpers = new Set<string>();
const exportHelpers = new Set<string>();
{
  let bodyRef: t.Statement[] = [];
  (function findBody(node: unknown): void {
    if (!node || typeof node !== "object") return;
    if (Array.isArray(node)) {
      for (const c of node) findBody(c);
      return;
    }
    const n = node as { type?: string; body?: unknown };
    if (
      n.type === "BlockStatement" &&
      Array.isArray(n.body) &&
      (n.body as t.Statement[]).length > bodyRef.length
    )
      bodyRef = n.body as t.Statement[];
    for (const k of Object.keys(n)) {
      if (k === "loc") continue;
      findBody((n as Record<string, unknown>)[k]);
    }
  })(ast.program);
  for (const stmt of bodyRef) {
    if (stmt.type !== "VariableDeclaration") continue;
    for (const d of stmt.declarations) {
      const kind = classifyHelper(d);
      if (!kind || d.id.type !== "Identifier") continue;
      if (kind === "esm") esmHelpers.add(d.id.name);
      else if (kind === "commonjs") commonJSHelpers.add(d.id.name);
      else exportHelpers.add(d.id.name);
    }
  }
}

/** `var X = <esmHelper>(() => {...})` (possibly multi-declarator). */
function initDefOf(stmt: t.Statement, index: number): InitDef[] {
  if (stmt.type !== "VariableDeclaration") return [];
  const defs: InitDef[] = [];
  for (const d of stmt.declarations) {
    if (
      d.id.type !== "Identifier" ||
      !d.init ||
      d.init.type !== "CallExpression" ||
      !esmHelpers.has(calleeName(d.init.callee) ?? "") ||
      d.init.arguments.length < 1
    )
      continue;
    const fn = d.init.arguments[0];
    if (fn.type !== "ArrowFunctionExpression" && fn.type !== "FunctionExpression")
      continue;
    const writes: string[] = [];
    const leading: string[] = [];
    if (fn.body.type === "BlockStatement") {
      let inLeading = true;
      for (const s of fn.body.body) {
        if (
          s.type === "ExpressionStatement" &&
          s.expression.type === "CallExpression" &&
          s.expression.arguments.length === 0 &&
          s.expression.callee.type === "Identifier"
        ) {
          if (inLeading) leading.push(s.expression.callee.name);
          continue;
        }
        inLeading = false;
        if (
          s.type === "ExpressionStatement" &&
          s.expression.type === "AssignmentExpression" &&
          s.expression.left.type === "Identifier"
        ) {
          writes.push(s.expression.left.name);
        }
      }
    }
    defs.push({ index, name: d.id.name, writes, leadingInitCalls: leading });
  }
  return defs;
}

/** `__export(E, {...})` statement → exported local names. */
function exportMapOf(stmt: t.Statement): { obj: string; locals: string[] } | null {
  if (
    stmt.type !== "ExpressionStatement" ||
    stmt.expression.type !== "CallExpression" ||
    !exportHelpers.has(calleeName(stmt.expression.callee) ?? "") ||
    stmt.expression.arguments.length < 2
  )
    return null;
  const [objArg, mapArg] = stmt.expression.arguments;
  if (objArg.type !== "Identifier" || mapArg.type !== "ObjectExpression")
    return null;
  const locals: string[] = [];
  for (const p of mapArg.properties) {
    if (
      p.type === "ObjectProperty" &&
      (p.value.type === "ArrowFunctionExpression" ||
        p.value.type === "FunctionExpression")
    ) {
      const b = p.value.body;
      if (b.type === "Identifier") locals.push(b.name);
    }
  }
  return { obj: objArg.name, locals };
}

const inits: InitDef[] = [];
const exportObjs = new Map<number, { obj: string; locals: string[] }>();
let commonJSFactories = 0;
for (let i = 0; i < body.length; i++) {
  const stmt = body[i];
  inits.push(...initDefOf(stmt, i));
  const em = exportMapOf(stmt);
  if (em) exportObjs.set(i, em);
  if (
    stmt.type === "VariableDeclaration" &&
    stmt.declarations.some(
      (d) =>
        d.init?.type === "CallExpression" &&
        commonJSHelpers.has(calleeName(d.init.callee) ?? "")
    )
  )
    commonJSFactories++;
}

const initNames = new Set(inits.map((d) => d.name));
// Import edges: leading init calls inside init bodies that name other inits.
let edges = 0;
let externalLeading = 0;
for (const d of inits) {
  for (const c of d.leadingInitCalls) {
    if (initNames.has(c)) edges++;
    else externalLeading++;
  }
}

// Segment attribution: statements between consecutive init defs belong to
// the segment ENDED by the following init def (contiguity per rule 2),
// measured only BETWEEN inits — statements after the last init def or in a
// bundle region with no inits are the eager zone.
const initIndexes = inits.map((d) => d.index).sort((a, b) => a - b);
let segmented = 0;
if (initIndexes.length > 0) {
  let prev = -1;
  for (const idx of initIndexes) {
    segmented += idx - prev; // includes the init def itself
    prev = idx;
  }
}
const eagerZone = body.length - segmented;

// Write-set / export-map corroborated statements: how many segment
// statements DECLARE a name some init writes or some export map exposes.
const corroborated = new Set<string>();
for (const d of inits) for (const w of d.writes) corroborated.add(w);
for (const [, em] of exportObjs) for (const l of em.locals) corroborated.add(l);
let corroboratedStmts = 0;
for (const stmt of body) {
  const declared: string[] = [];
  if (stmt.type === "FunctionDeclaration" && stmt.id) declared.push(stmt.id.name);
  if (stmt.type === "VariableDeclaration")
    for (const d of stmt.declarations)
      if (d.id.type === "Identifier") declared.push(d.id.name);
  if (declared.some((n) => corroborated.has(n))) corroboratedStmts++;
}

const pct = (n: number, d: number) =>
  d ? `${((100 * n) / d).toFixed(1)}%` : "n/a";
console.log(`=== 068 fossil census — ${LABEL || BUNDLE} ===`);
console.log(`  wrapper statements            ${body.length}`);
console.log(`  __esm init defs               ${inits.length}`);
console.log(`  __export namespace objects    ${exportObjs.size}`);
console.log(`  __commonJS factories          ${commonJSFactories}`);
console.log(
  `  init->init import edges       ${edges} (leading calls naming another init)`
);
console.log(`  leading calls, non-init       ${externalLeading}`);
console.log(
  `  segment-covered statements    ${segmented}  ${pct(segmented, body.length)}`
);
console.log(
  `  eager zone (no fossil)        ${eagerZone}  ${pct(eagerZone, body.length)}`
);
console.log(
  `  write/export-corroborated     ${corroboratedStmts}  ${pct(corroboratedStmts, body.length)}`
);
console.log(
  `ROW|${LABEL}|${body.length}|${inits.length}|${exportObjs.size}|${edges}|${segmented}|${eagerZone}|${corroboratedStmts}`
);
