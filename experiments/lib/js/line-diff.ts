/**
 * Line diff + line tokenizer for the measurement harness — moved verbatim
 * from the TS pipeline's src/rename/diff-reconcile.ts at the cutover
 * (docs/rust-port/19-cutover.md). The pipeline is the Rust binary now; these
 * are the harness's own copies, owned here: `computeNormalDiff` is the
 * text-level source for experiments/lib/diff.ts, `tokenizeLine` the
 * identifier tokenizer the `eval diff` ledger (exp055 real-ledger.ts) pairs
 * lines with. Both keep their pre-cutover bytes: every KPI card on record
 * was computed by exactly this code.
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const DIFF_MAX_BUFFER = 512 * 1024 * 1024;

/**
 * Line-diff two texts with the system `diff` (normal format). An
 * in-process Myers diff chokes on 370k-line bundles; the two legs are
 * ~95% identical so `diff` is fast.
 *
 * Both texts are CRLF-normalized first: a prior file checked out with
 * autocrlf against LF babel-generator output would otherwise differ on
 * every line (trailing \r), collapsing the whole file into non-noise.
 * Throws on a genuine `diff` failure (missing binary, oversized output);
 * callers treat this optional pass's failure as skip, not fatal.
 */
export function computeNormalDiff(priorText: string, newText: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "humanify-reconcile-"));
  try {
    const priorPath = path.join(dir, "prior.js");
    const newPath = path.join(dir, "new.js");
    fs.writeFileSync(priorPath, priorText.replace(/\r\n/g, "\n"));
    fs.writeFileSync(newPath, newText.replace(/\r\n/g, "\n"));
    const proc = spawnSync("diff", [priorPath, newPath], {
      encoding: "utf-8",
      maxBuffer: DIFF_MAX_BUFFER
    });
    // diff exits 0 (identical) or 1 (differences); anything else — including
    // a spawn failure (status null, e.g. no `diff` on PATH) — is an error.
    if (proc.status !== 0 && proc.status !== 1) {
      const detail = proc.error?.message || proc.stderr || "unknown error";
      throw new Error(`diff failed (status ${proc.status}): ${detail}`);
    }
    return proc.stdout;
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Line tokenizer
// ---------------------------------------------------------------------------

export interface LineToken {
  kind: "ident" | "text";
  text: string;
  /** 0-based column of the token's first character. */
  col: number;
}

const IDENT_START = /[A-Za-z_$]/;
const IDENT_CONT = /[A-Za-z0-9_$]/;
const NUMBER_CONT = /[0-9A-Za-z_$.]/;

/**
 * Words kept verbatim so a keyword change (`return` → `throw`) reads as a
 * genuine change. Contextual keywords (async/of/get/set/static/let) are
 * included: treating them as opaque only costs recall, never precision.
 */
const RESERVED_TOKEN_WORDS = new Set([
  ..."break case catch class const continue debugger default delete do else enum export extends false finally for function if import in instanceof new null return super switch this throw true try typeof var void while with yield".split(
    " "
  ),
  "let",
  "static",
  "async",
  "await",
  "of",
  "get",
  "set"
]);

/** Tokens after which a `/` is division, not the start of a regex. */
const DIVISION_PRECEDERS = new Set([
  ")",
  "]",
  "this",
  "true",
  "false",
  "null",
  "super"
]);

interface TokenizerState {
  line: string;
  i: number;
  tokens: LineToken[];
  /** Open template contexts: "quasi" = inside template text, a number =
   * brace depth of a `${}` expression. Empty = top-level code. */
  frames: Array<"quasi" | number>;
  /** Last non-whitespace token, for the regex/division heuristic. */
  prev: LineToken | null;
  failed: boolean;
}

function emit(
  st: TokenizerState,
  kind: LineToken["kind"],
  start: number
): void {
  const token = { kind, text: st.line.slice(start, st.i), col: start };
  st.tokens.push(token);
  if (token.text.trim().length > 0) st.prev = token;
}

function scanSimpleString(st: TokenizerState, quote: string): void {
  const start = st.i;
  st.i++;
  while (st.i < st.line.length) {
    const ch = st.line[st.i];
    if (ch === "\\") {
      st.i += 2;
      continue;
    }
    if (ch === quote) {
      st.i++;
      emit(st, "text", start);
      return;
    }
    st.i++;
  }
  st.failed = true; // unterminated (or line-continuation) — not self-contained
}

function scanQuasi(st: TokenizerState): void {
  const start = st.i;
  while (st.i < st.line.length) {
    const ch = st.line[st.i];
    if (ch === "\\") {
      st.i += 2;
      continue;
    }
    if (ch === "`") {
      st.i++;
      emit(st, "text", start);
      st.frames.pop();
      return;
    }
    if (ch === "$" && st.line[st.i + 1] === "{") {
      st.i += 2;
      emit(st, "text", start);
      st.frames.push(0);
      return;
    }
    st.i++;
  }
  st.failed = true; // template continues on the next line
}

function scanSlash(st: TokenizerState): void {
  const next = st.line[st.i + 1];
  if (next === "/") {
    const start = st.i;
    st.i = st.line.length;
    emit(st, "text", start); // line comment: rest is opaque
    return;
  }
  if (next === "*") {
    const end = st.line.indexOf("*/", st.i + 2);
    if (end === -1) {
      st.failed = true; // block comment continues past the line
      return;
    }
    const start = st.i;
    st.i = end + 2;
    emit(st, "text", start);
    return;
  }
  if (isDivisionContext(st.prev)) {
    const start = st.i;
    st.i++;
    emit(st, "text", start);
    return;
  }
  scanRegex(st);
}

function isDivisionContext(prev: LineToken | null): boolean {
  if (!prev) return false;
  if (prev.kind === "ident") return true;
  if (DIVISION_PRECEDERS.has(prev.text)) return true;
  return /^[0-9]/.test(prev.text); // number literal
}

function scanRegex(st: TokenizerState): void {
  const start = st.i;
  st.i++;
  let inClass = false;
  while (st.i < st.line.length) {
    const ch = st.line[st.i];
    if (ch === "\\") {
      st.i += 2;
      continue;
    }
    if (ch === "[") inClass = true;
    else if (ch === "]") inClass = false;
    else if (ch === "/" && !inClass) {
      st.i++;
      while (st.i < st.line.length && IDENT_CONT.test(st.line[st.i])) st.i++; // flags
      emit(st, "text", start);
      return;
    }
    st.i++;
  }
  st.failed = true; // unterminated regex — misdetection or broken line
}

function scanWord(st: TokenizerState): void {
  const start = st.i;
  st.i++;
  while (st.i < st.line.length && IDENT_CONT.test(st.line[st.i])) st.i++;
  const word = st.line.slice(start, st.i);
  emit(st, RESERVED_TOKEN_WORDS.has(word) ? "text" : "ident", start);
}

function scanNumber(st: TokenizerState): void {
  const start = st.i;
  st.i++;
  while (st.i < st.line.length && NUMBER_CONT.test(st.line[st.i])) st.i++;
  emit(st, "text", start);
}

function scanWhitespace(st: TokenizerState): void {
  const start = st.i;
  while (st.i < st.line.length && /\s/.test(st.line[st.i])) st.i++;
  emit(st, "text", start);
}

function scanBacktick(st: TokenizerState): void {
  const start = st.i;
  st.i++;
  emit(st, "text", start);
  st.frames.push("quasi");
}

function scanPunct(st: TokenizerState): void {
  const start = st.i;
  st.i++;
  emit(st, "text", start);
}

type CodeScanner = (st: TokenizerState) => void;

function pickScanner(ch: string): CodeScanner {
  if (ch === '"' || ch === "'") return (st) => scanSimpleString(st, ch);
  if (ch === "`") return scanBacktick;
  if (ch === "/") return scanSlash;
  if (IDENT_START.test(ch)) return scanWord;
  if (/[0-9]/.test(ch)) return scanNumber;
  if (/\s/.test(ch)) return scanWhitespace;
  if (ch === "{" || ch === "}") return (st) => stepBrace(st, ch);
  return scanPunct;
}

function stepCode(st: TokenizerState): void {
  pickScanner(st.line[st.i])(st);
}

function stepBrace(st: TokenizerState, ch: "{" | "}"): void {
  const top = st.frames[st.frames.length - 1];
  if (typeof top === "number") {
    if (ch === "{") st.frames[st.frames.length - 1] = top + 1;
    else if (top === 0)
      st.frames.pop(); // closes `${`, back to quasi text
    else st.frames[st.frames.length - 1] = top - 1;
  }
  const start = st.i;
  st.i++;
  emit(st, "text", start);
}

/**
 * Tokenize a single line of generator output into identifier and opaque
 * text tokens. Returns null when the line is not self-contained (open
 * string/template/comment) — callers must treat such lines as genuine.
 *
 * The failure direction is safe by construction: any misreading produces
 * token-stream mismatches or unresolvable positions, both of which make
 * the pass skip, never rename.
 */
export function tokenizeLine(line: string): LineToken[] | null {
  const st: TokenizerState = {
    line,
    i: 0,
    tokens: [],
    frames: [],
    prev: null,
    failed: false
  };
  while (st.i < line.length && !st.failed) {
    if (st.frames[st.frames.length - 1] === "quasi") scanQuasi(st);
    else stepCode(st);
  }
  if (st.failed || st.frames.length > 0) return null;
  return st.tokens;
}
