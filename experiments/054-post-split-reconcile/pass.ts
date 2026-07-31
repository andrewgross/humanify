/**
 * 054 — the post-split reconcile pass, simulated over two trees on disk.
 *
 * Shared by `ceiling.ts` (task 0, the number) and `classify.ts` (task 1, what
 * the number is made of), so the two can never drift apart.
 *
 * What it does, per file present at the SAME PATH in both trees:
 *
 *   1. `diff prior fresh`, then the REAL `reconcileDiffNoise` in production
 *      options (copied from `reconcile-step.ts`). Rule 4: no proxy is written
 *      for a gate that is one import away.
 *   2. Apply, check the pure-rename invariant, and rewrite the fresh TEXT at
 *      the identifier positions whose name changed — a validated rename only
 *      rewrites identifier tokens, so this is what a re-emit would contain
 *      without re-running the split emitter (whose formatting must not move).
 *   3. Re-diff with the SAME instrument and report `before - after`.
 *      The result is capped by what a line diff prints BY CONSTRUCTION: it is
 *      a real diff of two real texts, not a decomposition that can over-charge.
 *   4. Re-parse the rewritten text and require the ORIGINAL fresh file's
 *      structural signature. A botched substitution that deleted a token would
 *      otherwise manufacture a saving.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { parseSourceAst, traverse } from "../../src/babel-utils.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant
} from "../../src/output-validation.js";
import {
  collectWordTokens,
  computeNormalDiff,
  type ReconcileRename,
  reconcileDiffNoise
} from "../../src/rename/diff-reconcile.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

export const IS_ELIGIBLE = createIsEligible(undefined, undefined);
const IDENT_AT = /^[A-Za-z_$][\w$]*/;

export function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/** Lines a normal-format diff prints (`<` deletions plus `>` additions). */
export function diffLineCount(diffText: string): number {
  let n = 0;
  for (const line of diffText.split("\n")) {
    if (line.startsWith("< ") || line.startsWith("> ")) n++;
    else if (line === "<" || line === ">") n++;
  }
  return n;
}

export interface Substitution {
  /** 1-based line in the fresh text. */
  line: number;
  /** 0-based column in the fresh text. */
  col: number;
  from: string;
  to: string;
}

/**
 * Identifier positions whose name in the mutated AST differs from the token
 * standing at that loc in the original text — exactly what the rename rewrote.
 * Reading the ORIGINAL token out of the text means a position is substituted
 * only when the text really holds the old name there.
 */
function collectSubstitutions(ast: t.File, lines: string[]): Substitution[] {
  const subs: Substitution[] = [];
  traverse(ast, {
    Identifier(p: NodePath<t.Identifier>) {
      const loc = p.node.loc;
      if (!loc) return;
      const text = lines[loc.start.line - 1];
      if (text === undefined) return;
      const m = IDENT_AT.exec(text.slice(loc.start.column));
      if (!m || m[0] === p.node.name) return;
      subs.push({
        line: loc.start.line,
        col: loc.start.column,
        from: m[0],
        to: p.node.name
      });
    }
  });
  return subs;
}

export function applySubstitutions(
  lines: string[],
  subs: Substitution[]
): string {
  const byLine = new Map<number, Substitution[]>();
  for (const s of subs) {
    const l = byLine.get(s.line) ?? [];
    l.push(s);
    byLine.set(s.line, l);
  }
  const out = lines.slice();
  for (const [lineNo, list] of byLine) {
    list.sort((a, b) => b.col - a.col);
    let text = out[lineNo - 1];
    for (const s of list) {
      text = text.slice(0, s.col) + s.to + text.slice(s.col + s.from.length);
    }
    out[lineNo - 1] = text;
  }
  return out.join("\n");
}

export type FileStatus =
  | "ok"
  | "identical"
  | "diff-failed"
  | "parse-failed"
  | "invariant-violated"
  | "rewrite-unsound"
  | "no-renames"
  | "corpus-gate";

export interface FileResult {
  file: string;
  status: FileStatus;
  /** Diff lines before the pass. */
  before: number;
  /** Diff lines after applying every rename (== before unless status "ok"). */
  after: number;
  renames: ReconcileRename[];
  /** Skip reasons, for the gate funnel. */
  skipped: string[];
  hunks: {
    changed: number;
    noise: number;
    genuine: number;
    oversized: number;
    tainted: number;
  };
  /** Set only when status is "ok" — lets a caller re-price subsets. */
  subs?: Substitution[];
  freshLines?: string[];
  priorText?: string;
  /** Declaration source line (fresh side) per rename, for classification. */
  declText?: string[];
}

