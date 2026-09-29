/**
 * The ground-truth predicate's canonical form (exp092).
 *
 * A function whose canonical form is byte-identical across the two
 * versions MUST be matched by the matcher — that is the harness's
 * ground-truth rule. Canonicalization is deliberately SIMPLE and
 * conservative in one direction: it may MISS ground truth (a genuinely
 * unchanged function whose spelling changed in a way below does not
 * enter the must set), but it never MANUFACTURES it (nothing that
 * canonicalizes equal is provably a different function).
 *
 * What it erases:
 * - the wrapper spelling: `function (a, b) { … }` ≡ `(a, b) => { … }`
 *   (the leading function head, with its optional `var name =` prefix;
 *   `async` is preserved);
 * - identifier NAMES: every non-keyword identifier is rewritten to its
 *   first-occurrence index (`_0`, `_1`, …), so a consistent renaming —
 *   a bijection — inside the slice is erased. String literal contents
 *   are opaque and never rewritten. A TEMPLATE literal is split the way
 *   the pipeline reads it (hash/serialize.rs): each quasi (the string
 *   text between the head/backtick and `${`, between `}` and the next
 *   `${` or the closing backtick) is opaque like a string, while each
 *   `${…}` interpolation is CODE — its identifiers are rewritten by the
 *   same bijection, so a human name interpolated in the prior and the
 *   minified name interpolated in the fresh build read identical (the
 *   2026-09-29 number-blur study: the old whole-template-as-one-string
 *   reading manufactured 4,386 phantom literal diffs at walk scale);
 * - indentation and intra-line whitespace runs (the two sides are both
 *   products of the same stage-6 formatter, but a function may move to
 *   a different nesting depth between versions).
 *
 * What it keeps: punctuation, operators, numbers, keywords, string and
 * quasi contents, REGEX literals (each scanned as one token, so a quote
 * character inside a regex cannot desync the string scanner — +55
 * phantoms in the same study), and line structure (per-line, so the
 * near/far classification can count differing lines).
 */

const KEYWORDS = new Set([
  "abstract",
  "as",
  "async",
  "await",
  "break",
  "case",
  "catch",
  "class",
  "const",
  "continue",
  "debugger",
  "default",
  "delete",
  "do",
  "else",
  "enum",
  "export",
  "extends",
  "false",
  "finally",
  "for",
  "from",
  "function",
  "get",
  "if",
  "implements",
  "import",
  "in",
  "instanceof",
  "interface",
  "let",
  "new",
  "null",
  "of",
  "return",
  "set",
  "static",
  "super",
  "switch",
  "this",
  "throw",
  "true",
  "try",
  "typeof",
  "var",
  "void",
  "while",
  "with",
  "yield"
]);

