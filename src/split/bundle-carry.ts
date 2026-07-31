/**
 * Carry the post-split reconcile's renames into the bundle (exp054).
 *
 * `.humanify/humanified.js` is what the NEXT release points `--prior-version`
 * at: it is the lineage. The post-split pass renames bindings in split FILES,
 * so without this the tree and the bundle disagree by exactly those renames,
 * and every hop of a forward walk has to re-earn the restoration from the prior
 * tree instead of inheriting it through the ordinary matcher.
 *
 * ## Finding the binding
 *
 * The obvious route does not work: the runnable emit rewrites cross-file
 * references (`f(x)` becomes `ns.f(x)`), so an emitted statement is not
 * structurally the bundle statement it came from and cannot be matched by hash.
 * Name alone does not work either — measured on 215→216, 27% of renames share
 * their `fromName` with another binding in the same file, and the trail contains
 * `retryAttemptCount -> reactiveCompactResponse` AND
 * `retryAttemptCount -> reactiveCompactResponseSecondary` in one file, so a
 * name-keyed carry would put one of them on the wrong binding.
 *
 * What is exact is the ledger's `emitIndexes`: the bundle statement index behind
 * each emitted slot. A rename located at the file's j-th ledger statement maps
 * to bundle statement `emitIndexes[slot_j]`, and within that statement the
 * `nameOrdinal`-th declaration of the old name is the binding. The emit never
 * adds or removes a binding, so that sequence agrees on both sides.
 *
 * Everything abstains rather than guesses: a rename with no locator, a ledger
 * without `emitIndexes`, a statement whose declarations do not line up. An
 * abstention leaves the bundle naming that binding as before — exactly the
 * status quo — and is counted so it can be seen.
 */
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { parseSourceAst, traverse } from "../babel-utils.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant
} from "../output-validation.js";
import type { Binding } from "@babel/traverse";
import { violationWriteTargetPaths } from "../babel-utils.js";
import { attemptValidatedRename } from "../rename/validated-rename.js";
import type { PostSplitRename } from "./post-split-reconcile.js";
import type { StableSplitLedger } from "./stable-split.js";

export interface BundleCarryResult {
  /** The rewritten bundle, or undefined when nothing was carried. */
  code?: string;
  carried: number;
  /** Renames the bundle could not be given, with a reason each. */
  abstained: Map<string, number>;
}

const IDENT_AT = /^[A-Za-z_$][\w$]*/;

/** Which slot of the ledger holds each file's j-th emitted statement. */
function slotsByFile(ledger: StableSplitLedger): Map<string, number[]> {
  const byFile = new Map<string, number[]>();
  for (let slot = 0; slot < ledger.order.length; slot++) {
    const list = byFile.get(ledger.order[slot]) ?? [];
    list.push(slot);
    byFile.set(ledger.order[slot], list);
  }
  return byFile;
}

/**
 * The wrapper body — the statement list the ledger indexes. A Bun bundle is one
 * wrapper call; the split assigns files to the statements of its body.
 */
function wrapperBody(ast: t.File, expected: number): t.Statement[] | undefined {
  let found: t.Statement[] | undefined;
  traverse(ast, {
    Function(p: NodePath<t.Function>) {
      if (found) return;
      const body = p.node.body;
      if (body.type !== "BlockStatement") return;
      if (body.body.length !== expected) return;
      found = body.body;
      p.stop();
    }
  });
  // No fallback to `program.body`: on a one-statement ledger it would match the
  // whole IIFE and every binding inside it would look like a candidate. The
  // split only runs on wrapper bundles, so failing to find the body is a reason
  // to abstain, not to guess at a different statement list.
  return found;
}

interface Target {
  rename: PostSplitRename;
  stmt: t.Statement;
}

