/**
 * exp094 census, part 2 — the FUNCTION-level arrow<->function population the
 * pipeline's match key would see, over the frozen walk trees
 * (/work/walk-rust-0926).
 *
 * For every ArrowFunctionExpression / FunctionExpression node in every file of
 * every walked version, this records:
 *
 *   - `rawHash`: statementHash of the node AS WRITTEN (the family of hashes
 *     that today includes the node's TYPE, so an arrow and a function
 *     expression never compare equal);
 *   - `cousinHash`: statementHash of the same node RE-SPELLED as the canonical
 *     `function (params) { ... }` head (no safety gate — the spelling view,
 *     so a pair that differs only in spelling shares it);
 *   - the safety verdict under the exp094 equivalence rule, with the FIRST
 *     failing condition as the reason (own-scope `this`, own-scope
 *     `arguments`, `new.target`, a named `id`, `generator`, a concise arrow
 *     body, or `new` applied to the function value);
 *   - a coarse position (call argument with its receiver, variable
 *     initializer, property value, other).
 *
 * Then, per hop, pairs functions across the two versions of each common file
 * by `cousinHash`:
 *
 *   - a COUSIN PAIR is a pair whose forms differ (arrow on one side, function
 *     expression on the other) whose bodies hash equal once the head is
 *     normalized — i.e. the same function re-spelled, or a semantic edit
 *     wearing a spelling change;
 *   - each pair is classified BOTH-SAFE (the exp094 rule would unify it) or
 *     with the refusal reason that keeps it apart — the reliability question
 *     is exactly whether anything REAL sits in the refused set.
 *
 * Incremental: one JSON per hop on disk, skipped when present.
 *
 * Usage:
 *   npx tsx function-census.ts <trees-dir> <out-dir> [--surface src|vendor|both]
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { statementHash } from "../lib/js/statement-hash.js";

interface FnRecord {
  form: "arrow" | "function";
  rawHash: string;
  cousinHash: string | null;
  safe: boolean;
  reason: string | null;
  position: string;
  lines: number;
}

interface FileCensus {
  functions: FnRecord[];
  parseError?: string;
}

// --- the safety rule ---------------------------------------------------------

/** Node types whose BODIES run under their own `this`/`arguments` (or, for
 * class fields and static blocks, the instance's). An occurrence behind one
 * of these cannot observe the flipped function's binding, so the flip is
 * semantics-preserving for it. Arrow functions are deliberately absent: they
 * pass both through.
 *
 * exp037's `LEXICAL_BINDERS` (diff-composition.ts) is the same list minus
 * ClassProperty (a class field initializer's `this` is the instance's, not
 * the wrapper's — the one DELIBERATE widening of exp037's rule recorded in
 * the exp094 README; the census's first revision keyed it on oxc's ESTree
 * name `PropertyDefinition` by mistake, so it ran under the stricter
 * reading where field initializers counted as wrapper scope — the flip
 * population was safe under BOTH readings). babel's own type name for
 * class fields is `ClassProperty`. */
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

/** Fields of a Class* node that evaluate in the OUTER scope: the extends
 * clause and computed keys (a `class C extends this.Base {}` inside a flipped
 * wrapper observes the wrapper's `this` — unlike the class body). */
function outerScopeFields(node: t.Node): Set<string> {
  switch (node.type) {
    case "ClassDeclaration":
    case "ClassExpression":
      return new Set(["superClass"]);
    case "ClassMethod":
    case "ClassPrivateMethod":
    case "ClassProperty":
      return node.computed ? new Set(["key"]) : new Set();
    default:
      return new Set();
  }
}

/** The first unifiability condition the function FAILS, or null when the
 * arrow<->function spelling flip is semantics-preserving for it.
 *
 * Conditions: no `generator`, no binding `id`, a block body (a concise arrow
 * cannot be re-spelled without restructuring), no own-scope `this`,
 * `arguments`, or `new.target`, and no `new` applied to the function value.
 * `async` is NOT a condition — both spellings carry it and it does not
 * interact with `this`/`arguments`. */
function unsafeReason(fn: t.Function, isNewCallee: boolean): string | null {
  if (isNewCallee) return "newCallee";
  if (t.isFunctionExpression(fn)) {
    if (fn.generator) return "generator";
    if (fn.id) return "namedId";
  }
  if (t.isArrowFunctionExpression(fn) && !t.isBlockStatement(fn.body)) {
    return "conciseBody";
  }
  if (ownsLexicalBindingUse(fn)) {
    return "lexicalBindingUse";
  }
  if (usesNewTarget(fn)) return "newTarget";
  return null;
}

