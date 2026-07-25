/**
 * Offline replay of the split's prior-carried assignment, so a proposed tier can
 * be sized AT THE POINT OF DECISION rather than guessed at from the output tree.
 *
 * exp040's `relocation-churn.ts` sizes relocation from the trees: it finds
 * statements that ended up in a different file and asks whether a rare literal
 * could have identified them. That is the right size but the wrong question —
 * it cannot say whether the splitter would have USED the evidence, because it
 * does not know which tier placed the statement.
 *
 * This module reconstructs the decision. Given the two bundles and their split
 * ledgers it recomputes `assignWithPrior` (src/split/stable-split.ts) exactly:
 * hash tier -> name vote -> identity fill -> locality. It then lets a caller
 * swap in a CANDIDATE vote rule and diff the outcome, per statement, against
 * what actually shipped.
 *
 * Faithfulness is checked, not assumed: `replay()` reports its own tier counts
 * (compare against the pipeline's "inherited N/M (... via hashes, ... via
 * ordinals, ... residue by locality)" log line) and the share of statements
 * whose replayed file matches the ledger. If those do not line up, nothing
 * measured on top is trustworthy.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { contentAnchorPairs } from "../../src/split/content-anchor.js";
import { editedLineCounts, tokenSet } from "../034-eval-harness/diff-ledger.js";

export interface Ledger {
  files: string[];
  nameToFiles: Record<string, string[]>;
  order: string[];
  hashes?: string[];
}

export interface Stmt {
  idx: number;
  text: string;
  lines: number;
  /** The file this statement actually landed in (ledger.order[idx]). */
  file: string;
  /** String literals of 12+ chars — the content-anchor key. */
  literals: string[];
  /** Every binding identifier, params included — what the shipped splitter
   * votes on and what it writes into `nameToFiles`. */
  names: string[];
  /** Only the statement's OUTER bindings: the function/class/var names it
   * declares at module scope. A function's parameters are not module bindings
   * of the statement, but the shipped `declaredNames` counts them anyway. */
  outerNames: string[];
}

/** exp040's rule: long enough to be distinctive prose or a key, rather than a
 * flag name or a single word. */
const LITERAL = /"([^"\\\n]{12,})"|'([^'\\\n]{12,})'/g;

function literalsOf(text: string): string[] {
  const out: string[] = [];
  for (const m of text.matchAll(LITERAL)) out.push(m[1] ?? m[2]);
  return out;
}

export function readLedger(dir: string): Ledger {
  return JSON.parse(
    fs.readFileSync(path.join(dir, ".humanify", "split-ledger.json"), "utf8")
  ) as Ledger;
}

export function readMatchMap(dir: string): Map<string, string> {
  const p = path.join(dir, ".humanify", "prior-match-map.json");
  if (!fs.existsSync(p)) return new Map();
  return new Map(
    Object.entries(
      JSON.parse(fs.readFileSync(p, "utf8")) as Record<string, string>
    )
  );
}

/**
 * The bundle's top-level statements exactly as the splitter sees them — the
 * wrapper function's body in BUNDLE order, so index i lines up with
 * ledger.order[i] and ledger.hashes[i].
 */
export function loadSide(dir: string, ledger: Ledger): Stmt[] {
  const code = fs.readFileSync(
    path.join(dir, ".humanify", "humanified.js"),
    "utf8"
  );
  const ast = parseFileAst(code);
  if (!ast) throw new Error(`parse failed: ${dir}`);
  const wrapper = findWrapperFunction(ast);
  const bodyNode = wrapper?.functionPath.node.body;
  if (!bodyNode || !t.isBlockStatement(bodyNode)) {
    throw new Error(`no wrapper body: ${dir}`);
  }
  const body = bodyNode.body;
  if (body.length !== ledger.order.length) {
    throw new Error(
      `ledger/body mismatch in ${dir}: ledger ${ledger.order.length} vs body ${body.length}`
    );
  }
  return body.map((s, idx) => {
    const text =
      s.start != null && s.end != null ? code.slice(s.start, s.end) : "";
    return {
      idx,
      text,
      lines: text ? text.split("\n").length : 0,
      file: ledger.order[idx],
      literals: literalsOf(text),
      names: Object.keys(t.getBindingIdentifiers(s, false)),
      outerNames: Object.keys(t.getOuterBindingIdentifiers(s, false))
    };
  });
}

// ---------------------------------------------------------------------------
// vote rules
// ---------------------------------------------------------------------------

export interface VoteRule {
  /** Which of a statement's names get to vote. */
  useOuterNames: boolean;
  /**
   * When the voters disagree, should a UNANIMOUS subset of "all-same" votes
   * (names with exactly one home file in the prior release) decide, instead of
   * the whole statement falling to locality? Ordinal votes are positional
   * guesses across a name declared in many files; an all-same vote is not.
   */
  allSameFirst: boolean;
}