/** Pair each locatable rename with the bundle statement it belongs to. */
function resolveTargets(
  renames: PostSplitRename[],
  ledger: StableSplitLedger,
  body: t.Statement[],
  abstained: Map<string, number>
): Target[] {
  const bump = (reason: string) =>
    abstained.set(reason, (abstained.get(reason) ?? 0) + 1);
  const slots = slotsByFile(ledger);
  const indexes = ledger.emitIndexes;
  const targets: Target[] = [];
  for (const rename of renames) {
    // TOP-LEVEL renames are never carried. A split file exports through
    // `defineProperty(module.exports, "name", { get: () => local })`, whose key
    // is a STRING the tree's rename cannot reach — so in the tree the
    // declaration moves and the export key does not, and consumers keep using
    // the old name. Carry it into the bundle and the NEXT release derives the
    // export key from the bundle's new name, the key moves, and every consumer
    // churns: measured on 85->86 as 238 of 238 drifted self-hop lines, every
    // one of them an export key. Inner locals are never export keys.
    if (rename.topLevel) {
      bump("top-level-would-move-an-export-key");
      continue;
    }
    if (!rename.locator) {
      bump("no-locator");
      continue;
    }
    const fileSlots = slots.get(rename.file);
    const slot = fileSlots?.[rename.locator.bodyOrdinal];
    if (slot === undefined || !indexes) {
      bump(indexes ? "slot-out-of-range" : "ledger-has-no-emit-indexes");
      continue;
    }
    const stmt = body[indexes[slot]];
    if (!stmt) {
      bump("bundle-index-out-of-range");
      continue;
    }
    targets.push({ rename, stmt });
  }
  return targets;
}

/**
 * The declaration this rename refers to: the `nameOrdinal`-th binding named
 * `fromName` declared inside `stmt`, in source order.
 */
function findBinding(
  decls: Array<{
    stmt: t.Statement;
    name: string;
    start: number;
    path: NodePath;
  }>,
  target: Target
): NodePath | undefined {
  const matches = decls
    .filter((d) => d.stmt === target.stmt && d.name === target.rename.fromName)
    .sort((a, b) => a.start - b.start);
  return matches[target.rename.locator?.nameOrdinal ?? -1]?.path;
}

/** Every binding declaration in the bundle, bucketed by wrapper statement. */
function bundleDeclarations(
  ast: t.File,
  body: t.Statement[]
): Array<{ stmt: t.Statement; name: string; start: number; path: NodePath }> {
  const spans = body.map((s) => [s.start ?? -1, s.end ?? -1] as const);
  const statementOf = (pos: number): number => {
    let lo = 0;
    let hi = spans.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (pos < spans[mid][0]) hi = mid - 1;
      else if (pos > spans[mid][1]) lo = mid + 1;
      else return mid;
    }
    return -1;
  };
  const out: Array<{
    stmt: t.Statement;
    name: string;
    start: number;
    path: NodePath;
  }> = [];
  traverse(ast, {
    Scopable(p: NodePath) {
      for (const [name, binding] of Object.entries(p.scope.bindings)) {
        if (binding.scope.block !== p.node) continue;
        const start = binding.identifier.start;
        if (start == null) continue;
        const idx = statementOf(start);
        if (idx >= 0) out.push({ stmt: body[idx], name, start, path: p });
      }
    }
  });
  return out;
}

interface Substitution {
  line: number;
  col: number;
  from: string;
  to: string;
}

/**
 * Every position a rename of `binding` rewrites: its declaration, each
 * reference, and each write target inside a constant violation. Captured BEFORE
 * the rename, so the positions are read off the binding's own bookkeeping
 * rather than inferred from a mutated tree by matching names — which cannot
 * tell a renamed occurrence from an unrelated identifier that already had the
 * new name (there are 4,223 bindings spelled `error` in one bundle).
 *
 * Returns null when any position is missing a loc or the text there does not
 * actually hold the old name; the caller then abstains for this binding rather
 * than writing a substitution it cannot justify.
 */
function occurrencesOf(
  binding: Binding,
  fromName: string,
  toName: string,
  lines: string[]
): Substitution[] | null {
  const nodes: t.Node[] = [binding.identifier];
  for (const ref of binding.referencePaths) nodes.push(ref.node);
  for (const violation of binding.constantViolations) {
    for (const p of violationWriteTargetPaths(violation, fromName)) {
      nodes.push(p.node);
    }
  }
  const subs: Substitution[] = [];
  for (const node of nodes) {
    const loc = node.loc;
    if (!loc) return null;
    const text = lines[loc.start.line - 1];
    if (text === undefined) return null;
    const match = IDENT_AT.exec(text.slice(loc.start.column));
    if (!match || match[0] !== fromName) return null;
    subs.push({
      line: loc.start.line,
      col: loc.start.column,
      from: fromName,
      to: toName
    });
  }
  return subs;
}

