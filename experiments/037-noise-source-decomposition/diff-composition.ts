/**
 * Diff composition (exp037): decompose the REAL on-disk git diff of a split tree
 * into real change vs each noise mechanism, in GIT LINE units so the parts sum to
 * roughly the churn a human sees.
 *
 * Per common file, top-level statements are matched three ways:
 *   1. exact (hash+text) present on both sides  -> unchanged content. If it sits
 *      at a different position it still churns: REORDER (LCS over the common
 *      subsequence; anything off the LCS is displaced).
 *   2. same hash, different text -> pure NAMING churn (structure identical modulo
 *      renaming). Charged the ACTUAL differing line count (line-level LCS between
 *      the two statement texts), not whole-statement mass. Split out further:
 *      ALIAS churn when the statement is a `const x = require("...")` header line.
 *   3. hash present on only one side -> REAL change (added or removed lines).
 *      A novel statement is first offered a tier-3 "edited version" repair
 *      (masked head + >=50% token overlap -> only the edited lines charged).
 * Files present on only one side are counted whole (added/removed).
 *
 * SOFT NOISE (advisory, additive): statement pairs that fail every tier but
 * are the same code modulo WRAPPER SPELLING (arrow vs function expression —
 * a packaging-tool re-serialization) are reported in
 * `spellingIdenticalLines` while keeping their `real` charge, so the frozen
 * columns stay byte-identical and the breakdown can say how much of "real"
 * is only spelling. See `wrapperSpellingKey` for the tight rule.
 *
 * Usage: npx tsx diff-composition.ts <priorSrcDir> <freshSrcDir> [label]
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  editedLineCounts,
  maskedHead,
  tokenSet
} from "../034-eval-harness/diff-ledger.js";
import { statementHash } from "../lib/js/statement-hash.js";

export interface Stmt {
  hash: string;
  text: string;
  lines: string[];
  isRequire: boolean;
}

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/** `const x = require("...")` — the import header lines, where a changed alias
 * with an unchanged path is pure alias churn. */
function isRequireDecl(s: t.Statement): boolean {
  if (!t.isVariableDeclaration(s) || s.declarations.length !== 1) return false;
  const init = s.declarations[0].init;
  return (
    t.isCallExpression(init) &&
    t.isIdentifier(init.callee, { name: "require" }) &&
    init.arguments.length === 1 &&
    t.isStringLiteral(init.arguments[0])
  );
}

export function statementsOf(code: string): Stmt[] {
  // FATAL on a parse failure. This used to return [], which silently
  // converted the entire counterpart file into "real removed/added" lines
  // inside the lead KPI — a broken emitted file scoring as genuine change.
  let ast: ReturnType<typeof parseSync>;
  try {
    ast = parseSync(code, { sourceType: "unambiguous" });
  } catch (e) {
    throw new Error(
      `diff-composition: parse failed (${e instanceof Error ? e.message.split("\n")[0] : e})`
    );
  }
  if (!ast || ast.type !== "File") {
    throw new Error("diff-composition: parse produced no File ast");
  }
  return ast.program.body.map((s) => {
    const text =
      s.start != null && s.end != null ? code.slice(s.start, s.end) : "";
    return {
      hash: statementHash(s),
      text,
      lines: text.length ? text.split("\n") : [],
      isRequire: isRequireDecl(s)
    };
  });
}

/** Length of the longest common subsequence of two line arrays (rolling DP). */
function lcsLen(a: string[], b: string[]): number {
  if (a.length === 0 || b.length === 0) return 0;
  // Guard pathological sizes: fall back to multiset intersection (a lower bound
  // on LCS, so it slightly OVER-counts churn for giant statements).
  if (a.length * b.length > 25_000_000) {
    const counts = new Map<string, number>();
    for (const l of b) counts.set(l, (counts.get(l) ?? 0) + 1);
    let common = 0;
    for (const l of a) {
      const n = counts.get(l) ?? 0;
      if (n > 0) {
        common++;
        counts.set(l, n - 1);
      }
    }
    return common;
  }
  let prev = new Array<number>(b.length + 1).fill(0);
  let cur = new Array<number>(b.length + 1).fill(0);
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      cur[j] =
        a[i - 1] === b[j - 1] ? prev[j - 1] + 1 : Math.max(prev[j], cur[j - 1]);
    }
    [prev, cur] = [cur, prev];
    cur.fill(0);
  }
  return prev[b.length];
}