/** Own-scope `this`/`arguments` behind no lexical binder (exp037's rule,
 * extended: class field initializers are binders, extends/computed keys are
 * not). Iterative for the multi-thousand-line wrapper bodies. */
function ownsLexicalBindingUse(fn: t.Function): boolean {
  const stack: Array<{ node: t.Node; barrier: boolean }> = [];
  const push = (node: t.Node, barrier: boolean) => {
    stack.push({ node, barrier });
  };
  // The function's own params evaluate under its own binding, so everything
  // except its id starts in its own scope.
  for (const key of ["params", "body"] as const) {
    const v = (fn as unknown as Record<string, unknown>)[key];
    for (const c of Array.isArray(v) ? v : [v]) {
      if (c && typeof c === "object") {
        push(c as t.Node, false);
      }
    }
  }
  while (stack.length > 0) {
    const { node, barrier } = stack.pop() as {
      node: t.Node;
      barrier: boolean;
    };
    if (!barrier) {
      if (node.type === "ThisExpression") return true;
      if (node.type === "Identifier" && node.name === "arguments") {
        return true;
      }
    }
    const outer = outerScopeFields(node);
    const keys = t.VISITOR_KEYS[node.type] ?? [];
    for (const k of keys) {
      const child = (node as unknown as Record<string, unknown>)[k];
      // A member's non-computed property key is not a reference either.
      if (
        k === "property" &&
        (node.type === "MemberExpression" ||
          node.type === "OptionalMemberExpression") &&
        !(node as t.MemberExpression).computed
      ) {
        continue;
      }
      // A non-computed, non-shorthand object key (`{arguments: 1}`) is a
      // property NAME, not a reference; shorthand (`{arguments}`) IS one.
      if (
        k === "key" &&
        node.type === "ObjectProperty" &&
        !(node as t.ObjectProperty).computed &&
        !(node as t.ObjectProperty).shorthand
      ) {
        continue;
      }
      const barrierOf = (c: t.Node) =>
        LEXICAL_BINDERS.has(c.type) && !outer.has(k)
          ? true
          : outer.has(k)
            ? false
            : barrier;
      const pushChild = (c: unknown) => {
        if (Array.isArray(c)) {
          for (const cc of c) pushChild(cc);
        } else if (
          typeof c === "object" &&
          c !== null &&
          typeof (c as { type?: unknown }).type === "string"
        ) {
          push(c as t.Node, barrierOf(c as t.Node));
        }
      };
      pushChild(child);
    }
  }
  return false;
}

/** `new.target` in the flipped function's own scope (its `this`-family
 * binding; `import.meta` evaluates the same in both spellings and does not
 * refuse). */