export const SHIPPED: VoteRule = { useOuterNames: false, allSameFirst: false };

/** `nameToFiles` as buildLedger writes it, for a given name set — the prior
 * side must be rebuilt this way or the ordinal denominators do not correspond. */
export function nameToFilesFrom(
  stmts: Stmt[],
  useOuterNames: boolean
): Map<string, string[]> {
  const m = new Map<string, string[]>();
  for (const s of stmts) {
    for (const n of useOuterNames ? s.outerNames : s.names) {
      const list = m.get(n) ?? [];
      list.push(s.file);
      m.set(n, list);
    }
  }
  return m;
}

export type TierKind =
  | "hash"
  | "name"
  | "ordinal"
  | "allsame"
  | "fill"
  | "anchor"
  | "conflict"
  | "novote";

/** Placed by "follow your preceding neighbour" — no identity evidence used. */
export function isLocality(k: TierKind): boolean {
  return k === "conflict" || k === "novote";
}

export interface ReplayResult {
  assignment: string[];
  kind: TierKind[];
  counts: Record<TierKind, number>;
  /** Per statement, the files its names voted for (diagnostics). */
  ledgerAgreement: number;
}

function hashTier(
  currentHashes: string[],
  prior: Ledger
): Array<string | undefined> {
  if (!prior.hashes || prior.hashes.length !== prior.order.length) {
    return new Array(currentHashes.length);
  }
  const priorFiles = new Map<string, string[]>();
  for (let i = 0; i < prior.hashes.length; i++) {
    const list = priorFiles.get(prior.hashes[i]) ?? [];
    list.push(prior.order[i]);
    priorFiles.set(prior.hashes[i], list);
  }
  const counts = new Map<string, number>();
  for (const h of currentHashes) counts.set(h, (counts.get(h) ?? 0) + 1);
  return currentHashes.map((h) => {
    const files = priorFiles.get(h);
    if (!files || files.length !== counts.get(h)) return undefined;
    return files.every((f) => f === files[0]) ? files[0] : undefined;
  });
}

function identityFill(
  fresh: Stmt[],
  matchMap: Map<string, string>,
  priorNames: Map<string, string[]>
): Array<string | undefined> {
  if (matchMap.size === 0) return new Array(fresh.length);
  return fresh.map((s) => {
    const votes = new Set<string>();
    for (const name of s.names) {
      const priorName = matchMap.get(name);
      if (!priorName) continue;
      const files = priorNames.get(priorName);
      if (files && files.length > 0 && files.every((f) => f === files[0])) {
        votes.add(files[0]);
      }
    }
    return votes.size === 1 ? [...votes][0] : undefined;
  });
}

export interface ReplayInput {
  fresh: Stmt[];
  freshHashes: string[];
  prior: Ledger;
  /** Prior name->files, rebuilt for the rule's name set. */
  priorNames: Map<string, string[]>;
  matchMap: Map<string, string>;
  /**
   * Optional extra tier between the name vote and locality — the content
   * anchor. Fires only where the vote abstained, so it can never contradict a
   * tier that has evidence. Undefined entries abstain.
   */
  anchorTier?: Array<string | undefined>;
}

/** One statement's vote, under a rule. Exposed so the ceiling scripts can show
 * WHY a statement fell to locality. */
export function voteOf(
  names: string[],
  seen: Map<string, number>,
  priorNames: Map<string, string[]>,
  newCounts: Map<string, number>,
  rule: VoteRule
): { file?: string; kind: TierKind } {
  const votes = new Set<string>();
  const allSame = new Set<string>();
  let usedOrdinal = false;
  for (const name of names) {
    const ordinal = seen.get(name) ?? 0;
    seen.set(name, ordinal + 1);
    const files = priorNames.get(name);
    if (!files || files.length === 0) continue;
    if (files.every((f) => f === files[0])) {
      votes.add(files[0]);
      allSame.add(files[0]);
    } else if (newCounts.get(name) === files.length && ordinal < files.length) {
      votes.add(files[ordinal]);
      usedOrdinal = true;
    }
  }
  if (votes.size === 1) {
    return { file: [...votes][0], kind: usedOrdinal ? "ordinal" : "name" };
  }
  if (rule.allSameFirst && allSame.size === 1) {
    return { file: [...allSame][0], kind: "allsame" };
  }
  return { kind: votes.size > 1 ? "conflict" : "novote" };
}

