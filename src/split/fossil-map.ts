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
  /**
   * The ORIGINAL source path, when the bundler kept it — esbuild's
   * unminified form uses it as the init object's key. Absent for bun and
   * for any minified build, so nothing may depend on it; it is a gift when
   * present (exp075).
   */
  sourcePath?: string;
}

export interface FossilExtract {
  modules: FossilModule[];
  /** wrapper indexes in no segment (statements after the last init). */
  eagerZone: number[];
}

function calleeName(e: t.Expression | t.V8IntrinsicIdentifier): string | null {
  return e.type === "Identifier" ? e.name : null;
}

function thunkOf(
  e: t.Expression | null | undefined
): t.ArrowFunctionExpression | t.FunctionExpression | null {
  if (!e || e.type !== "ArrowFunctionExpression" || e.params.length !== 2) {
    return null;
  }
  const body = e.body;
  if (
    (body.type === "ArrowFunctionExpression" ||
      body.type === "FunctionExpression") &&
    body.params.length === 0
  ) {
    return body;
  }
  return null;
}

/**
 * The `__esm` helper SHAPE, in the two forms we have observed:
 *   bun:     `(fn, res) => () => (…, <identifier>)`
 *   esbuild: `(fn, res) => function __init() { return (…), <identifier>; }`
 * Both are a two-parameter arrow whose body is a zero-arg thunk ending in an
 * identifier — only the thunk's own shape differs (exp075, verified against
 * esbuild 0.27.2 output).
 */
function isEsmHelper(d: t.VariableDeclarator): boolean {
  if (d.id.type !== "Identifier" || !d.init) return false;
  const thunk = thunkOf(d.init);
  if (!thunk) return false;
  const seq = thunkResultSequence(thunk);
  if (!seq) return false;
  const last = seq.expressions[seq.expressions.length - 1];
  return last.type === "Identifier";
}

/** The `(…, ident)` a helper thunk yields, whichever form it takes. */
function thunkResultSequence(
  thunk: t.ArrowFunctionExpression | t.FunctionExpression
): t.SequenceExpression | null {
  if (thunk.body.type === "SequenceExpression") return thunk.body;
  if (thunk.body.type !== "BlockStatement") return null;
  const ret = thunk.body.body.find((s) => s.type === "ReturnStatement");
  const arg = ret?.type === "ReturnStatement" ? ret.argument : null;
  return arg?.type === "SequenceExpression" ? arg : null;
}

/** What a top-level statement DECLARES — the names the placement trail is
 * searched by, and the per-segment `declared` export list. */
export function declaredNames(stmt: t.Statement): string[] {
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
  /** esbuild only: the original source path, from the object key. */
  sourcePath?: string;
}

/**
 * The init function inside an `__esm(...)` argument, in either observed
 * form. bun passes the function directly; esbuild passes an OBJECT with a
 * single keyed method whose KEY IS THE ORIGINAL SOURCE PATH — ground truth
 * the minified build discards (exp072/exp075).
 */
function initFunctionOf(arg: t.Node | undefined): {
  fn: t.ArrowFunctionExpression | t.FunctionExpression;
  sourcePath?: string;
} | null {
  if (!arg) return null;
  if (
    arg.type === "ArrowFunctionExpression" ||
    arg.type === "FunctionExpression"
  ) {
    return { fn: arg };
  }
  if (arg.type !== "ObjectExpression" || arg.properties.length !== 1) {
    return null;
  }
  const prop = arg.properties[0];
  const key =
    prop.type === "ObjectMethod" || prop.type === "ObjectProperty"
      ? prop.key
      : null;
  const sourcePath =
    key?.type === "StringLiteral"
      ? key.value
      : key?.type === "Identifier"
        ? key.name
        : undefined;
  if (prop.type === "ObjectMethod") {
    return {
      fn: {
        ...prop,
        type: "FunctionExpression",
        id: null
      } as unknown as t.FunctionExpression,
      sourcePath
    };
  }
  if (
    prop.type === "ObjectProperty" &&
    (prop.value.type === "ArrowFunctionExpression" ||
      prop.value.type === "FunctionExpression")
  ) {
    return { fn: prop.value, sourcePath };
  }
  return null;
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

/**
 * True when a module's init body consists ONLY of zero-arg init calls —
 * the barrel-file fossil (a source file that just re-exports others
 * compiles to an init that forces its children and wires exports,
 * declaring nothing of its own). Part of the grammar, so it lives here.
 */
export function initBodyIsOnlyInitCalls(stmt: t.Statement): boolean {
  if (stmt.type !== "VariableDeclaration") return false;
  for (const d of stmt.declarations) {
    if (!d.init || d.init.type !== "CallExpression") continue;
    const fn = d.init.arguments[0];
    if (
      !fn ||
      (fn.type !== "ArrowFunctionExpression" &&
        fn.type !== "FunctionExpression") ||
      fn.body.type !== "BlockStatement"
    ) {
      continue;
    }
    const stmts = fn.body.body;
    if (stmts.length === 0) return false;
    return stmts.every(
      (s) =>
        s.type === "ExpressionStatement" &&
        s.expression.type === "CallExpression" &&
        s.expression.arguments.length === 0 &&
        s.expression.callee.type === "Identifier"
    );
  }
  return false;
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
      const init = initFunctionOf(d.init.arguments[0]);
      if (!init) continue;
      raw.push({
        index: i,
        name: d.id.name,
        leading: leadingInitCalls(init.fn),
        sourcePath: init.sourcePath
      });
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
    declared,
    sourcePath: r.sourcePath
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