function usesNewTarget(fn: t.Function): boolean {
  const stack: t.Node[] = [];
  const push = (c: unknown) => {
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
  const pushAll = (node: t.Node) => {
    const keys = t.VISITOR_KEYS[node.type] ?? [];
    for (const k of keys) {
      push((node as unknown as Record<string, unknown>)[k]);
    }
  };
  pushAll(fn);
  while (stack.length > 0) {
    const node = stack.pop() as t.Node;
    if (
      node.type === "MetaProperty" &&
      (node as t.MetaProperty).meta.name === "new"
    ) {
      return true;
    }
    pushAll(node);
  }
  return false;
}

// --- the walk ----------------------------------------------------------------

/** The canonical re-spelling of a function NODE (no safety gate): the
 * `function (params) { body }` view both spellings share. Null when the node
 * cannot be re-spelled (a concise arrow body). */
function cousinNode(fn: t.Function): t.FunctionExpression | null {
  if (!t.isBlockStatement(fn.body)) return null;
  const params = fn.params as t.FunctionParameter[];
  return t.functionExpression(null, params, fn.body, false, fn.async);
}

/** Text of a callee, for the receiver record (best-effort, names only). */
function calleeText(node: t.Node | undefined): string {
  if (!node) return "";
  if (t.isIdentifier(node)) return node.name;
  if (t.isMemberExpression(node)) {
    return `${calleeText(node.object)}.${calleeText(node.property)}`;
  }
  if (t.isSequenceExpression(node)) {
    const last = node.expressions[node.expressions.length - 1];
    return calleeText(last);
  }
  return node.type;
}

function censusFile(code: string): FileCensus {
  let ast: ReturnType<typeof parseSync>;
  try {
    ast = parseSync(code, { sourceType: "unambiguous" });
  } catch (e) {
    return {
      functions: [],
      parseError: e instanceof Error ? e.message.split("\n")[0] : String(e)
    };
  }
  const out: FnRecord[] = [];
  const stack: Array<{ node: t.Node; parent: t.Node | null }> = ast
    ? [{ node: ast.program, parent: null }]
    : [];
  while (stack.length > 0) {
    const { node, parent } = stack.pop() as {
      node: t.Node;
      parent: t.Node | null;
    };
    let isNewCallee = false;
    let position = "other";
    if (parent) {
      if (
        t.isCallExpression(parent) ||
        t.isOptionalCallExpression(parent) ||
        t.isNewExpression(parent)
      ) {
        position = "argument";
      } else if (t.isNewExpression(parent)) {
        isNewCallee = true;
        position = "newCallee";
      } else if (t.isVariableDeclarator(parent)) {
        position = "varInit";
      } else if (t.isProperty(parent) || t.isObjectProperty(parent)) {
        position = "propertyValue";
      } else if (t.isReturnStatement(parent)) {
        position = "return";
      } else if (t.isAssignmentExpression(parent)) {
        position = "assignment";
      }
    }
    if (
      (t.isArrowFunctionExpression(node) || t.isFunctionExpression(node)) &&
      node.start != null &&
      node.end != null
    ) {
      const reason = unsafeReason(node, isNewCallee);
      const cousin = cousinNode(node);
      out.push({
        form: t.isArrowFunctionExpression(node) ? "arrow" : "function",
        rawHash: statementHash(node as unknown as t.Statement),
        cousinHash: cousin
          ? statementHash(cousin as unknown as t.Statement)
          : null,
        safe: reason === null,
        reason,
        position:
          position === "argument" &&
          parent &&
          (t.isCallExpression(parent) || t.isOptionalCallExpression(parent))
            ? `argument:${calleeText(parent.callee)}`
            : position,
        lines: code.slice(node.start, node.end).split("\n").length
      });
    }
    const keys = t.VISITOR_KEYS[node.type] ?? [];
    for (const k of keys) {
      const child = (node as unknown as Record<string, unknown>)[k];
      const push = (c: unknown) => {
        if (Array.isArray(c)) {
          for (const cc of c) push(cc);
        } else if (
          typeof c === "object" &&
          c !== null &&
          typeof (c as { type?: unknown }).type === "string"
        ) {
          stack.push({ node: c as t.Node, parent: node });
        }
      };
      push(child);
    }
  }
  return { functions: out };
}

// --- the sweep ---------------------------------------------------------------

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

function inventoryOf(versionDir: string): Map<string, FileCensus> {
  const out = new Map<string, FileCensus>();
  for (const f of walk(versionDir)) {
    out.set(f, censusFile(fs.readFileSync(path.join(versionDir, f), "utf8")));
  }
  return out;
}

interface Flags {
  treesDir: string;
  outDir: string;
  surfaces: Array<"src" | "vendor">;
}

function parseFlags(argv: string[]): Flags {
  const flags: Flags = {
    treesDir: "",
    outDir: "",
    surfaces: ["src", "vendor"]
  };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === "--surface") {
      const v = argv[++i];
      if (v !== "src" && v !== "vendor" && v !== "both") {
        throw new Error(`--surface must be src|vendor|both, got ${v}`);
      }
      flags.surfaces = v === "both" ? ["src", "vendor"] : [v];
    } else if (!flags.treesDir) {
      flags.treesDir = path.resolve(a);
    } else if (!flags.outDir) {
      flags.outDir = path.resolve(a);
    } else {
      throw new Error(`unknown argument: ${a}`);
    }
  }
  if (!flags.treesDir || !flags.outDir) {
    throw new Error(
      "usage: function-census.ts <trees-dir> <out-dir> [--surface ...]"
    );
  }
  return flags;
}

