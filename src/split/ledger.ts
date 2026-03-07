import * as t from "@babel/types";
import type { SplitLedger, SplitLedgerEntry } from "./types.js";

/**
 * Walk top-level AST nodes and register each in a new ledger.
 */
export function collectLedger(ast: t.File, filePath: string): SplitLedger {
  const entries = new Map<string, SplitLedgerEntry>();
  const duplicated = new Map<string, string[]>();

  for (const stmt of ast.program.body) {
    const line = stmt.loc?.start.line ?? 0;
    const id = `${filePath}:${line}:${stmt.type}`;

    entries.set(id, {
      id,
      node: stmt,
      type: stmt.type,
      source: filePath,
    });
  }

  return { entries, duplicated };
}

/**
 * Record that a ledger entry has been assigned to an output file.
 */
export function assignEntry(ledger: SplitLedger, entryId: string, outputFile: string): void {
  const entry = ledger.entries.get(entryId);
  if (!entry) {
    throw new Error(`Ledger entry not found: ${entryId}`);
  }
  entry.outputFile = outputFile;
}

/**
 * Assert all ledger entries have been assigned. Throws with details if not.
 */
export function verifyComplete(ledger: SplitLedger): void {
  const unassigned: SplitLedgerEntry[] = [];

  for (const entry of ledger.entries.values()) {
    if (!entry.outputFile) {
      unassigned.push(entry);
    }
  }

  if (unassigned.length > 0) {
    const details = unassigned
      .map(e => `  - ${e.id} (${e.type})`)
      .join("\n");
    throw new Error(
      `Split would drop ${unassigned.length} nodes:\n${details}`
    );
  }
}

/**
 * Compute summary stats from the ledger.
 */
export function summarize(ledger: SplitLedger): {
  totalEntries: number;
  assignedEntries: number;
  unassignedEntries: number;
  outputFiles: number;
} {
  let assigned = 0;
  const files = new Set<string>();

  for (const entry of ledger.entries.values()) {
    if (entry.outputFile) {
      assigned++;
      files.add(entry.outputFile);
    }
  }

  return {
    totalEntries: ledger.entries.size,
    assignedEntries: assigned,
    unassignedEntries: ledger.entries.size - assigned,
    outputFiles: files.size,
  };
}