/** git-style churn between two texts: added + deleted lines. */
function lineChurn(a: string[], b: string[]): number {
  return a.length + b.length - 2 * lcsLen(a, b);
}

/** Indices of `fresh` on the LCS of the two key sequences (order-stable ones). */
export function onLcs(prior: string[], fresh: string[]): Set<number> {
  const n = prior.length;
  const m = fresh.length;
  if (n === 0 || m === 0 || n * m > 25_000_000) {
    return new Set(fresh.map((_, i) => i)); // give up: treat as in-order
  }
  const dp: number[][] = Array.from({ length: n + 1 }, () =>
    new Array(m + 1).fill(0)
  );
  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      dp[i][j] =
        prior[i - 1] === fresh[j - 1]
          ? dp[i - 1][j - 1] + 1
          : Math.max(dp[i - 1][j], dp[i][j - 1]);
    }
  }
  const keep = new Set<number>();
  let i = n;
  let j = m;
  while (i > 0 && j > 0) {
    if (prior[i - 1] === fresh[j - 1]) {
      keep.add(j - 1);
      i--;
      j--;
    } else if (dp[i - 1][j] >= dp[i][j - 1]) i--;
    else j--;
  }
  return keep;
}

/** Git-line churn of a split tree diff, split by what caused each line. */
export interface Tally {
  real: number;
  naming: number;
  alias: number;
  reorder: number;
  fileAddRemove: number;
  /**
   * SOFT NOISE (advisory, 2026-09-29): lines charged to `real` above whose
   * statement pair is identical code modulo WRAPPER SPELLING — the packaging
   * tool re-serialized a wrapper from `createModule((a,b) => {...})` to
   * `createModule(function(a,b) {...})`, so the wrapper's AST TYPE flips the
   * statementHash (tier 2 cannot pair) and the keyword breaks the masked head
   * (tier 3 cannot repair), and both sides are charged full mass as real
   * change. This field reports that mass WITHOUT moving a single existing
   * charge — Andrew's call (2026-09-29): upstream changed the source, so we
   * keep replicating it, but the noise calc should be able to say how much of
   * "real" is only spelling. See `wrapperSpellingKey` for the exact rule.
   */
  spellingIdenticalLines: number;
}

/**
 * One noise instance, kept so the diff can be READ and not just totalled.
 *
 * The tallies below are the only thing the eval consumes, and a total cannot
 * show WHAT the noise is — which is the question anyone reviewing a release
 * actually asks. Collection is opt-in and never touches a tally, so scoring is
 * unaffected whether or not a sink is passed.
 */
export interface NoiseSample {
  /**
   * `real` is NOT noise — it is the change bucket, sampled so it can be
   * audited. exp054 removed 5,026 git lines of which the noise buckets
   * accounted for 450: the rest was name churn sitting inside statements whose
   * hash flipped, which this classifier charges to real change and no noise KPI
   * can see. Sampling it is how that mass gets measured instead of assumed.
   *
   * `spelling` is SOFT noise: the pair's charge stands inside `real` — the
   * sample exists so the charge can be IDENTIFIED as a wrapper-spelling
   * re-serialization (arrow vs function expression), not defended as genuine
   * change. See `wrapperSpellingKey`.
   */
  kind: "reorder" | "naming" | "alias" | "real" | "spelling";
  file: string;
  /** git lines this instance charges. */
  lines: number;
  priorText?: string;
  freshText?: string;
  /** How many prior statements shared this hash when the pair was chosen. 1 is
   * a forced pairing; >1 means the rule PICKED one, and 051 exists because that
   * pick decides whether these lines are called noise or real change. */
  candidates?: number;
  /** Token overlap of the chosen pair (`NaN` under FIFO, which never scores). */
  score?: number;
}

