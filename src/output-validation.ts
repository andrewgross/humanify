import { parseSync } from "@babel/core";
import type { Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import { traverse } from "./babel-utils.js";

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

/**
 * Facts a rename pass must preserve, measured on the input AST before any
 * renames. Renaming bindings can neither change which names the file
 * observes as free (capture / left-behind reference) nor how many
 * bindings exist (split / merged declaration).
 */
export interface SemanticBaseline {
  /** Names the file references without a binding (Program-scope globals) */
  freeNames: Set<string>;
  /** Total binding count across all scopes */
  totalBindingCount: number;
}

/** A violated rename invariant found by comparing output to the baseline. */
export interface OutputSemanticFailure {
  message: string;
  /** Names that were bound before but are free in the output */
  addedFreeNames?: string[];
  /** Names that were free before but are bound in the output (capture) */
  removedFreeNames?: string[];
  bindingCountBefore?: number;
  bindingCountAfter?: number;
}

const EXCERPT_CONTEXT_LINES = 2;
const MIN_LINE_NUMBER_WIDTH = 2;
const FAILURE_NAME_SAMPLE = 5;

/** Measure free names and total binding count for an AST. */
function measureSemantics(ast: t.Node): SemanticBaseline {
  let freeNames = new Set<string>();
  let totalBindingCount = 0;
  const seenScopes = new Set<Scope>();
  traverse(ast, {
    Program(path) {
      freeNames = new Set(Object.keys(path.scope.globals ?? {}));
    },
    Scopable(path) {
      const scope = path.scope;
      if (seenScopes.has(scope)) return;
      seenScopes.add(scope);
      totalBindingCount += Object.keys(scope.bindings).length;
    }
  });
  return { freeNames, totalBindingCount };
}

/**
 * Capture the semantic baseline of the (parsed, pre-rename) input.
 * Must run before any rename mutates the AST or its scope info.
 */
export function captureSemanticBaseline(ast: t.Node): SemanticBaseline {
  return measureSemantics(ast);
}

function compareSemantics(
  baseline: SemanticBaseline,
  after: SemanticBaseline
): OutputSemanticFailure | undefined {
  const added = [...after.freeNames]
    .filter((n) => !baseline.freeNames.has(n))
    .sort();
  const removed = [...baseline.freeNames]
    .filter((n) => !after.freeNames.has(n))
    .sort();
  const countChanged = after.totalBindingCount !== baseline.totalBindingCount;
  if (added.length === 0 && removed.length === 0 && !countChanged) {
    return undefined;
  }

  const parts: string[] = [];
  if (removed.length > 0) {
    parts.push(
      `${removed.length} free name(s) became bound (capture): ` +
        removed.slice(0, FAILURE_NAME_SAMPLE).join(", ")
    );
  }
  if (added.length > 0) {
    parts.push(
      `${added.length} name(s) became free (left-behind reference): ` +
        added.slice(0, FAILURE_NAME_SAMPLE).join(", ")
    );
  }
  if (countChanged) {
    parts.push(
      `binding count changed ${baseline.totalBindingCount} → ` +
        `${after.totalBindingCount} (split or merged declaration)`
    );
  }

  const failure: OutputSemanticFailure = {
    message: `Rename semantic invariants violated: ${parts.join("; ")}`
  };
  if (added.length > 0) failure.addedFreeNames = added;
  if (removed.length > 0) failure.removedFreeNames = removed;
  if (countChanged) {
    failure.bindingCountBefore = baseline.totalBindingCount;
    failure.bindingCountAfter = after.totalBindingCount;
  }
  return failure;
}

/**
 * Re-parses generated output and, when a baseline is provided, checks the
 * rename invariants held. One parse serves both checks. This is the last
 * line of defense before writing output: renames mutate the AST directly
 * and are never re-checked by Babel, and a capture (C1) or binding split
 * (C2) produces output that PARSES cleanly but misbehaves at runtime.
 */
export function validateOutput(
  code: string,
  baseline?: SemanticBaseline
): {
  parseFailure?: OutputParseFailure;
  semanticFailure?: OutputSemanticFailure;
} {
  let ast: t.Node | null;
  try {
    ast = parseSync(code, {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    });
  } catch (err) {
    return { parseFailure: describeParseError(err, code) };
  }
  if (!ast) {
    return {
      parseFailure: { message: "Parser returned no AST for generated output" }
    };
  }
  if (!baseline) return {};
  const semanticFailure = compareSemantics(baseline, measureSemantics(ast));
  return semanticFailure ? { semanticFailure } : {};
}

/**
 * Parse-only validation of generated output. Returns null when the code
 * parses cleanly, or failure details when it does not.
 */
export function validateOutputParses(code: string): OutputParseFailure | null {
  return validateOutput(code).parseFailure ?? null;
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
