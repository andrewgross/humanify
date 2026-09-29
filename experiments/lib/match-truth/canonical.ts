/**
 * The ground-truth predicate's canonical form (exp092).
 *
 * A function whose canonical form is byte-identical across the two
 * versions MUST be matched by the matcher — that is the harness's
 * ground-truth rule. Canonicalization is deliberately SIMPLE and
 * conservative in one direction: it may MISS ground truth (a genuinely
 * unchanged function whose spelling changed in a way below does not
 * enter the must set), but it never MANUFACTURES it (nothing that
 * canonicalizes equal is provably a different function... with the one
 * documented exception: identifier-shaped contents inside a template
 * literal are treated as opaque strings, and a regex literal containing
 * a quote character can confuse the string tracker).
 *
 * What it erases:
 * - the wrapper spelling: `function (a, b) { … }` ≡ `(a, b) => { … }`
 *   (the leading function head, with its optional `var name =` prefix;
 *   `async` is preserved);
 * - identifier NAMES: every non-keyword identifier is rewritten to its
 *   first-occurrence index (`_0`, `_1`, …), so a consistent renaming —
 *   a bijection — inside the slice is erased. String and template
 *   literal contents are opaque and never rewritten;
 * - indentation and intra-line whitespace runs (the two sides are both
 *   products of the same stage-6 formatter, but a function may move to
 *   a different nesting depth between versions).
 *
 * What it keeps: punctuation, operators, numbers, keywords, string
 * contents, line structure (per-line, so the near/far classification
 * can count differing lines).
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

/** The index just past the string/template literal starting at `i`. */
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
    if (quote !== "`" && d === "\n") {
      return j;
    }
    j += 1;
  }
  return j;
}

/** Where the numeric literal starting at `i` ends (loose mode only). */
function scanNumberEnd(code: string, i: number): number {
  NUMBER.lastIndex = i;
  const m = NUMBER.exec(code);
  return m && m.index === i ? i + m[0].length : i + 1;
}

/**
 * The scan state for one canonicalization: the output accumulated so
 * far, the identifier→index map, and the loose flag. Split out of the
 * loop so each side stays under the complexity ceiling.
 */
class Canonicalizer {
  private out = "";
  private readonly names = new Map<string, number>();

  constructor(private readonly loose: boolean) {}

  /**
   * Rewrite every non-keyword identifier to its first-occurrence index,
   * treating string and template contents as opaque. Runs of spaces/tabs
   * (outside strings) collapse to one; newlines are kept (one blank
   * line at most). With `loose`, numeric literals blur to `#` and
   * string contents to their length — the tier-2 key.
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
    const c = code[i];
    if (c === '"' || c === "'" || c === "`") {
      return this.literal(code, i);
    }
    if (c === " " || c === "\t") {
      this.out += " ";
      return i + 1;
    }
    if (c === "\n" || c === "\r") {
      return this.newlineRun(code, i);
    }
    IDENTIFIER.lastIndex = i;
    const m = IDENTIFIER.exec(code);
    if (m && m.index === i) {
      this.out += this.rewrittenIdentifier(m[0]);
      return i + m[0].length;
    }
    if (this.loose && c >= "0" && c <= "9") {
      this.out += "#";
      return scanNumberEnd(code, i);
    }
    this.out += c;
    return i + 1;
  }

  /** A string/template literal, verbatim — or `#s<length>` when loose. */
  private literal(code: string, i: number): number {
    const end = scanLiteralEnd(code, i);
    this.out += this.loose ? `#s${end - i - 2}` : code.slice(i, end);
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