/** Where samples go. `cap` bounds memory on a 900k-line tree. */
export interface NoiseSink {
  file: string;
  samples: NoiseSample[];
  cap: number;
}

function keep(sink: NoiseSink | undefined, s: NoiseSample): void {
  if (sink && sink.samples.length < sink.cap) sink.samples.push(s);
}

/**
 * How step 2 picks WHICH prior statement a fresh statement is "a rename of",
 * when several prior statements share its hash.
 *
 * This matters because `statementHash` masks every identifier NAME — its own
 * docstring warns that short statements "collide across unrelated code" — so a
 * shared hash means "same shape, names blanked", not "the same statement". Where
 * a file holds many statements of one shape, the choice is real and arbitrary.
 *
 * - `fifo` (default): first available prior statement of that hash, i.e.
 *   emission order. Every number in the 033-050 arc came out of this rule, so it
 *   stays the default and is pinned by a test.
 * - `corroborated`: the candidate sharing the most identifier tokens, and only
 *   when that overlap clears `minOverlap`. A refused pair is not a rename: both
 *   sides fall through to the real-change path, where they are charged the lines
 *   a line diff would print.
 */
export type Pairing = "fifo" | "corroborated";

export interface ComposeOptions {
  pairing?: Pairing;
  /** Jaccard-ish token overlap a corroborated pair must clear. Matches the
   * diff-ledger's own edited-vs-unrelated threshold used in step 3 below. */
  minOverlap?: number;
}

const DEFAULTS: Required<ComposeOptions> = {
  pairing: "fifo",
  minOverlap: 0.5
};

/** Tokenising a 5k-line statement once per candidate comparison is the whole
 * cost of the corroborated rule; each statement is tokenised once instead. */
const tokenCache = new WeakMap<Stmt, Set<string>>();
function tokensOf(s: Stmt): Set<string> {
  let t = tokenCache.get(s);
  if (!t) {
    t = tokenSet(s.text);
    tokenCache.set(s, t);
  }
  return t;
}

/** Shared-token score of two statements — `|A n B| / max(|A|,|B|)`, the rule
 * step 3 already uses to decide "edited version of" vs "unrelated". */
function overlap(a: Set<string>, b: Set<string>): number {
  let inter = 0;
  for (const w of a) if (b.has(w)) inter++;
  return inter / Math.max(a.size, b.size, 1);
}

/** The prior statement `s` is a rename of, plus the corroboration score, or
 * `null` when no candidate corroborates. Mutates `bucket` to consume the pick. */
function takeTwin(
  bucket: Stmt[],
  s: Stmt,
  opts: Required<ComposeOptions>
): { twin: Stmt; score: number } | null {
  if (bucket.length === 0) return null;
  if (opts.pairing === "fifo") {
    return { twin: bucket.shift() as Stmt, score: Number.NaN };
  }
  const sw = tokensOf(s);
  let bestIdx = -1;
  let bestScore = 0;
  for (let i = 0; i < bucket.length; i++) {
    const score = overlap(sw, tokensOf(bucket[i]));
    if (score > bestScore) {
      bestScore = score;
      bestIdx = i;
    }
  }
  if (bestIdx < 0 || bestScore < opts.minOverlap) return null;
  return { twin: bucket.splice(bestIdx, 1)[0], score: bestScore };
}

