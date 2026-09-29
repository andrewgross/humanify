/**
 * The ask-log comparator — the decision-level sibling of the byte-diff
 * neutrality gate, with zero LLM calls.
 *
 *   npx tsx scripts/diff-asks.ts <a.jsonl> <b.jsonl>
 *
 * Compares two `--dump-asks` logs (scripts/ask-trace.ts produces them) and
 * prints a human delta: asks only in A, only in B, and asks that match but
 * CHANGED (same scope/round/identifiers/cause, different reason, wave,
 * prior-context or variant). Intended uses:
 *
 *  (a) two BINARIES on the same input — a should-change-nothing refactor
 *      must produce an empty diff (exit 0; anything else names the ask
 *      that moved, even when every byte of the tree happens to land the
 *      same);
 *  (b) two VERSIONS' runs — the fall-through structure between hops;
 *  (c) a behavior change's blast radius — the diff IS the predicted call
 *      delta.
 *
 * Matching: asks pair on (site, scope, round, identifiers, isRetry,
 * retryCause) — the ask's identity, not its schedule. Wave, phase,
 * priorContext, usedNamesCount, promptVariant and reason are COMPARED
 * fields (a change in any is a difference). Exit 0 only when the two logs
 * are identical row for row; exit 1 prints the delta. Totals first.
 */
import * as fs from "node:fs";
import * as path from "node:path";

import type { AskRow } from "./ask-trace.js";

export interface AskDelta {
  /** Rows with no counterpart in B. */
  onlyA: AskRow[];
  /** Rows with no counterpart in A. */
  onlyB: AskRow[];
  /** Matched on identity but differing in a compared field. */
  changed: Array<{ a: AskRow; b: AskRow; fields: string[] }>;
  identical: boolean;
}

/** The ask's identity — what two rows must agree on to be the same ask. */
function identity(r: AskRow): string {
  return JSON.stringify([
    r.site,
    r.scope,
    r.round,
    r.identifiers,
    r.isRetry,
    r.retryCause ?? null,
    r.retryCauseDetail ?? null
  ]);
}

/** Which compared fields two identified asks disagree on. */
function differing(a: AskRow, b: AskRow): string[] {
  const out: string[] = [];
  if (a.reason !== b.reason) out.push("reason");
  if (a.priorContext !== b.priorContext) out.push("priorContext");
  if ((a.wave ?? null) !== (b.wave ?? null)) out.push("wave");
  if ((a.phase ?? null) !== (b.phase ?? null)) out.push("phase");
  if (a.usedNamesCount !== b.usedNamesCount) out.push("usedNamesCount");
  if (a.promptVariant !== b.promptVariant) out.push("promptVariant");
  if (a.scopeKind !== b.scopeKind) out.push("scopeKind");
  return out;
}

/** The multiset diff of two ask logs. */
export function diffAsks(a: AskRow[], b: AskRow[]): AskDelta {
  const pool = new Map<string, AskRow[]>();
  for (const r of b) {
    const k = identity(r);
    pool.set(k, [...(pool.get(k) ?? []), r]);
  }
  const onlyA: AskRow[] = [];
  const changed: Array<{ a: AskRow; b: AskRow; fields: string[] }> = [];
  for (const ra of a) {
    const k = identity(ra);
    const bucket = pool.get(k);
    if (!bucket || bucket.length === 0) {
      onlyA.push(ra);
      continue;
    }
    const [rb, ...rest] = bucket;
    pool.set(k, rest);
    const fields = differing(ra, rb);
    if (fields.length > 0) changed.push({ a: ra, b: rb, fields });
  }
  const onlyB = [...pool.values()].flat();
  return {
    onlyA,
    onlyB,
    changed,
    identical: onlyA.length === 0 && onlyB.length === 0 && changed.length === 0
  };
}

