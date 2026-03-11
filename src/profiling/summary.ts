import type { ProfileReport } from "./types.js";

/**
 * Format a profile report as a human-readable summary for console output.
 */
export function formatProfileSummary(report: ProfileReport): string {
  const lines: string[] = [];
  const total = report.meta.totalDurationMs;

  lines.push("=== Performance Profile ===");
  lines.push(`Total duration: ${formatDuration(total)}`);
  lines.push("");

  // Stage breakdown
  if (report.stageSummaries.length > 0) {
    lines.push("Stage breakdown:");
    // Sort by duration descending
    const sorted = [...report.stageSummaries].sort((a, b) => b.durationMs - a.durationMs);
    for (const stage of sorted) {
      const pct = total > 0 ? ((stage.durationMs / total) * 100).toFixed(1) : "0.0";
      lines.push(`  ${stage.name.padEnd(25)} ${formatDuration(stage.durationMs).padStart(10)}  (${pct}%)${stage.spanCount > 1 ? `  [${stage.spanCount} spans]` : ""}`);
    }
    lines.push("");
  }

  // Rename timing percentiles
  if (report.renameTiming) {
    const rt = report.renameTiming;
    lines.push(`Rename timing (${rt.count} functions):`);
    lines.push(`  p50: ${formatDuration(rt.p50)}  p95: ${formatDuration(rt.p95)}  p99: ${formatDuration(rt.p99)}`);
    lines.push(`  min: ${formatDuration(rt.minMs)}  max: ${formatDuration(rt.maxMs)}`);
    lines.push("");
  }

  // Concurrency stats
  if (report.concurrencySnapshots.length > 0) {
    const snapshots = report.concurrencySnapshots;
    const avgInFlight = snapshots.reduce((s, c) => s + c.inFlight, 0) / snapshots.length;
    const maxInFlight = Math.max(...snapshots.map(c => c.inFlight));
    const avgReady = snapshots.reduce((s, c) => s + c.ready, 0) / snapshots.length;
    lines.push("Concurrency utilization:");
    lines.push(`  avg in-flight: ${avgInFlight.toFixed(1)}  max in-flight: ${maxInFlight}`);
    lines.push(`  avg ready: ${avgReady.toFixed(1)}  samples: ${snapshots.length}`);
    lines.push("");
  }

  return lines.join("\n");
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms.toFixed(0)}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60_000);
  const secs = ((ms % 60_000) / 1000).toFixed(0);
  return `${mins}m${secs}s`;
}