function main(): void {
  const flags = parseFlags(process.argv.slice(2));
  fs.mkdirSync(flags.outDir, { recursive: true });
  const versions = fs
    .readdirSync(flags.treesDir)
    .filter((v) => v.startsWith("2.1."))
    .sort();
  console.log(`function census over ${flags.treesDir} -> ${flags.outDir}`);
  // Rolling inventory: parse each version once per surface.
  let previous: Map<string, Map<string, FileCensus>> = new Map();
  for (let i = 0; i < versions.length; i += 1) {
    const version = versions[i];
    const current = new Map<string, Map<string, FileCensus>>();
    for (const surface of flags.surfaces) {
      current.set(
        surface,
        inventoryOf(path.join(flags.treesDir, version, surface))
      );
    }
    if (i > 0) {
      const from = versions[i - 1];
      const outFile = path.join(flags.outDir, `${from}--${version}.json`);
      if (!fs.existsSync(outFile)) {
        const record: Record<string, unknown> = { hop: `${from}->${version}` };
        for (const surface of flags.surfaces) {
          record[surface] = pairSurface(
            previous.get(surface) ?? new Map(),
            current.get(surface) ?? new Map()
          );
        }
        fs.writeFileSync(outFile, `${JSON.stringify(record, null, 2)}\n`);
      }
      console.log(`  ${from}->${version}: paired`);
    }
    previous = current;
  }
}

interface CousinPair {
  file: string;
  priorForm: string;
  freshForm: string;
  lines: number;
  priorReason: string | null;
  freshReason: string | null;
  position: string;
}

/** Pair the two inventories of one surface by cousinHash, and classify. */
function pairSurface(
  prior: Map<string, FileCensus>,
  fresh: Map<string, FileCensus>
): Record<string, unknown> {
  const pairs: CousinPair[] = [];
  const population = {
    priorFunctions: 0,
    freshFunctions: 0,
    priorRefusals: {} as Record<string, number>,
    freshRefusals: {} as Record<string, number>
  };
  let parseErrors = 0;
  for (const [file, freshCensus] of fresh) {
    const priorCensus = prior.get(file);
    if (!priorCensus) continue;
    if (priorCensus.parseError || freshCensus.parseError) {
      parseErrors += 1;
      continue;
    }
    const buckets = new Map<string, FnRecord[]>();
    for (const fn of priorCensus.functions) {
      population.priorFunctions += 1;
      if (fn.reason) {
        population.priorRefusals[fn.reason] =
          (population.priorRefusals[fn.reason] ?? 0) + 1;
      }
      if (fn.cousinHash) {
        const l = buckets.get(fn.cousinHash) ?? [];
        l.push(fn);
        buckets.set(fn.cousinHash, l);
      }
    }
    for (const fn of freshCensus.functions) {
      population.freshFunctions += 1;
      if (fn.reason) {
        population.freshRefusals[fn.reason] =
          (population.freshRefusals[fn.reason] ?? 0) + 1;
      }
    }
    for (const fn of freshCensus.functions) {
      if (!fn.cousinHash) continue;
      const bucket = buckets.get(fn.cousinHash);
      if (!bucket) continue;
      const j = bucket.findIndex((b) => b.form !== fn.form);
      if (j < 0) continue; // same spelling on both sides: not a flip
      const twin = bucket.splice(j, 1)[0];
      pairs.push({
        file,
        priorForm: twin.form,
        freshForm: fn.form,
        lines: Math.max(twin.lines, fn.lines),
        priorReason: twin.reason,
        freshReason: fn.reason,
        position: twin.position
      });
    }
  }
  const bothSafe = pairs.filter((p) => !p.priorReason && !p.freshReason);
  const refused = pairs.filter((p) => p.priorReason || p.freshReason);
  const refusedByReason: Record<string, number> = {};
  for (const p of refused) {
    for (const r of [p.priorReason, p.freshReason]) {
      if (r) refusedByReason[r] = (refusedByReason[r] ?? 0) + 1;
    }
  }
  return {
    parseErrors,
    population,
    cousinPairs: pairs.length,
    bothSafePairs: bothSafe.length,
    bothSafeLines: bothSafe.reduce((a, p) => a + p.lines, 0),
    refusedPairs: refused.length,
    refusedByReason,
    pairs: pairs.map((p) => ({
      file: p.file,
      lines: p.lines,
      forms: `${p.priorForm}->${p.freshForm}`,
      safe: !p.priorReason && !p.freshReason,
      reason: p.priorReason ?? p.freshReason,
      position: p.position
    }))
  };
}

main();