/**
 * SOFT-NOISE DETECTOR (2026-09-29): is this statement a bundler wrapper whose
 * head spells its function argument as an ARROW or as a FUNCTION EXPRESSION?
 *
 * The known case (2.1.207→208, four files named in
 * docs/rust-port/20-overnight-report.md §2): upstream's packaging tool
 * re-serialized `createModule((a,b) => {...})` into
 * `createModule(function(a,b) {...})`. Identical code, different spelling —
 * but the function argument's AST TYPE is part of `statementHash`, so tier 2
 * sees two different hashes, and the `function` keyword breaks the masked head
 * so tier 3 cannot repair the pair either. Both sides are therefore charged
 * FULL mass as real change: the fake "+9,162 lines of real change".
 *
 * The category is DELIBERATELY TIGHT. A statement qualifies only when its
 * first line is a sequence-callee call — `var x = (0, ns.method)(` — whose
 * first argument is a function spelled one of these three ways:
 *
 *   `(a, b) => {`      parenthesized arrow
 *   `a => {`           single-identifier arrow (the other 207 form)
 *   `function (a, b) {`  function expression
 *
 * Everything else is refused, ON PURPOSE (a refusal is a false negative, never
 * a false positive):
 *
 *   - direct calls `method(function (a) {` — never observed in a walk; widen
 *     only when one is.
 *   - `async` wrappers, generators (`function*`), default/destructured
 *     parameters — the head must be exactly one of the three forms above.
 *   - a `this`/`arguments` the wrapper's OWN scope can observe: an arrow
 *     binds both lexically, so flipping its spelling CHANGES MEANING — a real
 *     edit, not spelling. Occurrences behind a nested function or class are
 *     bound there and do NOT refuse (the real giants nest whole classes that
 *     use `this`; the wrapper itself must stay clean).
 *   - any structural difference in the body: the pair is compared with
 *     `statementHash` after the head is normalized, so a changed literal,
 *     operator or node (!0 vs true included) refuses. Identifier names are
 *     masked by the hash, exactly as in every other tier.
 *   - a parse failure of the normalized text: the detector returns `null`
 *     rather than touching a tally (unlike `statementsOf`, which is FATAL on
 *     broken input — here the input file already parsed, so a failed reparse
 *     means the normalization is wrong, and refusing is safe).
 *
 * Returns the WRAPPER FORM plus the hash of the text with the head rewritten
 * to the canonical `function (params) {` spelling — two statements flagged as
 * a spelling pair iff their keys share a hash and differ in form.
 */
interface WrapperSpellingKey {
  form: "arrow" | "function";
  /** statementHash of the statement with its wrapper head normalized. */
  hash: string;
}

/** Sequence-callee call head: `var x = (0, ns.method)(`. No `;`/`{` may appear
 * before it, so only the statement's own head can match — never a wrapper
 * nested somewhere inside its body. */
