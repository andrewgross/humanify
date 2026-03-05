/**
 * Post-run coverage diagnostics.
 *
 * Aggregates per-function rename reports into a summary showing
 * how many identifiers were renamed vs skipped and why.
 */

import type { FunctionRenameReport, IdentifierOutcome } from "../analysis/types.js";

export interface CoverageSummary {
  functions: { total: number; renamed: number; skipped: number };
  moduleBindings: { total: number; renamed: number; skipped: number };
  identifiers: {
    total: number;
    renamed: number;
    notMinified: number;
    llmMissing: number;
    llmCollision: number;
    llmInvalid: number;
  };
}

/**
 * Build a coverage summary from rename reports.
 *
 * @param reports All rename reports collected during processing
 * @param totalFunctions Total function nodes in the graph (including those with no minified identifiers)
 * @param totalModuleBindings Total module binding nodes in the graph
 */
export function buildCoverageSummary(
  reports: ReadonlyArray<FunctionRenameReport>,
  totalFunctions: number,
  totalModuleBindings: number
): CoverageSummary {
  let fnRenamed = 0;
  let mbRenamed = 0;
  let idTotal = 0;
  let idRenamed = 0;
  let idMissing = 0;
  let idCollision = 0;
  let idInvalid = 0;

  for (const report of reports) {
    const isModuleBinding = report.functionId.startsWith("module-binding-batch:");

    if (isModuleBinding) {
      if (report.renamedCount > 0) mbRenamed++;
    } else {
      if (report.renamedCount > 0) fnRenamed++;
    }

    idTotal += report.totalIdentifiers;

    for (const outcome of Object.values(report.outcomes)) {
      switch (outcome.status) {
        case "renamed":
          idRenamed++;
          break;
        case "missing":
          idMissing++;
          break;
        case "duplicate":
          idCollision++;
          break;
        case "invalid":
          idInvalid++;
          break;
        // "not-collected" identifiers aren't counted in totalIdentifiers
      }
    }
  }

  // Functions that had no minified identifiers at all don't appear in reports,
  // so notMinified at the function level = total - those with reports
  const fnWithReports = reports.filter(r => !r.functionId.startsWith("module-binding-batch:")).length;
  const mbWithReports = reports.filter(r => r.functionId.startsWith("module-binding-batch:")).length;

  return {
    functions: {
      total: totalFunctions,
      renamed: fnRenamed,
      skipped: totalFunctions - fnWithReports
    },
    moduleBindings: {
      total: totalModuleBindings,
      renamed: mbRenamed,
      skipped: totalModuleBindings - mbWithReports
    },
    identifiers: {
      total: idTotal,
      renamed: idRenamed,
      notMinified: 0, // Not tracked at identifier level in reports (filtered before reports)
      llmMissing: idMissing,
      llmCollision: idCollision,
      llmInvalid: idInvalid
    }
  };
}

/**
 * Format a coverage summary as a human-readable block.
 */
export function formatCoverageSummary(summary: CoverageSummary): string {
  const lines: string[] = [];
  const sep = "\u2500";

  lines.push(` ${sep}${sep} Coverage Summary ${sep.repeat(60)}`);

  if (summary.functions.total > 0) {
    const fpct = summary.functions.total > 0
      ? ((summary.functions.renamed / summary.functions.total) * 100).toFixed(1)
      : "0.0";
    lines.push(` Functions:        ${fmt(summary.functions.renamed)} renamed / ${fmt(summary.functions.total)} total  (${fpct}%)`);
  }

  if (summary.moduleBindings.total > 0) {
    const mpct = summary.moduleBindings.total > 0
      ? ((summary.moduleBindings.renamed / summary.moduleBindings.total) * 100).toFixed(1)
      : "0.0";
    lines.push(` Module bindings:  ${fmt(summary.moduleBindings.renamed)} renamed / ${fmt(summary.moduleBindings.total)} total  (${mpct}%)`);
  }

  const id = summary.identifiers;
  if (id.total > 0) {
    const ipct = ((id.renamed / id.total) * 100).toFixed(1);
    lines.push(` Identifiers:      ${fmt(id.renamed)} renamed / ${fmt(id.total)} total  (${ipct}%)`);

    if (id.llmMissing > 0) {
      lines.push(`   LLM missing:    ${fmt(id.llmMissing)}  (not returned after retries)`);
    }
    if (id.llmCollision > 0) {
      lines.push(`   LLM collision:  ${fmt(id.llmCollision)}  (name conflict unresolved)`);
    }
    if (id.llmInvalid > 0) {
      lines.push(`   LLM invalid:    ${fmt(id.llmInvalid)}  (invalid name returned)`);
    }
  }

  return lines.join("\n");
}

function fmt(n: number): string {
  return n.toLocaleString("en-US").padStart(8);
}