function applySubstitutions(lines: string[], subs: Substitution[]): string {
  const byLine = new Map<number, Substitution[]>();
  for (const sub of subs) {
    const list = byLine.get(sub.line) ?? [];
    list.push(sub);
    byLine.set(sub.line, list);
  }
  const out = lines.slice();
  for (const [lineNo, list] of byLine) {
    list.sort((a, b) => b.col - a.col);
    let text = out[lineNo - 1];
    let previousCol = Number.POSITIVE_INFINITY;
    for (const sub of list) {
      if (sub.col >= previousCol) continue; // same position twice: skip
      text =
        text.slice(0, sub.col) + sub.to + text.slice(sub.col + sub.from.length);
      previousCol = sub.col;
    }
    out[lineNo - 1] = text;
  }
  return out.join("\n");
}

/**
 * Carry one rename, or the reason it was refused. Mutates the AST on success so
 * later renames see the freed name — a chain (`a -> b`, `b -> c`) needs the
 * blocker gone before the second can apply.
 */
function carryOne(
  target: Target,
  decls: Array<{
    stmt: t.Statement;
    name: string;
    start: number;
    path: NodePath;
  }>,
  lines: string[]
): Substitution[] | string {
  const scopePath = findBinding(decls, target);
  if (!scopePath) return "binding-not-found";
  const { fromName, toName } = target.rename;
  const binding = scopePath.scope.getBinding(fromName);
  if (!binding) return "binding-not-found";
  // Capture the positions BEFORE the rename: afterwards the binding carries the
  // new name and its old occurrences are indistinguishable from unrelated
  // identifiers that already had it (one bundle has 4,223 bindings spelled
  // `error`).
  const occurrences = occurrencesOf(binding, fromName, toName, lines);
  if (!occurrences) return "occurrence-not-in-text";
  const attempt = attemptValidatedRename(scopePath.scope, fromName, toName);
  if (!attempt.applied) return `rename-rejected:${attempt.reason}`;
  return occurrences;
}

/**
 * Give the bundle the names the tree shipped. Returns `code` only when at least
 * one rename was carried AND the result is provably a pure rename of the input;
 * anything else returns no code and the caller keeps the bundle it has.
 */
export function carryRenamesIntoBundle(
  bundleCode: string,
  ledger: StableSplitLedger,
  renames: PostSplitRename[]
): BundleCarryResult {
  const abstained = new Map<string, number>();
  if (renames.length === 0) return { carried: 0, abstained };

  const ast = parseSourceAst(bundleCode, { filename: "humanified.js" });
  if (!ast) {
    abstained.set("bundle-unparseable", renames.length);
    return { carried: 0, abstained };
  }
  const body = wrapperBody(ast, ledger.order.length);
  if (!body) {
    abstained.set("wrapper-body-not-found", renames.length);
    return { carried: 0, abstained };
  }

  const baseline = captureSemanticBaseline(ast);
  const targets = resolveTargets(renames, ledger, body, abstained);
  const decls = bundleDeclarations(ast, body);
  const lines = bundleCode.split("\n");
  const bump = (reason: string) =>
    abstained.set(reason, (abstained.get(reason) ?? 0) + 1);

  const subs: Substitution[] = [];
  let carried = 0;
  for (const target of targets) {
    const one = carryOne(target, decls, lines);
    if (typeof one === "string") {
      bump(one);
      continue;
    }
    subs.push(...one);
    carried++;
  }
  if (carried === 0) return { carried: 0, abstained };
  if (checkStructuralInvariant(ast, baseline)) {
    abstained.set("invariant-violated", carried);
    return { carried: 0, abstained };
  }
  const code = applySubstitutions(lines, subs);
  const reparsed = parseSourceAst(code, { filename: "humanified.js" });
  if (!reparsed || checkStructuralInvariant(reparsed, baseline)) {
    abstained.set("rewrite-unsound", carried);
    return { carried: 0, abstained };
  }
  return { code, carried, abstained };
}
