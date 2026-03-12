/**
 * Progress rendering for the humanify pipeline.
 *
 * TtyRenderer: in-place updating dashboard at ~4Hz on stderr.
 * LineRenderer: periodic one-line updates for non-TTY / piped output.
 */

import type { PipelineStage, ProcessingMetrics } from "../llm/metrics.js";
import { formatDuration } from "../llm/metrics.js";

interface ProgressRenderer {
  /** Update the progress display with a new metrics snapshot */
  update(metrics: ProcessingMetrics): void;
  /** Print a one-off message that scrolls above the dashboard */
  message(text: string): void;
  /** Clean up: clear intervals, restore cursor, print final summary */
  finish(): void;
}

export function createProgressRenderer(opts: {
  tty: boolean;
}): ProgressRenderer {
  return opts.tty ? new TtyRenderer() : new LineRenderer();
}

const STAGE_LABELS: Record<PipelineStage, string> = {
  parsing: "Parsing",
  "building-graph": "Building dependency graph",
  renaming: "Renaming functions & modules",
  "library-params": "Renaming library parameters",
  "library-prefix": "Applying library prefixes",
  generating: "Generating output",
  done: "Done"
};

function formatNumber(n: number): string {
  return n.toLocaleString("en-US");
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function buildProgressBar(
  completed: number,
  total: number,
  width: number
): string {
  if (total === 0) return `[${"\u00b7".repeat(width)}]`;
  const ratio = Math.min(1, completed / total);
  const filled = Math.round(ratio * width);
  const empty = width - filled;
  return (
    "[" +
    "=".repeat(Math.max(0, filled - 1)) +
    (filled > 0 ? ">" : "") +
    "\u00b7".repeat(empty) +
    "]"
  );
}

function pct(completed: number, total: number): string {
  if (total === 0) return "  0.0%";
  const p = ((completed / total) * 100).toFixed(1);
  return p.padStart(6);
}

function computeEta(
  freshElapsed: number,
  totalCompleted: number,
  totalItems: number
): string {
  const pctDone = totalItems > 0 ? totalCompleted / totalItems : 0;
  const freshEta =
    pctDone > 0
      ? Math.round((freshElapsed * (1 - pctDone)) / pctDone)
      : undefined;
  return freshEta ? formatDuration(freshEta) : "...";
}

function buildLlmMetricLine(
  m: ProcessingMetrics,
  cols: number,
  barWidth: number
): string[] {
  const lines: string[] = [];

  // LLM stats
  const llmParts = [
    `${formatNumber(m.llm.completedCalls)} reqs`,
    `${m.llm.inFlightCalls} in-flight`,
    `${m.llm.failedCalls} failed`,
    `avg ${m.llm.avgResponseTimeMs}ms`
  ];
  if (m.llm.retries > 0) {
    llmParts.push(`${m.llm.retries} retries`);
  }
  lines.push(` LLM        ${llmParts.join(" \u00b7 ")}`);

  // Token stats
  if (m.llm.totalTokens) {
    const tokParts: string[] = [];
    if (m.llm.inputTokens && m.llm.outputTokens) {
      tokParts.push(
        `${formatTokens(m.llm.inputTokens)} in / ${formatTokens(m.llm.outputTokens)} out`
      );
    } else {
      tokParts.push(`${formatTokens(m.llm.totalTokens)} total`);
    }
    tokParts.push(`${formatNumber(m.tokensPerSecond)} tok/s`);
    lines.push(` Tokens      ${tokParts.join(" \u00b7 ")}`);
  }

  // Suppress unused parameter warning — barWidth kept for API consistency
  void cols;
  void barWidth;

  return lines;
}

class TtyRenderer implements ProgressRenderer {
  private lastMetrics: ProcessingMetrics | null = null;
  private lastLineCount = 0;
  private interval: ReturnType<typeof setInterval> | null = null;
  private finished = false;
  private pendingMessages: string[] = [];
  private lastStage: PipelineStage | null = null;

  constructor() {
    this.interval = setInterval(() => this.redraw(), 250);

    // Ensure cleanup on exit
    const cleanup = () => this.finish();
    process.once("exit", cleanup);
    process.once("SIGINT", () => {
      cleanup();
      process.exit(130);
    });
  }

  update(metrics: ProcessingMetrics): void {
    this.lastMetrics = metrics;

    // Detect stage change and emit a message
    if (this.lastStage !== null && metrics.stage !== this.lastStage) {
      const _label = STAGE_LABELS[metrics.stage] ?? metrics.stage;
      if (metrics.stage !== "done") {
        this.pendingMessages.push(
          ` \u2713 ${STAGE_LABELS[this.lastStage] ?? this.lastStage} (${formatDuration(metrics.elapsedMs)})`
        );
      }
    }
    this.lastStage = metrics.stage;
  }

  message(text: string): void {
    this.pendingMessages.push(text);
    this.redraw();
  }

  finish(): void {
    if (this.finished) return;
    this.finished = true;

    if (this.interval) {
      clearInterval(this.interval);
      this.interval = null;
    }

    // Clear current dashboard
    this.clearLines();

    // Print any pending messages
    for (const msg of this.pendingMessages) {
      process.stderr.write(`${msg}\n`);
    }
    this.pendingMessages = [];

    // Print final summary
    if (this.lastMetrics) {
      const m = this.lastMetrics;
      const elapsed = formatDuration(Date.now() - m.startTime);
      process.stderr.write(` \u2713 Done in ${elapsed}\n`);
      if (m.llm.totalTokens) {
        const tokenDetail =
          m.llm.inputTokens && m.llm.outputTokens
            ? `${formatTokens(m.llm.inputTokens)} in / ${formatTokens(m.llm.outputTokens)} out`
            : `${formatTokens(m.llm.totalTokens)} tokens`;
        const retryDetail =
          m.llm.retries > 0 ? ` | ${m.llm.retries} retries` : "";
        process.stderr.write(
          `   ${tokenDetail} | ${formatNumber(m.llm.completedCalls)} LLM calls | ${m.llm.failedCalls} failed${retryDetail}\n`
        );
      }
    }
  }

  private redraw(): void {
    if (this.finished || !this.lastMetrics) return;

    const m = this.lastMetrics;
    const lines: string[] = [];

    // Print pending messages above the dashboard
    if (this.pendingMessages.length > 0) {
      this.clearLines();
      for (const msg of this.pendingMessages) {
        process.stderr.write(`${msg}\n`);
      }
      this.pendingMessages = [];
      this.lastLineCount = 0;
    } else {
      this.clearLines();
    }

    const cols = process.stderr.columns ?? 80;
    const barWidth = Math.max(10, cols - 50);

    const freshElapsed = Date.now() - m.startTime;
    const elapsed = formatDuration(freshElapsed);
    const totalCompleted = m.functions.completed + m.moduleBindings.completed;
    const totalItems = m.functions.total + m.moduleBindings.total;
    const eta = computeEta(freshElapsed, totalCompleted, totalItems);

    const header = ` humanify`;
    const timing = `elapsed ${elapsed}  ETA ${eta}`;
    const pad = Math.max(1, cols - header.length - timing.length);
    lines.push(header + " ".repeat(pad) + timing);

    // Stage bar
    const stageLabel = STAGE_LABELS[m.stage] ?? m.stage;
    const stageLine = ` \u2500\u2500 ${stageLabel} `;
    lines.push(
      stageLine + "\u2500".repeat(Math.max(0, cols - stageLine.length - 1))
    );

    // Functions progress
    if (m.functions.total > 0) {
      const bar = buildProgressBar(
        m.functions.completed,
        m.functions.total,
        barWidth
      );
      lines.push(
        ` Functions  ${bar} ${formatNumber(m.functions.completed).padStart(8)} / ${formatNumber(m.functions.total).padEnd(8)} (${pct(m.functions.completed, m.functions.total)})`
      );
    }

    // Module bindings progress
    if (m.moduleBindings.total > 0) {
      const bar = buildProgressBar(
        m.moduleBindings.completed,
        m.moduleBindings.total,
        barWidth
      );
      lines.push(
        ` Modules    ${bar} ${formatNumber(m.moduleBindings.completed).padStart(8)} / ${formatNumber(m.moduleBindings.total).padEnd(8)} (${pct(m.moduleBindings.completed, m.moduleBindings.total)})`
      );
    }

    // LLM + token metric lines
    for (const line of buildLlmMetricLine(m, cols, barWidth)) {
      lines.push(line);
    }

    // Write all lines
    const output = `${lines.join("\n")}\n`;
    process.stderr.write(output);
    this.lastLineCount = lines.length;
  }

  private clearLines(): void {
    if (this.lastLineCount > 0) {
      // Move cursor up and clear each line
      process.stderr.write(`\x1b[${this.lastLineCount}A`);
      for (let i = 0; i < this.lastLineCount; i++) {
        process.stderr.write("\x1b[2K\n");
      }
      process.stderr.write(`\x1b[${this.lastLineCount}A`);
      this.lastLineCount = 0;
    }
  }
}

class LineRenderer implements ProgressRenderer {
  private lastEmitTime = 0;
  private lastStage: PipelineStage | null = null;
  private emitIntervalMs = 5000;

  update(metrics: ProcessingMetrics): void {
    const now = Date.now();
    const stageChanged =
      this.lastStage !== null && metrics.stage !== this.lastStage;
    this.lastStage = metrics.stage;

    if (stageChanged || now - this.lastEmitTime >= this.emitIntervalMs) {
      this.lastEmitTime = now;
      const line = this.formatLine(metrics);
      process.stderr.write(`${line}\n`);
    }
  }

  message(text: string): void {
    process.stderr.write(`${text}\n`);
  }

  finish(): void {
    // Nothing to clean up
  }

  private formatLine(m: ProcessingMetrics): string {
    const totalCompleted = m.functions.completed + m.moduleBindings.completed;
    const totalItems = m.functions.total + m.moduleBindings.total;
    const p =
      totalItems > 0 ? Math.round((totalCompleted / totalItems) * 100) : 0;
    const freshElapsed = Date.now() - m.startTime;
    const eta = computeEta(freshElapsed, totalCompleted, totalItems);

    let line = `[${p}%] ${formatNumber(m.functions.completed)}/${formatNumber(m.functions.total)} functions`;
    if (m.moduleBindings.total > 0) {
      line += ` | ${formatNumber(m.moduleBindings.completed)}/${formatNumber(m.moduleBindings.total)} modules`;
    }
    line += ` | LLM: ${m.llm.inFlightCalls} in-flight`;
    if (m.llm.retries > 0) {
      line += `, ${m.llm.retries} retries`;
    }
    if (m.llm.inputTokens && m.llm.outputTokens) {
      line += ` | ${formatTokens(m.llm.inputTokens)} in / ${formatTokens(m.llm.outputTokens)} out`;
    }
    line += ` | ETA: ${eta}`;

    return line;
  }
}