const FUNCTION_HEAD =
  /^(\s*(?:(?:var|let|const)\s+[A-Za-z_$][A-Za-z0-9_$]*\s*=\s*)?)(async\s+)?function\s*(\([^)]*\))\s*\{/;

/** Erase the wrapper spelling of the slice's leading function head. */
function normalizeWrapper(slice: string): string {
  const head = slice.match(FUNCTION_HEAD);
  if (!head) {
    return slice;
  }
  return slice.replace(FUNCTION_HEAD, "$1$2$3 => {");
}

const IDENTIFIER = /[A-Za-z_$][A-Za-z0-9_$]*/y;
const NUMBER = /[0-9][\w.]*/y;

/**
 * The keywords a regex literal may directly follow (`return /x/` is a
 * regex; `total / 2` is division). The value-vs-operator decision is
 * otherwise made on the last significant character emitted.
 */
const REGEX_AFTER_KEYWORDS = new Set([
  "await",
  "case",
  "delete",
  "do",
  "else",
  "in",
  "instanceof",
  "new",
  "of",
  "return",
  "throw",
  "typeof",
  "void",
  "yield"
]);

/** The index just past the string literal starting at `i`. */
function scanLiteralEnd(code: string, i: number): number {
  const quote = code[i];
  let j = i + 1;
  while (j < code.length) {
    const d = code[j];
    if (d === "\\") {
      j += 2;
      continue;
    }
    if (d === quote) {
      return j + 1;
    }
    // A non-template literal cannot span lines in parsed code; if the
    // scan hits a newline first, the literal ends there.
    if (d === "\n") {
      return j;
    }
    j += 1;
  }
  return j;
}

/**
 * The end of the regex literal starting at `i`: past its closing `/`
 * (unescaped, outside a character class) and the trailing flags, or -1
 * when no closing `/` follows on the line — the `/` is then division.
 */
function scanRegexEnd(code: string, i: number): number {
  const close = regexBodyEnd(code, i + 1);
  if (close < 0) {
    return -1;
  }
  let k = close + 1;
  while (k < code.length && /[a-z]/i.test(code[k])) {
    k += 1;
  }
  return k;
}

/**
 * The index of the regex body's closing `/` — unescaped, outside a
 * character class (`[` … `]`, where both delimiters are literal) — or -1
 * when a newline or the end of the code comes first.
 */
function regexBodyEnd(code: string, j: number): number {
  let inClass = false;
  while (j < code.length) {
    const r = code[j];
    if (r === "\\") {
      j += 2;
      continue;
    }
    if (r === "\n") {
      return -1;
    }
    if (inClass) {
      inClass = r !== "]";
    } else if (r === "[") {
      inClass = true;
    } else if (r === "/") {
      return j;
    }
    j += 1;
  }
  return -1;
}

/** Where the numeric literal starting at `i` ends (loose mode only). */
function scanNumberEnd(code: string, i: number): number {
  NUMBER.lastIndex = i;
  const m = NUMBER.exec(code);
  return m && m.index === i ? i + m[0].length : i + 1;
}

/**
 * One entered template literal (`tpl: true`, accumulating quasi text)
 * or one of its `${…}` interpolations (`tpl: false`, counting braces so
 * the interpolation closes at ITS `}`, not at an inner object's).
 */
interface Frame {
  tpl: boolean;
  quasi: string;
  depth: number;
}

/**
 * The scan state for one canonicalization: the output accumulated so
 * far, the identifier→index map, the loose flag, the last significant
 * character (the regex-vs-division decision), and the template-literal
 * nesting. Split out of the loop so each side stays under the
 * complexity ceiling.
 */
class Canonicalizer {
  private out = "";
  private readonly names = new Map<string, number>();
  private lastSignificant = "";
  private lastWord = "";
  private readonly frames: Frame[] = [{ tpl: false, quasi: "", depth: 0 }];

  constructor(private readonly loose: boolean) {}

  /**
   * Rewrite every non-keyword identifier to its first-occurrence index,
   * treating string contents — and each template literal's QUASI text —
   * as opaque, while every `${…}` interpolation is canonicalized as
   * code. Runs of spaces/tabs (outside strings) collapse to one;
   * newlines are kept (one blank line at most). With `loose`, numeric
   * literals blur to `#` and string/quasi contents to their length —
   * the tier-2 key.
   */
  run(code: string): string {
    let i = 0;
    while (i < code.length) {
      i = this.step(code, i);
    }
    return this.out;
  }

  /** Consume the token at `i` (or one opaque char) and return the next
   * index, appending its canonical spelling to the output. */
  private step(code: string, i: number): number {
    const frame = this.frames[this.frames.length - 1];
    if (frame.tpl) {
      return this.templateChar(code, i, frame);
    }
    const c = code[i];
    if (c === '"' || c === "'") {
      return this.stringLiteral(code, i);
    }
    if (c === "`") {
      this.frames.push({ tpl: true, quasi: "", depth: 0 });
      return i + 1;
    }
    if (c === "/") {
      return this.slash(code, i);
    }
    if (c === "{" || c === "}") {
      return this.brace(i, frame, c);
    }
    if (c === " " || c === "\t") {
      this.out += " ";
      return i + 1;
    }
    if (c === "\n" || c === "\r") {
      return this.newlineRun(code, i);
    }
    const token = this.wordOrNumber(code, i);
    return token ?? this.punct(i, c);
  }

  /**
   * A word or numeric token at `i`, if one starts there: an identifier's
   * rewritten spelling (a keyword verbatim, any other its
   * first-occurrence index — and the token that decides regex vs
   * division after it), or a numeric literal — ONE token, both forms:
   * its tail (`1024n`'s BigInt suffix) is part of the literal, never a
   * fresh identifier of the bijection (strict keeps the spelling
   * verbatim; loose blurs it to `#`). Null when neither starts there.
   */
  private wordOrNumber(code: string, i: number): number | null {
    IDENTIFIER.lastIndex = i;
    const m = IDENTIFIER.exec(code);
    if (m !== null && m.index === i) {
      const word = m[0];
      this.out += this.rewrittenIdentifier(word);
      this.lastSignificant = word[word.length - 1];
      this.lastWord = word;
      return i + word.length;
    }
    if (code[i] >= "0" && code[i] <= "9") {
      const end = scanNumberEnd(code, i);
      this.out += this.loose ? "#" : code.slice(i, end);
      this.lastSignificant = code[end - 1];
      this.lastWord = "";
      return end;
    }
    return null;
  }

  /** One opaque punctuation/operator character. */
  private punct(i: number, c: string): number {
    this.out += c;
    this.lastSignificant = c;
    this.lastWord = "";
    return i + 1;
  }

  /**
   * One character of quasi text inside a template literal: escapes pair,
   * a backtick ends the template, `${` starts an interpolation (the
   * quasi so far is string content), and anything else accumulates.
   */
  private templateChar(code: string, i: number, frame: Frame): number {
    const c = code[i];
    if (c === "\\") {
      frame.quasi += code.slice(i, i + 2);
      return i + 2;
    }
    if (c === "`") {
      this.emitQuasi(frame);
      this.frames.pop();
      this.lastSignificant = "`";
      this.lastWord = "";
      return i + 1;
    }
    if (c === "$" && code[i + 1] === "{") {
      this.emitQuasi(frame);
      this.frames.push({ tpl: false, quasi: "", depth: 0 });
      this.out += "${";
      this.lastSignificant = "{";
      this.lastWord = "";
      return i + 2;
    }
    frame.quasi += c;
    return i + 1;
  }

  /** The quasi accumulated so far: verbatim between «…», or `#s<len>`
   * when loose (a string, blurred like every other string). */
  private emitQuasi(frame: Frame): void {
    this.out += this.loose ? `#s${frame.quasi.length}` : `«${frame.quasi}»`;
    this.lastSignificant = "»";
    this.lastWord = "";
    frame.quasi = "";
  }

  /**
   * A brace in code position: `{` opens a nesting level inside the
   * current frame; `}` closes one — or, at depth 0 inside a template
   * interpolation, ENDS the interpolation (back to quasi text).
   */
  private brace(i: number, frame: Frame, c: string): number {
    if (c === "{") {
      frame.depth += 1;
      return this.punct(i, c);
    }
    if (frame.depth === 0 && this.frames.length > 1) {
      this.frames.pop();
    } else if (frame.depth > 0) {
      frame.depth -= 1;
    }
    return this.punct(i, "}");
  }

  /**
   * A `/`: division when it follows a value (an identifier, a literal, a
   * closing paren — the last significant character emitted), except
   * after a keyword a regex follows (`return /x/`); otherwise the whole
   * regex literal is consumed as one verbatim token (so a quote inside
   * it is not mistaken for a string opener), or the `/` stands alone
   * when no closing `/` is found on the line.
   */
  private slash(code: string, i: number): number {
    const afterValue =
      /[\w$)"'`]/.test(this.lastSignificant) &&
      !REGEX_AFTER_KEYWORDS.has(this.lastWord);
    const end = afterValue ? -1 : scanRegexEnd(code, i);
    if (end < 0) {
      return this.punct(i, "/");
    }
    const raw = code.slice(i, end);
    this.out += raw;
    this.lastSignificant = raw[raw.length - 1];
    this.lastWord = "";
    return end;
  }

  /** A string literal, verbatim — or `#s<length>` when loose. */
  private stringLiteral(code: string, i: number): number {
    const end = scanLiteralEnd(code, i);
    this.out += this.loose ? `#s${end - i - 2}` : code.slice(i, end);
    this.lastSignificant = '"';
    this.lastWord = "";
    return end;
  }

  /** One newline for a whole run of them. */
  private newlineRun(code: string, i: number): number {
    this.out += "\n";
    let j = i + 1;
    while (j < code.length && (code[j] === "\n" || code[j] === "\r")) {
      j += 1;
    }
    return j;
  }

  /** A keyword verbatim; any other identifier its first-occurrence
   * index. */
  private rewrittenIdentifier(word: string): string {
    if (KEYWORDS.has(word)) {
      return word;
    }
    let idx = this.names.get(word);
    if (idx === undefined) {
      idx = this.names.size;
      this.names.set(word, idx);
    }
    return `_${idx}`;
  }
}

/** The slice's canonical form — the ground-truth comparison key. */
export function canonical(slice: string): string {
  return new Canonicalizer(false).run(normalizeWrapper(slice));
}

/**
 * The LOOSE canonical form — the tier-2 "should match" key: the same
 * normalization, plus literals blurred (numbers to `#`, string contents
 * to their LENGTH, mirroring the pipeline's structural hash policy of
 * string-length + number-magnitude only). Two functions that differ
 * ONLY in literals — the classic unchanged-through-a-rebuild when the
 * minifier re-picks constants — are loose-equal. Advisory: two
 * genuinely different functions that happen to differ only in literals
 * also read loose-equal, which is why this tier is reported separately
 * and never hard-fails.
 */
export function canonicalLoose(slice: string): string {
  return new Canonicalizer(true).run(normalizeWrapper(slice));
}

/**
 * The symmetric line-multiset distance between two canonical forms —
 * the near/far classifier's edit mass: how many LINES (with multiplicity)
 * exist on one side but not the other.
 */
export function lineDiffMass(a: string, b: string): number {
  const ma = lineCounts(a);
  const mb = lineCounts(b);
  let mass = 0;
  for (const [k, v] of ma) {
    mass += Math.max(0, v - (mb.get(k) ?? 0));
  }
  for (const [k, v] of mb) {
    mass += Math.max(0, v - (ma.get(k) ?? 0));
  }
  return mass;
}

function lineCounts(text: string): Map<string, number> {
  const m = new Map<string, number>();
  for (const line of text.split("\n")) {
    m.set(line, (m.get(line) ?? 0) + 1);
  }
  return m;
}