function emptyResult(
  file: string,
  status: FileStatus,
  before: number
): FileResult {
  return {
    file,
    status,
    before,
    after: before,
    renames: [],
    skipped: [],
    hunks: { changed: 0, noise: 0, genuine: 0, oversized: 0, tainted: 0 }
  };
}

export function runFile(
  priorRoot: string,
  freshRoot: string,
  file: string,
  keepDetail: boolean
): FileResult {
  const priorText = fs.readFileSync(path.join(priorRoot, file), "utf8");
  const freshText = fs.readFileSync(path.join(freshRoot, file), "utf8");
  if (priorText === freshText) return emptyResult(file, "identical", 0);

  let diffText: string;
  try {
    diffText = computeNormalDiff(priorText, freshText);
  } catch {
    return emptyResult(file, "diff-failed", 0);
  }
  const before = diffLineCount(diffText);
  const ast = parseSourceAst(freshText, { filename: file });
  if (!ast) return emptyResult(file, "parse-failed", before);

  const baseline = captureSemanticBaseline(ast);
  const result = reconcileDiffNoise(ast, diffText, {
    apply: true,
    descriptiveTier: true,
    // Ablation switch for task 1: every cross-module misfire found by reading
    // the survivors came in through the consumer tier, on a require alias
    // whose module MOVED. `NO_CONSUMER=1` prices the pass without it.
    consumerTier: process.env.NO_CONSUMER !== "1",
    priorNames: collectWordTokens(priorText),
    isEligible: IS_ELIGIBLE,
    priorLineCount: priorText.split("\n").length
  });
  const base = emptyResult(file, "ok", before);
  base.skipped = result.skipped.map((s) => s.reason);
  base.hunks = result.hunks;
  if (result.priorTooDissimilar) return { ...base, status: "corpus-gate" };
  if (result.renames.length === 0) return { ...base, status: "no-renames" };
  base.renames = result.renames;
  if (checkStructuralInvariant(ast, baseline)) {
    return { ...base, status: "invariant-violated" };
  }
  const lines = freshText.split("\n");
  let subs = collectSubstitutions(ast, lines);
  // Ablation for the proposed gate: the consumer tier fires precisely when the
  // DECLARATION genuinely changed, and for an import binding a genuinely
  // changed declaration means a DIFFERENT MODULE — consumer testimony cannot
  // tell "renamed alias" from "different module exporting the same member".
  // Every cross-module misfire task 1 found is exactly this shape.
  // `NO_REQUIRE_DECL=1` is the stronger form actually chosen for the pass: no
  // tier may rename an import binding at all, which also protects the
  // one-alias-per-module-tree-wide property the emitter maintains.
  const banRequire =
    process.env.NO_CONSUMER_REQUIRE === "1" ||
    process.env.NO_REQUIRE_DECL === "1";
  if (banRequire) {
    const consumerOnly = process.env.NO_REQUIRE_DECL !== "1";
    const banned = new Set(
      result.renames
        .filter(
          (r) =>
            (!consumerOnly || r.kind === "consumer") &&
            /(?:const|let|var)\s+[A-Za-z_$][\w$]*\s*=\s*require\(/.test(
              lines[r.declLine - 1] ?? ""
            )
        )
        .map((r) => `${r.fromName} ${r.toName}`)
    );
    subs = subs.filter((s) => !banned.has(`${s.from} ${s.to}`));
  }
  const rewritten = applySubstitutions(lines, subs);
  const reparsed = parseSourceAst(rewritten, { filename: file });
  if (!reparsed || checkStructuralInvariant(reparsed, baseline)) {
    return { ...base, status: "rewrite-unsound" };
  }
  base.after = diffLineCount(computeNormalDiff(priorText, rewritten));
  if (keepDetail) {
    base.subs = subs;
    base.freshLines = lines;
    base.priorText = priorText;
    base.declText = result.renames.map((r) => lines[r.declLine - 1] ?? "");
  }
  return base;
}

/** Re-price a subset of a file's renames: what would `keep` alone save? */
export function priceSubset(
  res: FileResult,
  keep: (sub: Substitution) => boolean
): number {
  if (!res.subs || !res.freshLines || res.priorText === undefined) return 0;
  const rewritten = applySubstitutions(res.freshLines, res.subs.filter(keep));
  return (
    res.before - diffLineCount(computeNormalDiff(res.priorText, rewritten))
  );
}
