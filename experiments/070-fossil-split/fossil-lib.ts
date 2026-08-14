/**
 * 070 shared extraction — exp068's fossil grammar (SPEC.md), factored so
 * the matcher and ceiling read the same modules. Rename-blind: helpers
 * are identified by SHAPE, segments by contiguity, signatures by the
 * split-ledger's identifier-blind statementHash.
 */
import * as fs from "node:fs";
import { parse } from "@babel/parser";
import type * as t from "@babel/types";

export interface FossilModule {
  /** init def's wrapper statement index (segment END, inclusive) */
  initIndex: number;
  /** wrapper statement indexes of this segment (contiguous) */
  statements: number[];
  /** rename-blind statement hashes of the segment (sorted multiset) */
  hashes: string[];
  /** init-index targets of leading init calls (imports) */
  imports: number[];
  /** names declared by segment statements (post-rename; same-version use only) */
  declared: Set<string>;
  /** init binding name (post-rename; same-version use only) */
  initName: string;
}

export interface FossilExtract {
  modules: FossilModule[];
  wrapperStatements: number;
  eagerZone: number[];
  /** statements not in any segment (rule 8) */
  unattributed: number;
}

function largestBlock(ast: t.File): t.Statement[] {
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
  return body;
}

function calleeName(e: t.Expression | t.V8IntrinsicIdentifier): string | null {
  return e.type === "Identifier" ? e.name : null;
}

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

function classifyHelper(d: t.VariableDeclarator): "esm" | "commonjs" | null {
  if (d.id.type !== "Identifier" || !d.init) return null;
  const thunk = isArrow2ReturningThunk(d.init);
  if (thunk && thunk.body.type === "SequenceExpression") {
    const last = thunk.body.expressions[thunk.body.expressions.length - 1];
    if (last.type === "Identifier") return "esm";
    if (last.type === "MemberExpression") return "commonjs";
  }
  return null;
}

function declaredNames(stmt: t.Statement): string[] {
  const out: string[] = [];
  if (stmt.type === "FunctionDeclaration" && stmt.id) out.push(stmt.id.name);
  if (stmt.type === "ClassDeclaration" && stmt.id) out.push(stmt.id.name);
  if (stmt.type === "VariableDeclaration") {
    for (const d of stmt.declarations) {
      if (d.id.type === "Identifier") out.push(d.id.name);
    }
  }
  return out;
}

export function extractFossils(
  bundlePath: string,
  ledgerPath: string
): FossilExtract {
  const code = fs.readFileSync(bundlePath, "utf8");
  const ast = parse(code, { sourceType: "unambiguous", errorRecovery: false });
  const body = largestBlock(ast);
  const ledger = JSON.parse(fs.readFileSync(ledgerPath, "utf8")) as {
    hashes?: string[];
  };
  const hashes = ledger.hashes ?? [];
  if (hashes.length !== body.length) {
    throw new Error(
      `ledger/body mismatch: ${hashes.length} hashes vs ${body.length} statements (${bundlePath})`
    );
  }

  // Pass 1: helper names by shape.
  const esmHelpers = new Set<string>();
  for (const stmt of body) {
    if (stmt.type !== "VariableDeclaration") continue;
    for (const d of stmt.declarations) {
      if (classifyHelper(d) === "esm" && d.id.type === "Identifier")
        esmHelpers.add(d.id.name);
    }
  }

  // Pass 2: init defs with leading init calls.
  interface RawInit {
    index: number;
    name: string;
    leading: string[];
  }
  const raw: RawInit[] = [];
  for (let i = 0; i < body.length; i++) {
    const stmt = body[i];
    if (stmt.type !== "VariableDeclaration") continue;
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
      if (
        fn.type !== "ArrowFunctionExpression" &&
        fn.type !== "FunctionExpression"
      )
        continue;
      const leading: string[] = [];
      if (fn.body.type === "BlockStatement") {
        for (const s of fn.body.body) {
          if (
            s.type === "ExpressionStatement" &&
            s.expression.type === "CallExpression" &&
            s.expression.arguments.length === 0 &&
            s.expression.callee.type === "Identifier"
          ) {
            leading.push(s.expression.callee.name);
            continue;
          }
          break;
        }
      }
      raw.push({ index: i, name: d.id.name, leading });
    }
  }

  // Sort FIRST, then index by final position — imports must reference
  // indexes into the final modules array.
  raw.sort((a, b) => a.index - b.index);
  const nameToInit = new Map<string, number>();
  raw.forEach((r, k) => nameToInit.set(r.name, k));

  // Segments: statements between consecutive init defs belong to the segment
  // ENDED by the following init def (SPEC rule 2).
  const modules: FossilModule[] = [];
  let prev = -1;
  for (const r of raw) {
    const statements: number[] = [];
    for (let i = prev + 1; i <= r.index; i++) statements.push(i);
    prev = r.index;
    const declared = new Set<string>();
    for (const i of statements)
      for (const n of declaredNames(body[i])) declared.add(n);
    modules.push({
      initIndex: r.index,
      statements,
      hashes: statements.map((i) => hashes[i]).sort(),
      imports: r.leading
        .map((n) => nameToInit.get(n))
        .filter((x): x is number => x !== undefined),
      declared,
      initName: r.name
    });
  }
  const covered = new Set(modules.flatMap((m) => m.statements));
  const eagerZone: number[] = [];
  for (let i = 0; i < body.length; i++) if (!covered.has(i)) eagerZone.push(i);
  return {
    modules,
    wrapperStatements: body.length,
    eagerZone,
    unattributed: eagerZone.length
  };
}
