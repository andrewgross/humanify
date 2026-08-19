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
   * request, positionally aligned with `identifierNames`.
   *
   * An entry that is undefined, or outside the function's own loc range,
   * yields NO window on its own — the comment here used to claim such entries
   * "fold into the header window", which the code never did. See
   * `identifierNames` for what rescues them.
   */
  anchorStartLines?: Array<number | undefined>;
  /**
   * The identifiers being asked about, positionally aligned with
   * `anchorStartLines`. Used only to rescue an entry whose declaration loc is
   * unusable: the identifier is located in the GENERATED code and anchored
   * there instead.
   *
   * Without this, such a binding is silently dropped and the model is asked to
   * name a symbol absent from the code it was shown — measured at 4 of 171,756
   * on a cold run, one of which produced `unusedParameterPlaceholder` for a
   * value that is neither unused nor a placeholder.
   */
  identifierNames?: string[];
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
  lines: string[]
): number[] | undefined {
  const lineCount = lines.length;
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
  anchorStartLines.forEach((line, i) => {
    if (line !== undefined && line >= fnStartLine && line <= fnEndLine) {
      anchors.push(line - fnStartLine + 1);
      return;
    }
    // No usable declaration loc. Rather than drop the identifier — which asks
    // the model to name a symbol it cannot see — find it in the generated code.
    const name = sel.identifierNames?.[i];
    const found = name === undefined ? -1 : firstOccurrenceLine(lines, name);
    if (found > 0) {
      anchors.push(found);
      return;
    }
    debug.log(
      "processor",
      `Unanchored identifier ${name ?? `#${i}`} in ${sel.sessionId}: no ` +
        `declaration loc and not found in generated code — it will not be shown`
    );
  });
  return anchors;
}

/** 1-based line of the first whole-token occurrence of `name`, or -1. */
function firstOccurrenceLine(lines: string[], name: string): number {
  // `$` and `_` are identifier characters in JS but `$` is a NON-word
  // character to the regex engine, so `\b` is not an identifier boundary here.
  const token = new RegExp(
    `(?<![A-Za-z0-9_$])${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![A-Za-z0-9_$])`
  );
  for (let i = 0; i < lines.length; i++) {
    if (token.test(lines[i])) return i + 1;
  }
  return -1;
}

/**
 * Select the code shown to the LLM for one rename request. Under the cap:
 * the full code, unchanged. Over the cap: declaration-anchored windows, or
 * legacy flat truncation when anchors are unavailable.
 */
export function selectFunctionCode(sel: FunctionCodeSelection): string {
  const lines = sel.code.split("\n");
  if (lines.length <= MAX_CODE_LINES) return sel.code;

  const anchors = resolveAnchors(sel, lines);
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
