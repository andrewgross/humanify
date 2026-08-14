/**
 * Read the Bun module fossils out of a wrapper body — the OWNER of that
 * question (docs/responsibility.md).
 *
 * A Bun bundle records its original file structure (exp068, validated
 * against a known-source corpus — the grammar lives in
 * `experiments/068-module-fossils/SPEC.md`): every lazily-forceable
 * source file compiles to a CONTIGUOUS wrapper segment terminated by its
 * `__esm` init definition; the init's leading zero-arg init calls are
 * that file's imports; helpers are identified by SHAPE because the
 * pipeline renames every identifier. Statements after the last init def
 * (the entry-point tail) belong to no module — the eager zone, which
 * rule 8 requires every consumer to COUNT, never hide.
 *
 * This is deliberately extracted at SPLIT time, from the split's own
 * parsed body, not at unpack time: statement indexes here must index the
 * same array the assignment labels, and the unpack stage's statement
 * space (pre-vendor-extraction, pre-rename) is a different one. The
 * adapter DECLARES the capability (`providesModuleFossils`); the split
 * exercises it where the indexes are valid — the same offsets-alignment
 * reasoning as the split's own private re-parse (responsibility.md,
 * "known exceptions").
 */
import type * as t from "@babel/types";

export interface FossilModule {
  /** wrapper index of the init def — the segment terminator. */
  initIndex: number;
  /** wrapper indexes of the segment, contiguous, init included. */
  statements: number[];
  /** rename-blind statement hashes of the segment, SORTED (the
   * cross-version signature). */
  hashes: string[];
  /** module indexes of leading init calls — the import edges. */
  imports: number[];
  /** names the segment declares (post-rename; same-version use only). */
  declared: string[];
}

export interface FossilExtract {
  modules: FossilModule[];
  /** wrapper indexes in no segment (statements after the last init). */
  eagerZone: number[];
}

function calleeName(e: t.Expression | t.V8IntrinsicIdentifier): string | null {
  return e.type === "Identifier" ? e.name : null;
}

function arrowThunkOf(
  e: t.Expression | null | undefined
): t.ArrowFunctionExpression | null {
  if (
    !e ||
    e.type !== "ArrowFunctionExpression" ||
    e.params.length !== 2 ||
    e.body.type !== "ArrowFunctionExpression" ||
    e.body.params.length !== 0
  ) {
    return null;
  }
  return e.body;
}

/** The `__esm` helper SHAPE: `(fn, res) => () => (…, <identifier>)`. */
function isEsmHelper(d: t.VariableDeclarator): boolean {
  if (d.id.type !== "Identifier" || !d.init) return false;
  const thunk = arrowThunkOf(d.init);
  if (!thunk || thunk.body.type !== "SequenceExpression") return false;
  const last = thunk.body.expressions[thunk.body.expressions.length - 1];
  return last.type === "Identifier";
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

interface RawInit {
  index: number;
  name: string;
  leading: string[];
}

/** Leading zero-arg identifier calls of an init body — the import edges. */
function leadingInitCalls(
  fn: t.ArrowFunctionExpression | t.FunctionExpression
): string[] {
  const leading: string[] = [];
  if (fn.body.type !== "BlockStatement") return leading;
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
  return leading;
}

function findInitDefs(body: t.Statement[], esmHelpers: Set<string>): RawInit[] {
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
      ) {
        continue;
      }
      const fn = d.init.arguments[0];
      if (
        fn.type !== "ArrowFunctionExpression" &&
        fn.type !== "FunctionExpression"
      ) {
        continue;
      }
      raw.push({ index: i, name: d.id.name, leading: leadingInitCalls(fn) });
    }
  }
  return raw;
}

/**
 * Extract the fossil modules of a wrapper body. `hashes` is the
 * rename-blind statementHash per statement, same order as `body` — the
 * caller (the split) computes them once for everything.
 */
function collectEsmHelpers(body: t.Statement[]): Set<string> {
  const helpers = new Set<string>();
  for (const stmt of body) {
    if (stmt.type !== "VariableDeclaration") continue;
    for (const d of stmt.declarations) {
      if (isEsmHelper(d) && d.id.type === "Identifier") {
        helpers.add(d.id.name);
      }
    }
  }
  return helpers;
}

function buildModule(
  r: RawInit,
  segStart: number,
  body: t.Statement[],
  hashes: string[],
  nameToModule: Map<string, number>
): FossilModule {
  const statements: number[] = [];
  for (let i = segStart; i <= r.index; i++) statements.push(i);
  const declared: string[] = [];
  for (const i of statements) declared.push(...declaredNames(body[i]));
  return {
    initIndex: r.index,
    statements,
    hashes: statements.map((i) => hashes[i]).sort(),
    imports: r.leading
      .map((n) => nameToModule.get(n))
      .filter((x): x is number => x !== undefined),
    declared
  };
}

export function extractFossilModules(
  body: t.Statement[],
  hashes: string[]
): FossilExtract {
  if (hashes.length !== body.length) {
    throw new Error(
      `fossil map: ${hashes.length} hashes for ${body.length} statements`
    );
  }
  const raw = findInitDefs(body, collectEsmHelpers(body)).sort(
    (a, b) => a.index - b.index
  );
  const nameToModule = new Map<string, number>();
  raw.forEach((r, k) => {
    nameToModule.set(r.name, k);
  });

  const modules: FossilModule[] = [];
  let prev = -1;
  for (const r of raw) {
    modules.push(buildModule(r, prev + 1, body, hashes, nameToModule));
    prev = r.index;
  }
  const eagerZone: number[] = [];
  for (let i = prev + 1; i < body.length; i++) eagerZone.push(i);
  return { modules, eagerZone };
}
