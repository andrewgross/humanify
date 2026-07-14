/**
 * Rename ledger — a replayable record of every identifier rename, keyed by
 * byte position so it can be applied to reproduce the humanified output
 * without re-running the LLM.
 *
 * Derivation (no pipeline instrumentation): the post-rename AST still holds
 * each identifier's ORIGINAL parse offsets (Babel's `scope.rename` changes
 * `.name`, never `.start/.end`), so for every binding we compare the source
 * text at its declaration offset (the original name) against the node's
 * current `.name` (the final name). A difference is a rename; its
 * occurrences are the declaration + all reference and write-target offsets.
 *
 * Because renames only change identifier tokens — never structure or
 * formatting — a right-to-left text splice of `finalName` at every recorded
 * offset in the beautified source snapshot reproduces the generated output
 * exactly (the `applyRenameLedger` ⇔ `generate(ast)` invariant, which the
 * pipeline self-checks when emitting a ledger). The base entries' coordinate
 * space is the BEAUTIFIED input the LLM rename passes ran on. Post-generate
 * passes (`--reconcile-prior-diff`, the deferred sweep) rename the generated
 * output in their own spaces; each is captured as a `post` stage keyed by its
 * own snapshot hash, so replay reproduces the FINAL shipped output.
 */

import { createHash } from "node:crypto";
import type { Binding, NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { traverse, violationWriteTargetPaths } from "../babel-utils.js";

/** A half-open [start, end) byte range in the source snapshot. */
export type Span = [number, number];

export interface RenameLedgerEntry {
  /** The identifier's text in the source snapshot (the minified name). */
  originalName: string;
  /** The name it was renamed to (the shipped name). */
  finalName: string;
  /** Every occurrence to rewrite: declaration + reads + write targets. */
  occurrences: Span[];
}

export interface RenameLedger {
  version: 1;
  /** sha256 of the source snapshot these offsets index into. */
  sourceSha256: string;
  entries: RenameLedgerEntry[];
  /**
   * Post-generate stages (`--reconcile-prior-diff`, deferred sweep). Each
   * renames the PREVIOUS stage's generated output — a distinct coordinate
   * space — so it carries its own `sourceSha256` (the prior stage's output)
   * and offsets. Applied in order after `entries`, they reproduce the final
   * shipped output, not just the LLM-rename output.
   */
  post?: Array<{ sourceSha256: string; entries: RenameLedgerEntry[] }>;
}

function sha256(text: string): string {
  return createHash("sha256").update(text).digest("hex");
}

/** Every [start, end) byte range of a binding's identifier — declaration,
 * reads (referencePaths), and write targets (constantViolations). */
function occurrencesOf(binding: Binding, finalName: string): Span[] {
  const byStart = new Map<number, Span>();
  const add = (n: { start?: number | null; end?: number | null }): void => {
    if (n.start != null && n.end != null)
      byStart.set(n.start, [n.start, n.end]);
  };
  add(binding.identifier);
  for (const ref of binding.referencePaths) {
    if (ref.isIdentifier()) add(ref.node);
  }
  for (const violation of binding.constantViolations) {
    for (const idPath of violationWriteTargetPaths(violation, finalName)) {
      add(idPath.node);
    }
  }
  return [...byStart.values()];
}

/**
 * Derive the rename ledger from the beautified source snapshot and the
 * post-rename AST (offsets index into `source`).
 */
export function buildRenameLedger(source: string, ast: t.File): RenameLedger {
  const entries: RenameLedgerEntry[] = [];
  const seen = new Set<number>();
  traverse(ast, {
    enter(path: NodePath) {
      // Process each scope once, at its owning path.
      if (path.scope.path !== path) return;
      for (const name of Object.keys(path.scope.bindings)) {
        const binding = path.scope.bindings[name];
        const start = binding.identifier.start;
        const end = binding.identifier.end;
        if (start == null || end == null || seen.has(start)) continue;
        seen.add(start);
        const originalName = source.slice(start, end);
        const finalName = binding.identifier.name;
        if (originalName === finalName) continue;
        entries.push({
          originalName,
          finalName,
          occurrences: occurrencesOf(binding, finalName)
        });
      }
    }
  });
  return { version: 1, sourceSha256: sha256(source), entries };
}

/** Apply one stage's entries to `source`, verifying the snapshot hash first.
 * A pure text transform: every occurrence is spliced to its finalName,
 * right-to-left so earlier offsets stay valid. */
function applyStage(
  source: string,
  sourceSha256: string,
  entries: RenameLedgerEntry[]
): string {
  if (sha256(source) !== sourceSha256) {
    throw new Error(
      "rename ledger: source does not match the ledger's sourceSha256"
    );
  }
  const edits: Array<{ start: number; end: number; text: string }> = [];
  for (const entry of entries) {
    for (const [start, end] of entry.occurrences) {
      edits.push({ start, end, text: entry.finalName });
    }
  }
  edits.sort((a, b) => b.start - a.start);
  let out = source;
  let prevStart = Number.POSITIVE_INFINITY;
  for (const edit of edits) {
    if (edit.end > prevStart) {
      throw new Error("rename ledger: overlapping occurrences (internal)");
    }
    prevStart = edit.start;
    out = out.slice(0, edit.start) + edit.text + out.slice(edit.end);
  }
  return out;
}

/**
 * Replay a ledger onto its source snapshot, returning the renamed code.
 * The base `entries` reproduce the LLM-rename output; each `post` stage then
 * transforms that output through the reconcile / deferred-sweep coordinate
 * spaces, so the result is the final shipped code.
 */
export function applyRenameLedger(
  source: string,
  ledger: RenameLedger
): string {
  let out = applyStage(source, ledger.sourceSha256, ledger.entries);
  for (const stage of ledger.post ?? []) {
    out = applyStage(out, stage.sourceSha256, stage.entries);
  }
  return out;
}