const SEQUENCE_CALL_HEAD = /^([^;{]*\)\()/;
const WRAPPER_HEADS: Array<{
  re: RegExp;
  form: "arrow" | "function";
}> = [
  { re: /^function\s*\(([^()]*)\)\s*\{/, form: "function" },
  { re: /^\(([^()]*)\)\s*=>\s*\{/, form: "arrow" },
  { re: /^([A-Za-z_$][\w$]*)\s*=>\s*\{/, form: "arrow" }
];

/**
 * Node types that BIND their own `this`/`arguments` (or, for classes, run
 * their bodies under their own `this`). An occurrence behind one of these
 * cannot observe the wrapper's binding, so the wrapper's spelling flip is
 * semantics-preserving for it. Arrow functions are deliberately absent:
 * they pass both through, so an arrow nested in the wrapper still observes
 * the WRAPPER's binding.
 */
const LEXICAL_BINDERS = new Set([
  "FunctionDeclaration",
  "FunctionExpression",
  "ObjectMethod",
  "ClassMethod",
  "ClassPrivateMethod",
  "ClassDeclaration",
  "ClassExpression",
  "StaticBlock"
]);

/**
 * Does a `this` or `arguments` occurrence in the WRAPPER's own lexical scope
 * exist — one the arrow↔function flip would change the meaning of?
 *
 * The wrapper is the function expression at `offset` of the already-parsed
 * normalized statement. Iterative (explicit stack) like `statementHash`, for
 * the same reason: multi-thousand-line wrapper bodies. `x.arguments` (a
 * property, not the binding) is skipped so a member access cannot force a
 * refusal. Returns true => REFUSE the pair.
 */
function wrapperOwnsLexicalBindingUse(wrapper: t.FunctionExpression): boolean {
  const stack: Array<{ node: t.Node; barrier: boolean }> = [
    { node: wrapper, barrier: false }
  ];
  while (stack.length > 0) {
    const { node, barrier } = stack.pop() as {
      node: t.Node;
      barrier: boolean;
    };
    if (!barrier) {
      if (node.type === "ThisExpression") return true;
      if (node.type === "Identifier" && node.name === "arguments") return true;
    }
    const nextBarrier = (child: t.Node) =>
      LEXICAL_BINDERS.has(child.type) ? true : barrier;
    const keys = t.VISITOR_KEYS[node.type] ?? [];
    for (const k of keys) {
      // Non-computed member property: an Identifier node that is NOT a
      // reference to the `arguments` binding.
      if (
        k === "property" &&
        (node.type === "MemberExpression" ||
          node.type === "OptionalMemberExpression") &&
        !(node as t.MemberExpression).computed
      ) {
        continue;
      }
      const child = (node as unknown as Record<string, unknown>)[k];
      const push = (c: unknown) => {
        if (Array.isArray(c)) {
          for (const cc of c) push(cc);
        } else if (
          typeof c === "object" &&
          c !== null &&
          typeof (c as { type?: unknown }).type === "string"
        ) {
          stack.push({ node: c as t.Node, barrier: nextBarrier(c as t.Node) });
        }
      };
      push(child);
    }
  }
  return false;
}

function wrapperSpellingKey(s: Stmt): WrapperSpellingKey | null {
  const firstLine = s.text.split("\n", 1)[0];
  const call = SEQUENCE_CALL_HEAD.exec(firstLine);
  if (!call) return null;
  const afterCall = firstLine.slice(call[0].length);
  let head: { re: RegExp; form: "arrow" | "function" } | null = null;
  for (const h of WRAPPER_HEADS) {
    if (h.re.test(afterCall)) {
      head = h;
      break;
    }
  }
  if (!head) return null;
  const m = head.re.exec(afterCall);
  if (!m) return null; // unreachable (tested above); keeps TS happy
  const rest =
    firstLine.slice(call[0].length + m[0].length) +
    s.text.slice(firstLine.length);
  const normalized = call[0] + `function (${m[1]}) {` + rest;
  let ast: ReturnType<typeof parseSync>;
  try {
    ast = parseSync(normalized, { sourceType: "unambiguous" });
  } catch {
    return null; // a broken normalization is a refusal, never a crash
  }
  if (!ast || ast.program.body.length !== 1) return null;
  // The wrapper sits exactly at the offset where the canonical head was
  // written; a `this`/`arguments` it can observe makes the flip a real
  // semantic change — refuse (see the detector docstring).
  const wrapper = findNodeAt(ast.program.body[0], call[0].length);
  if (!wrapper || wrapper.type !== "FunctionExpression") return null;
  if (wrapperOwnsLexicalBindingUse(wrapper as t.FunctionExpression))
    return null;
  return { form: head.form, hash: statementHash(ast.program.body[0]) };
}

/** First node in visitor order whose `start` is the given offset. */
function findNodeAt(root: t.Node, offset: number): t.Node | null {
  const stack: t.Node[] = [root];
  while (stack.length > 0) {
    const node = stack.pop() as t.Node;
    if (node.start === offset) return node;
    const keys = t.VISITOR_KEYS[node.type] ?? [];
    for (const k of keys) {
      const child = (node as unknown as Record<string, unknown>)[k];
      if (Array.isArray(child)) {
        for (const c of child) {
          if (c && typeof c.type === "string") stack.push(c);
        }
      } else if (
        typeof child === "object" &&
        child !== null &&
        typeof (child as { type?: unknown }).type === "string"
      ) {
        stack.push(child as t.Node);
      }
    }
  }
  return null;
}

/**
 * The soft-noise pass: pair leftover fresh statements with leftover removed
 * statements that are the same code modulo WRAPPER SPELLING (one side an
 * arrow, the other a function expression — `wrapperSpellingKey` for the
 * rule). Charges NOTHING to the existing columns; counts the mass the current
 * rules already charged to `real` for those pairs into
 * `tally.spellingIdenticalLines`, so the scoreboard stays byte-identical and
 * the breakdown can say "of which N are spelling-only".
 */
function chargeSpellingTwins(
  leftoverFresh: Stmt[],
  leftoverRemoved: Stmt[],
  tally: Tally,
  sink?: NoiseSink
): void {
  const removedByKey = new Map<
    string,
    Array<{ key: WrapperSpellingKey; stmt: Stmt }>
  >();
  for (const r of leftoverRemoved) {
    const key = wrapperSpellingKey(r);
    if (!key) continue;
    const list = removedByKey.get(key.hash) ?? [];
    list.push({ key, stmt: r });
    removedByKey.set(key.hash, list);
  }
  for (const s of leftoverFresh) {
    const key = wrapperSpellingKey(s);
    if (!key) continue;
    const bucket = removedByKey.get(key.hash);
    if (!bucket) continue;
    const i = bucket.findIndex((e) => e.key.form !== key.form);
    if (i < 0) continue; // same spelling on both sides is not a flip
    const twin = bucket.splice(i, 1)[0].stmt;
    const lines = s.lines.length + twin.lines.length;
    tally.spellingIdenticalLines += lines;
    keep(sink, {
      kind: "spelling",
      file: sink?.file ?? "",
      lines,
      priorText: twin.text,
      freshText: s.text
    });
  }
}

function classifyFile(
  priorCode: string,
  freshCode: string,
  tally: Tally,
  sink?: NoiseSink,
  opts: Required<ComposeOptions> = DEFAULTS
): void {
  const prior = statementsOf(priorCode);
  const fresh = statementsOf(freshCode);

  // 1. exact (hash+text) pairing, FIFO by multiset
  const exactKey = (s: Stmt) => `${s.hash}\u0000${s.text}`;
  const priorExact = new Map<string, number>();
  for (const s of prior)
    priorExact.set(exactKey(s), (priorExact.get(exactKey(s)) ?? 0) + 1);
  const freshExactMatched: Stmt[] = [];
  const freshRest: Stmt[] = [];
  for (const s of fresh) {
    const k = exactKey(s);
    const n = priorExact.get(k) ?? 0;
    if (n > 0) {
      priorExact.set(k, n - 1);
      freshExactMatched.push(s);
    } else freshRest.push(s);
  }
  const priorRest: Stmt[] = [];
  const stillAvailable = new Map(priorExact);
  const priorExactMatched: Stmt[] = [];
  for (const s of prior) {
    const k = exactKey(s);
    const n = stillAvailable.get(k) ?? 0;
    if (n > 0) {
      stillAvailable.set(k, n - 1);
      priorRest.push(s); // unmatched leftover copy
    } else priorExactMatched.push(s);
  }

  // REORDER: exact-matched statements emitted out of order.
  const inOrder = onLcs(
    priorExactMatched.map(exactKey),
    freshExactMatched.map(exactKey)
  );
  freshExactMatched.forEach((s, i) => {
    if (!inOrder.has(i)) {
      tally.reorder += s.lines.length * 2; // delete + add
      keep(sink, {
        kind: "reorder",
        file: sink?.file ?? "",
        lines: s.lines.length * 2,
        freshText: s.text
      });
    }
  });

  // 2. same hash, different text -> NAMING churn (charged actual differing lines)
  const priorByHash = new Map<string, Stmt[]>();
  for (const s of priorRest) {
    const l = priorByHash.get(s.hash) ?? [];
    l.push(s);
    priorByHash.set(s.hash, l);
  }
  const novelFresh: Stmt[] = [];
  for (const s of freshRest) {
    const bucket = priorByHash.get(s.hash);
    const candidates = bucket?.length ?? 0;
    const picked = bucket ? takeTwin(bucket, s, opts) : null;
    if (picked) {
      const { twin, score } = picked;
      const churn = lineChurn(twin.lines, s.lines);
      const isAlias = s.isRequire && twin.isRequire;
      if (isAlias) tally.alias += churn;
      else tally.naming += churn;
      if (churn > 0)
        keep(sink, {
          kind: isAlias ? "alias" : "naming",
          file: sink?.file ?? "",
          lines: churn,
          priorText: twin.text,
          freshText: s.text,
          candidates,
          score
        });
    } else {
      // No hash twin, or none that corroborated: new OR edited code, priced
      // by step 3 below.
      novelFresh.push(s);
    }
  }
  const removed: Stmt[] = [];
  for (const l of priorByHash.values()) for (const s of l) removed.push(s);

  // 3. A hash-flipped statement is usually an EDITED version of a prior one, not
  // a wholesale add+remove. Pair it with the removed statement it came from
  // (same rename-blind head, >=50% token overlap — the diff-ledger's rule) and
  // charge only the lines a line-diff would print. Without this, one edited line
  // inside a 5k-line statement is charged as 5k lines of "real change" (the
  // statement-mass trap the 034 README documents).
  const removedByHead = new Map<string, Stmt[]>();
  for (const s of removed) {
    const k = maskedHead(s.text);
    const l = removedByHead.get(k) ?? [];
    l.push(s);
    removedByHead.set(k, l);
  }
  const usedRemoved = new Set<Stmt>();
  const tier3PairedFresh = new Set<Stmt>();
  for (const s of novelFresh) {
    const sw = tokenSet(s.text);
    let best: Stmt | null = null;
    let bestScore = 0;
    for (const c of removedByHead.get(maskedHead(s.text)) ?? []) {
      if (usedRemoved.has(c)) continue;
      const cw = tokenSet(c.text);
      let inter = 0;
      for (const w of cw) if (sw.has(w)) inter++;
      const score = inter / Math.max(sw.size, cw.size, 1);
      if (score > bestScore) {
        best = c;
        bestScore = score;
      }
    }
    if (best && bestScore >= 0.5) {
      usedRemoved.add(best);
      tier3PairedFresh.add(s);
      const e = editedLineCounts(s.text, best.text);
      tally.real += e.fresh + e.prior;
      // An EDITED pair: both sides exist, so its charged lines can be walked
      // and asked whether they differ only in identifiers.
      keep(sink, {
        kind: "real",
        file: sink?.file ?? "",
        lines: e.fresh + e.prior,
        priorText: best.text,
        freshText: s.text
      });
    } else {
      tally.real += s.lines.length; // genuinely new code
      keep(sink, {
        kind: "real",
        file: sink?.file ?? "",
        lines: s.lines.length,
        freshText: s.text
      });
    }
  }
  for (const s of removed) {
    if (!usedRemoved.has(s)) {
      tally.real += s.lines.length; // genuinely removed
      keep(sink, {
        kind: "real",
        file: sink?.file ?? "",
        lines: s.lines.length,
        priorText: s.text
      });
    }
  }

  // 4. SOFT-NOISE pass (advisory, additive): among the statements just charged
  // as one-sided real change, pair the ones that are the same code modulo
  // WRAPPER SPELLING. Nothing already charged changes; the pair's mass is
  // REPORTED so "of which N are spelling-only" can be said of `real`.
  chargeSpellingTwins(
    novelFresh.filter((s) => !tier3PairedFresh.has(s)),
    removed.filter((s) => !usedRemoved.has(s)),
    tally,
    sink
  );
}

/**
 * The tally for ONE file pair.
 *
 * `composeDiff` walks directories, which makes it impossible to ask "what does
 * this decomposition say about THIS file?" — the question you need to check the
 * decomposition against git, which diffs one file at a time. Same `classifyFile`,
 * so a per-file check cannot drift from the totals.
 */
export function composeFile(
  priorCode: string,
  freshCode: string,
  options?: ComposeOptions
): Tally {
  const tally: Tally = {
    real: 0,
    naming: 0,
    alias: 0,
    reorder: 0,
    fileAddRemove: 0,
    spellingIdenticalLines: 0
  };
  classifyFile(priorCode, freshCode, tally, undefined, {
    ...DEFAULTS,
    ...options
  });
  return tally;
}

/**
 * Decompose the on-disk diff between two split trees into real change and each
 * noise mechanism, in git lines. Exported so the eval harness can score emit
 * layout without duplicating the rule — its statement classification is
 * position-AWARE, which is exactly what `analyze.ts` cannot see.
 */
export function composeDiff(
  priorDir: string,
  freshDir: string,
  /** Opt-in: collect up to `cap` readable noise instances alongside the tally. */
  collect?: { samples: NoiseSample[]; cap: number },
  options?: ComposeOptions
): Tally {
  const opts = { ...DEFAULTS, ...options };
  const priorFiles = new Set(walk(priorDir));
  const freshFiles = new Set(walk(freshDir));
  const tally: Tally = {
    real: 0,
    naming: 0,
    alias: 0,
    reorder: 0,
    fileAddRemove: 0,
    spellingIdenticalLines: 0
  };

  for (const f of freshFiles) {
    if (priorFiles.has(f)) {
      try {
        classifyFile(
          fs.readFileSync(path.join(priorDir, f), "utf8"),
          fs.readFileSync(path.join(freshDir, f), "utf8"),
          tally,
          collect
            ? { file: f, samples: collect.samples, cap: collect.cap }
            : undefined,
          opts
        );
      } catch (e) {
        throw new Error(`${f}: ${e instanceof Error ? e.message : e}`);
      }
    } else {
      tally.fileAddRemove += fs
        .readFileSync(path.join(freshDir, f), "utf8")
        .split("\n").length;
    }
  }
  for (const f of priorFiles) {
    if (!freshFiles.has(f)) {
      tally.fileAddRemove += fs
        .readFileSync(path.join(priorDir, f), "utf8")
        .split("\n").length;
    }
  }
  return tally;
}

function main() {
  const [priorDir, freshDir, label] = process.argv.slice(2);
  const tally = composeDiff(priorDir, freshDir);
  const noise = tally.naming + tally.alias + tally.reorder;
  const total = noise + tally.real + tally.fileAddRemove;
  const pct = (n: number) => ((100 * n) / total).toFixed(1).padStart(5);
  console.log(`=== DIFF COMPOSITION${label ? ` — ${label}` : ""} ===`);
  console.log(`  accounted churn lines: ${total}`);
  console.log(
    `  REAL change            ${String(tally.real).padStart(7)}  ${pct(tally.real)}%`
  );
  console.log(
    `  new/removed files      ${String(tally.fileAddRemove).padStart(7)}  ${pct(tally.fileAddRemove)}%`
  );
  console.log(
    `  --- noise ---          ${String(noise).padStart(7)}  ${pct(noise)}%`
  );
  console.log(
    `    naming churn         ${String(tally.naming).padStart(7)}  ${pct(tally.naming)}%`
  );
  console.log(
    `    require-alias churn  ${String(tally.alias).padStart(7)}  ${pct(tally.alias)}%`
  );
  console.log(
    `    reorder churn        ${String(tally.reorder).padStart(7)}  ${pct(tally.reorder)}%`
  );
  console.log(
    `    spelling-identical   ${String(tally.spellingIdenticalLines).padStart(7)}  ` +
      `${pct(tally.spellingIdenticalLines)}%  (soft noise: charged inside REAL, ` +
      "wrapper arrow<->function flips)"
  );
  console.log(
    `ROW|${label ?? ""}|${total}|${tally.real}|${tally.fileAddRemove}|${tally.naming}|${tally.alias}|${tally.reorder}|${tally.spellingIdenticalLines}`
  );
}

// Run the CLI only when executed directly, not when imported.
if (
  process.argv[1] &&
  import.meta.url.endsWith(process.argv[1].split("/").pop() ?? "")
) {
  main();
}
