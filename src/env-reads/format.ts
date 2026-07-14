/** Render an EnvReadsReport as terminal text or Markdown. */

import type { EnvLocation, EnvReadsReport, EnvSiteUse } from "./analyze.js";

function fmtLoc(loc: EnvLocation): string {
  return `${loc.file}:${loc.line}`;
}

function textSites(title: string, note: string, uses: EnvSiteUse[]): string[] {
  if (uses.length === 0) return [];
  const lines = [`${title} (${uses.length})${note}:`];
  for (const u of uses) lines.push(`  ${fmtLoc(u.loc)}  ${u.snippet}`);
  lines.push("");
  return lines;
}

function textReport(report: EnvReadsReport): string {
  const out: string[] = [
    `Environment variable reads — ${report.filesAnalyzed} file(s), ` +
      `${report.byVar.length} variable(s)`,
    ""
  ];
  if (report.byVar.length > 0) {
    out.push(`Variables (${report.byVar.length}):`);
    for (const v of report.byVar) {
      out.push(`  ${v.name}`);
      for (const loc of v.locations) out.push(`    ${fmtLoc(loc)}`);
    }
    out.push("");
  }
  out.push(
    ...textSites(
      "Dynamic keys",
      " — computed at runtime, not statically resolvable",
      report.dynamic
    )
  );
  out.push(...textSites("Whole-env / enumerated uses", "", report.enumerated));
  return `${out.join("\n").trimEnd()}\n`;
}

function mdSites(title: string, note: string, uses: EnvSiteUse[]): string[] {
  if (uses.length === 0) return [];
  const lines = [`## ${title} (${uses.length})`, ""];
  if (note) lines.push(note, "");
  for (const u of uses) lines.push(`- ${fmtLoc(u.loc)} — \`${u.snippet}\``);
  lines.push("");
  return lines;
}

function mdReport(report: EnvReadsReport): string {
  const out: string[] = [
    "# Environment variable reads",
    "",
    `${report.filesAnalyzed} file(s), ${report.byVar.length} variable(s).`,
    ""
  ];
  if (report.byVar.length > 0) {
    out.push(`## Variables (${report.byVar.length})`, "");
    for (const v of report.byVar) {
      out.push(`- \`${v.name}\` — ${v.locations.map(fmtLoc).join(", ")}`);
    }
    out.push("");
  }
  out.push(
    ...mdSites(
      "Dynamic keys",
      "Computed at runtime, not statically resolvable.",
      report.dynamic
    )
  );
  out.push(...mdSites("Whole-env / enumerated uses", "", report.enumerated));
  return `${out.join("\n").trimEnd()}\n`;
}

export function formatEnvReadsReport(
  report: EnvReadsReport,
  opts: { markdown?: boolean } = {}
): string {
  return opts.markdown ? mdReport(report) : textReport(report);
}
