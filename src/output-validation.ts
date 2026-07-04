import { parseSync } from "@babel/core";

/**
 * Details of a generated-output parse failure.
 * Produced when the code we are about to write to disk is not valid JavaScript
 * (e.g., a rename introduced a duplicate declaration or a reserved word).
 */
export interface OutputParseFailure {
  /** First line of the parser error message */
  message: string;
  /** 1-based line of the failure, when known */
  line?: number;
  /** 0-based column of the failure, when known */
  column?: number;
  /** Source lines around the failure, with the failing line marked */
  excerpt?: string;
}

const EXCERPT_CONTEXT_LINES = 2;
const MIN_LINE_NUMBER_WIDTH = 2;

/**
 * Re-parses generated output before it is written to disk.
 * Returns null when the code parses cleanly, or failure details when it
 * does not. Renames are applied to the AST and never re-checked by Babel,
 * so this is the last line of defense against emitting unparseable output.
 */
export function validateOutputParses(code: string): OutputParseFailure | null {
  try {
    const ast = parseSync(code, {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    });
    if (!ast) {
      return { message: "Parser returned no AST for generated output" };
    }
    return null;
  } catch (err) {
    return describeParseError(err, code);
  }
}

function describeParseError(err: unknown, code: string): OutputParseFailure {
  const error = err as {
    message?: string;
    loc?: { line?: number; column?: number };
  };
  const rawMessage = error.message ?? String(err);
  const message = rawMessage.split("\n")[0];

  const { line, column } = extractLocation(error, rawMessage);
  const failure: OutputParseFailure = { message };
  if (line !== undefined) {
    failure.line = line;
    failure.excerpt = buildExcerpt(code, line);
  }
  if (column !== undefined) failure.column = column;
  return failure;
}

function extractLocation(
  error: { loc?: { line?: number; column?: number } },
  message: string
): { line?: number; column?: number } {
  if (typeof error.loc?.line === "number") {
    return { line: error.loc.line, column: error.loc.column };
  }
  // Babel messages end with "(line:column)"
  const match = message.match(/\((\d+):(\d+)\)/);
  if (match) {
    return { line: Number(match[1]), column: Number(match[2]) };
  }
  return {};
}

/** Renders the failing line with surrounding context, code-frame style. */
function buildExcerpt(code: string, failureLine: number): string {
  const lines = code.split("\n");
  const first = Math.max(1, failureLine - EXCERPT_CONTEXT_LINES);
  const last = Math.min(lines.length, failureLine + EXCERPT_CONTEXT_LINES);
  const width = Math.max(MIN_LINE_NUMBER_WIDTH, String(last).length);

  const rendered: string[] = [];
  for (let lineNo = first; lineNo <= last; lineNo++) {
    const marker = lineNo === failureLine ? "> " : "  ";
    rendered.push(
      `${marker}${String(lineNo).padStart(width)} | ${lines[lineNo - 1]}`
    );
  }
  return rendered.join("\n");
}
