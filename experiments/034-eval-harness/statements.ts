/**
 * Shared statement extraction for the eval harness: top-level statements of a
 * humanified single-file output, each with its rename-invariant statementHash
 * and source text.
 *
 * Wrapper location is delegated to `bundleStatements` — the owner — which
 * THROWS on a wrapperless input. This used to fall back silently to the
 * Program body, which for a bundle yields one giant statement and garbage
 * numbers instead of an error; `experiments/lib/trees.ts` was written
 * specifically to make that an error, and this file predated it.
 */
import { statementHash } from "../../src/split/statement-hash.js";
import { bundleStatements } from "../lib/trees.js";

export interface Stmt {
  hash: string;
  text: string;
  lines: number;
}

export function statementsOf(code: string): Stmt[] {
  return bundleStatements(code, "statementsOf").map((stmt) => {
    const text =
      stmt.start != null && stmt.end != null
        ? code.slice(stmt.start, stmt.end)
        : "";
    return {
      hash: statementHash(stmt),
      text,
      lines: text.length ? text.split("\n").length : 0
    };
  });
}