/** Parse one `--dump-asks` log. */
export function readAsks(path: string): AskRow[] {
  const text = fs.readFileSync(path, "utf8");
  const rows: AskRow[] = [];
  for (const [i, line] of text.split("\n").entries()) {
    if (!line.trim()) continue;
    try {
      rows.push(JSON.parse(line) as AskRow);
    } catch (e) {
      throw new Error(`${path}:${i + 1}: not a JSONL row: ${String(e)}`);
    }
  }
  return rows;
}

function describe(r: AskRow): string {
  const cause = r.isRetry ? ` retry=${r.retryCause ?? "?"}` : "";
  return `[${r.site}] ${r.scope} (round ${r.round},${cause} ${r.reason}) [${r.identifiers.join(", ")}]`;
}

/** One compared field's value as printed in a CHANGED line. */
function fieldValue(r: AskRow, field: string): string {
  const values: Record<string, unknown> = {
    reason: r.reason,
    priorContext: r.priorContext,
    wave: r.wave ?? null,
    phase: r.phase ?? null,
    usedNamesCount: r.usedNamesCount,
    promptVariant: r.promptVariant,
    scopeKind: r.scopeKind
  };
  return JSON.stringify(values[field] ?? null);
}

/** `  reason: A 3 / B 5` per reason, both logs' counts side by side. */
function reasonCounts(a: AskRow[], b: AskRow[]): string[] {
  const count = (rows: AskRow[]) =>
    rows.reduce(
      (m, r) => m.set(r.reason, (m.get(r.reason) ?? 0) + 1),
      new Map<string, number>()
    );
  const ca = count(a);
  const cb = count(b);
  const keys = [...new Set([...ca.keys(), ...cb.keys()])].sort();
  return keys.map((k) => `  ${k}: A ${ca.get(k) ?? 0} / B ${cb.get(k) ?? 0}`);
}

/** The rows under a group heading, per scope, sorted. */
function groupLines(label: string, rows: AskRow[]): string[] {
  if (rows.length === 0) return [];
  const byScope = new Map<string, AskRow[]>();
  for (const r of rows) {
    byScope.set(r.scope, [...(byScope.get(r.scope) ?? []), r]);
  }
  const scopes = [...byScope.keys()].sort();
  const body = scopes.flatMap((scope) =>
    (byScope.get(scope) ?? []).map((r) => `  ${describe(r)}`)
  );
  return [``, `— ${label} (${rows.length}) —`, ...body];
}

/** The human delta, totals first. */
export function formatDelta(
  labels: { a: string; b: string },
  a: AskRow[],
  b: AskRow[],
  d: AskDelta
): string {
  const lines: string[] = [
    `A: ${labels.a} — ${a.length} ask(s)`,
    `B: ${labels.b} — ${b.length} ask(s)`,
    ...reasonCounts(a, b),
    `IDENTICAL: ${d.identical ? "yes" : "no"} — only-in-A ${d.onlyA.length}, only-in-B ${d.onlyB.length}, changed ${d.changed.length}`
  ];
  lines.push(...groupLines("ONLY IN A", d.onlyA));
  lines.push(...groupLines("ONLY IN B", d.onlyB));
  if (d.changed.length > 0) {
    lines.push(``, `— CHANGED (${d.changed.length}) —`);
    for (const { a: ra, b: rb, fields } of d.changed) {
      const moves = fields
        .map((f) => `${f} ${fieldValue(ra, f)} → ${fieldValue(rb, f)}`)
        .join(", ");
      lines.push(`  ${describe(ra)}: ${moves}`);
    }
  }
  return lines.join("\n");
}

function main(): void {
  const [a, b] = process.argv.slice(2);
  if (!a || !b || a.startsWith("-") || b.startsWith("-")) {
    console.error("usage: npx tsx scripts/diff-asks.ts <a.jsonl> <b.jsonl>");
    process.exit(2);
  }
  const rowsA = readAsks(a);
  const rowsB = readAsks(b);
  const delta = diffAsks(rowsA, rowsB);
  console.log(formatDelta({ a, b }, rowsA, rowsB, delta));
  process.exit(delta.identical ? 0 : 1);
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(import.meta.filename)
) {
  main();
}