export function replay(input: ReplayInput, rule: VoteRule): ReplayResult {
  const { fresh, freshHashes, prior, priorNames, matchMap, anchorTier } = input;
  const names = (s: Stmt) => (rule.useOuterNames ? s.outerNames : s.names);
  const newCounts = new Map<string, number>();
  for (const s of fresh) {
    for (const n of names(s)) newCounts.set(n, (newCounts.get(n) ?? 0) + 1);
  }
  const viaHash = hashTier(freshHashes, prior);
  const viaFill = identityFill(fresh, matchMap, priorNames);
  const seen = new Map<string, number>();
  const assignment: string[] = new Array(fresh.length);
  const kind: TierKind[] = new Array(fresh.length);
  const counts: Record<TierKind, number> = {
    hash: 0,
    name: 0,
    ordinal: 0,
    allsame: 0,
    fill: 0,
    anchor: 0,
    conflict: 0,
    novote: 0
  };
  let agree = 0;

  for (let i = 0; i < fresh.length; i++) {
    // The vote is computed for EVERY statement even when the hash tier already
    // decided: it advances the per-name ordinal cursors, which must stay
    // aligned exactly as the shipped code aligns them.
    const vote = voteOf(names(fresh[i]), seen, priorNames, newCounts, rule);
    let file: string;
    let k: TierKind;
    if (viaHash[i] !== undefined) {
      file = viaHash[i] as string;
      k = "hash";
    } else if (vote.file !== undefined) {
      file = vote.file;
      k = vote.kind;
    } else if (vote.kind === "novote" && viaFill[i] !== undefined) {
      file = viaFill[i] as string;
      k = "fill";
    } else if (anchorTier?.[i] !== undefined) {
      file = anchorTier[i] as string;
      k = "anchor";
    } else {
      file = i > 0 ? assignment[i - 1] : prior.files[0];
      k = vote.kind;
    }
    assignment[i] = file;
    kind[i] = k;
    counts[k]++;
    if (file === fresh[i].file) agree++;
  }
  return { assignment, kind, counts, ledgerAgreement: agree };
}

// ---------------------------------------------------------------------------
// pairing a fresh statement with the prior statement it came from
// ---------------------------------------------------------------------------

/** The diff-ledger rule: two statement texts are the same code, edited, when
 * they share at least half their tokens. Load-bearing — without it a single
 * shared string once paired a 5,073-line statement with a 7-line one. */
export function looksLikeSameStatement(a: string, b: string): boolean {
  const ta = tokenSet(a);
  const tb = tokenSet(b);
  let inter = 0;
  for (const w of tb) if (ta.has(w)) inter++;
  return inter / Math.max(ta.size, tb.size, 1) >= 0.5;
}

/**
 * Git lines that stop being printed if `fresh` is placed with `twin` instead of
 * where it is today: git prints the prior copy deleted from its file
 * (twin.lines) and the fresh copy added in the new one (fresh.lines);
 * co-located it prints only the lines that actually differ.
 */
export function recoveredLines(fresh: Stmt, twin: Stmt): number {
  const e = editedLineCounts(fresh.text, twin.text);
  return twin.lines + fresh.lines - (e.fresh + e.prior);
}

/**
 * The content anchor, as the split will run it. This is a thin wrapper over the
 * SHIPPED implementation (`src/split/content-anchor.ts`) rather than a second
 * copy of the rules, so the ceiling measured here is a measurement of what
 * actually ships — a parallel implementation would drift and the ceiling would
 * quietly stop describing the tier.
 */
export class AnchorIndex {
  private readonly pairs: Map<number, number>;

  constructor(
    private readonly prior: Stmt[],
    fresh: Stmt[]
  ) {
    this.pairs = contentAnchorPairs(
      prior.map((s) => ({ text: s.text, file: s.file })),
      fresh.map((s) => s.text)
    );
  }

  verdict(f: Stmt): Stmt | undefined {
    const priorIdx = this.pairs.get(f.idx);
    return priorIdx === undefined ? undefined : this.prior[priorIdx];
  }
}

/**
 * Prior statements indexed by outer name, for ACCOUNTING only (never for a
 * placement decision): given a fresh statement, which prior statement is it?
 * Used to price a proposed move in git lines. Unique name match, then the
 * similarity gate; ambiguous pairs are priced at zero rather than guessed.
 */
export class TwinIndex {
  private readonly byName = new Map<string, Stmt[]>();

  constructor(prior: Stmt[]) {
    for (const s of prior) {
      for (const n of s.outerNames) {
        const list = this.byName.get(n) ?? [];
        list.push(s);
        this.byName.set(n, list);
      }
    }
  }

  find(f: Stmt, anchors?: AnchorIndex): Stmt | undefined {
    const cands = new Map<number, Stmt>();
    for (const n of f.outerNames) {
      for (const c of this.byName.get(n) ?? []) cands.set(c.idx, c);
    }
    if (cands.size === 1) {
      const only = [...cands.values()][0];
      if (looksLikeSameStatement(only.text, f.text)) return only;
    }
    return anchors?.verdict(f);
  }
}

export const pct = (a: number, b: number): string =>
  b ? `${((100 * a) / b).toFixed(1)}%` : "-";
