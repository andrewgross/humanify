/**
 * Code selection for the LLM rename prompt.
 *
 * Functions over MAX_CODE_LINES used to be flat-truncated: the first 500
 * lines plus a marker. The identifier list is NOT capped, so bindings
 * declared past the cap were requested blind — the LLM omitted them
 * (minifier-reroll noise) or named them without context (asymmetric /
 * transfer-gap noise). See experiments/015-megafunction-truncation.
 *
 * selectFunctionCode replaces the flat cut with declaration-anchored line
 * windows: every identifier in the batch contributes a window around its
 * declaration line, the function header is always included (params and
 * signature context), and windows are merged and rendered with elision
 * markers. The shown context for a binding depends only on its own
 * neighborhood — stable across versions when that region is stable, which
 * is what lets both legs of a cross-version run agree.
 *
 * Anchors are input-file locs mapped onto generated lines. That mapping is
 * only exact when the generated line count equals the function's loc span
 * (true for all oversized functions measured on the Claude Code fixtures —
 * renames never change babel's line structure). When it does not hold, or
 * locs are missing, selection falls back to the legacy flat truncation.
 */
import { debug } from "../debug.js";

/** Maximum lines of function code shown to the LLM per request. */
export const MAX_CODE_LINES = 500;

/** Header window: function signature + opening context, always shown. */
const HEADER_LINES = 30;
/** Window padding around an anchored declaration line. */
const PAD_BEFORE = 20;
const PAD_AFTER = 40;
/** Padding floor when the budget forces windows to shrink. */
const MIN_PAD = 2;

export interface FunctionCodeSelection {
  /** Full generated function code. */
  code: string;
  /** Session id, for debug logging only. */
  sessionId: string;
  /** 1-based input-file line range of the function, when known. */
  fnStartLine?: number;
  fnEndLine?: number;
  /**
   * 1-based input-file declaration lines of the identifiers in this
   * request. Entries outside the function range (or undefined) fold into
   * the header window.
   */
  anchorStartLines?: Array<number | undefined>;
}

/** Legacy flat truncation — the fallback when anchors are unavailable. */
function truncateFlat(lines: string[], sessionId: string): string {
  debug.log(
    "processor",
    `Truncated function ${sessionId} from ${lines.length} to ${MAX_CODE_LINES} lines`
  );
  return `${lines.slice(0, MAX_CODE_LINES).join("\n")}\n  // ... [truncated] ...\n}`;
}

/**
 * Cap prompt CONTEXT code (e.g. the prior version of a close-matched
 * function) at the code budget. An uncapped multi-thousand-line prior
 * overflows the model context and 400-fails the whole batch — worse than
 * losing the past-cap part of the context.
 */
export function capContextCode(code: string, sessionId: string): string {
  const lines = code.split("\n");
  if (lines.length <= MAX_CODE_LINES) return code;
  debug.log(
    "processor",
    `Capped context code for ${sessionId} from ${lines.length} to ${MAX_CODE_LINES} lines`
  );
  return `${lines.slice(0, MAX_CODE_LINES).join("\n")}\n  // ... [truncated] ...\n}`;
}

interface Window {
  from: number; // 1-based inclusive
  to: number;
}

/** Merge sorted-by-from windows that overlap or touch. */
function mergeWindows(windows: Window[]): Window[] {
  const sorted = [...windows].sort((a, b) => a.from - b.from);
  const merged: Window[] = [];
  for (const w of sorted) {
    const last = merged[merged.length - 1];
    if (last && w.from <= last.to + 1) {
      if (w.to > last.to) last.to = w.to;
    } else {
      merged.push({ ...w });
    }
  }
  return merged;
}

function totalLines(windows: Window[]): number {
  return windows.reduce((sum, w) => sum + (w.to - w.from + 1), 0);
}

/** Build merged anchor windows with the given padding. */
function buildWindows(
  anchors: number[],
  lineCount: number,
  padBefore: number,
  padAfter: number
): Window[] {
  const windows: Window[] = [
    { from: 1, to: Math.min(HEADER_LINES, lineCount) },
    { from: lineCount, to: lineCount } // closing brace
  ];
  for (const a of anchors) {
    windows.push({
      from: Math.max(1, a - padBefore),
      to: Math.min(lineCount, a + padAfter)
    });
  }
  return mergeWindows(windows);
}

/** Render selected windows with elision markers between the gaps. */
function renderWindows(lines: string[], windows: Window[]): string {
  const parts: string[] = [];
  let prevEnd = 0;
  for (const w of windows) {
    if (w.from > prevEnd + 1) {
      parts.push(`  // … [lines ${prevEnd + 1}–${w.from - 1} omitted] …`);
    }
    for (let i = w.from; i <= w.to; i++) parts.push(lines[i - 1]);
    prevEnd = w.to;
  }
  return parts.join("\n");
}

/**
 * Map input-file anchor lines to function-relative generated lines.
 * Returns undefined when the loc→generated mapping cannot be trusted.
 */
function resolveAnchors(
  sel: FunctionCodeSelection,
  lineCount: number
): number[] | undefined {
  const { fnStartLine, fnEndLine, anchorStartLines } = sel;
  if (
    fnStartLine === undefined ||
    fnEndLine === undefined ||
    anchorStartLines === undefined
  ) {
    return undefined;
  }
  // The mapping "generated line = input line - fnStartLine + 1" is exact
  // only when the generated line count equals the loc span.
  if (fnEndLine - fnStartLine + 1 !== lineCount) return undefined;
  const anchors: number[] = [];
  for (const line of anchorStartLines) {
    if (line === undefined || line < fnStartLine || line > fnEndLine) continue;
    anchors.push(line - fnStartLine + 1);
  }
  return anchors;
}

/**
 * Select the code shown to the LLM for one rename request. Under the cap:
 * the full code, unchanged. Over the cap: declaration-anchored windows, or
 * legacy flat truncation when anchors are unavailable.
 */
export function selectFunctionCode(sel: FunctionCodeSelection): string {
  const lines = sel.code.split("\n");
  if (lines.length <= MAX_CODE_LINES) return sel.code;

  const anchors = resolveAnchors(sel, lines.length);
  if (anchors === undefined) return truncateFlat(lines, sel.sessionId);

  let padBefore = PAD_BEFORE;
  let padAfter = PAD_AFTER;
  let windows = buildWindows(anchors, lines.length, padBefore, padAfter);

  // Shrink padding until the selection fits the budget. Anchor lines
  // themselves always survive: a batch is ~10 identifiers, far below the
  // cap even at the padding floor.
  while (totalLines(windows) > MAX_CODE_LINES && padAfter > MIN_PAD) {
    padBefore = Math.max(MIN_PAD, Math.floor(padBefore / 2));
    padAfter = Math.max(MIN_PAD, Math.floor(padAfter / 2));
    windows = buildWindows(anchors, lines.length, padBefore, padAfter);
  }

  debug.log(
    "processor",
    `Windowed function ${sel.sessionId}: ${lines.length} lines → ` +
      `${totalLines(windows)} in ${windows.length} window(s) for ${anchors.length} anchor(s)`
  );
  return renderWindows(lines, windows);
}
