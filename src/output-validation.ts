import type { Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import {
  computeRenameInvariantSignature,
  serializePathTokens
} from "./analysis/structural-hash.js";
import { parseSourceAst, traverse } from "./babel-utils.js";

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
interface FreeNameMeasure {
  /** Names the file references without a binding (Program-scope globals) */
  freeNames: Set<string>;
  /** Total binding count across all scopes */
  totalBindingCount: number;
}

export interface SemanticBaseline extends FreeNameMeasure {
  /**
   * Rename-invariant structural signature of the whole program (binding
   * identifiers become order-keyed slots; literals, operators, property keys,
   * and free names are verbatim). It is unchanged after renaming iff the
   * rename pass altered nothing but binding names — so a mismatch means the
   * output is NOT a pure rename of the input. Fully static, so it validates
   * artifacts that can't be executed (e.g. Bun bytecode decompilations).
   */
  structuralSignature: string;
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

/** Compute the whole-program structural signature (throws if no Program). */
function programStructuralSignature(ast: t.Node): string {
  let signature: string | undefined;
  traverse(ast, {
    Program(path) {
      signature = computeRenameInvariantSignature(path);
      path.stop();
    }
  });
  if (signature === undefined) {
    throw new Error("cannot compute structural signature: no Program node");
  }
  return signature;
}

/** Measure free names and total binding count for an AST. */
function measureSemantics(ast: t.Node): FreeNameMeasure {
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
  return {
    ...measureSemantics(ast),
    structuralSignature: programStructuralSignature(ast)
  };
}

/**
 * Hermetic rename-only invariant: recompute the structural signature on the
 * post-rename AST (before generation, so no re-parse noise) and compare it to
 * the pre-rename baseline. A mismatch means the rename pass changed something
 * other than binding names — a dropped statement, flipped operator, altered
 * literal, etc. — which is a bug, not a rename.
 */
export function checkStructuralInvariant(
  ast: t.Node,
  baseline: SemanticBaseline,
  /** The pre-rename source. When given, the failure NAMES the first diverging
   *  token instead of only asserting that one exists. */
  originalCode?: string
): OutputSemanticFailure | undefined {
  if (programStructuralSignature(ast) === baseline.structuralSignature) {
    return undefined;
  }
  return {
    message:
      "Rename changed program structure beyond identifier names " +
      "(structural signature mismatch): the output is not a pure rename of " +
      "the input — a statement, literal, operator, or property access differs." +
      divergenceSuffix(ast, originalCode)
  };
}

/** The localised divergence, as a message suffix, or "" when unavailable. */
function divergenceSuffix(ast: t.Node, originalCode?: string): string {
  if (!originalCode) return "";
  const detail = describeStructuralDivergence(ast, originalCode);
  return detail ? `\n${detail}` : "";
}

function compareSemantics(
  baseline: FreeNameMeasure,
  after: FreeNameMeasure
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
 * Structural signature recomputed on the FRESH parse of the output.
 * The pre-generation checkStructuralInvariant resolves identifiers
 * through binding caches captured before any rename, so a rename-
 * introduced capture — an occurrence now resolving to a DIFFERENT
 * binding because two bindings share a name — is invisible to it. A
 * bound→bound capture also changes neither the free-name set nor the
 * binding count. Only a cold re-parse resolves names the way a runtime
 * would, so this comparison is the one gate that catches captures.
 */
function checkResolvedSignature(
  ast: t.Node,
  baseline: SemanticBaseline,
  originalCode?: string
): OutputSemanticFailure | undefined {
  if (programStructuralSignature(ast) === baseline.structuralSignature) {
    return undefined;
  }
  return {
    message:
      "Rename changed how identifiers resolve (structural signature " +
      "mismatch on re-parse): a renamed binding captures references of " +
      "another binding, or structure changed beyond binding names." +
      divergenceSuffix(ast, originalCode)
  };
}

/**
 * Re-parses generated output and, when a baseline is provided, checks the
 * rename invariants held. One parse serves all checks. This is the last
 * line of defense before writing output: renames mutate the AST directly
 * and are never re-checked by Babel, and a capture (C1) or binding split
 * (C2) produces output that PARSES cleanly but misbehaves at runtime.
 */
export function validateOutput(
  code: string,
  baseline?: SemanticBaseline,
  /** Pre-rename source, so a capture failure can name the diverging token. */
  originalCode?: string
): {
  parseFailure?: OutputParseFailure;
  semanticFailure?: OutputSemanticFailure;
} {
  let ast: t.Node | null;
  try {
    ast = parseSourceAst(code);
  } catch (err) {
    return { parseFailure: describeParseError(err, code) };
  }
  if (!ast) {
    return {
      parseFailure: { message: "Parser returned no AST for generated output" }
    };
  }
  if (!baseline) return {};
  const semanticFailure =
    compareSemantics(baseline, measureSemantics(ast)) ??
    checkResolvedSignature(ast, baseline, originalCode);
  return semanticFailure ? { semanticFailure } : {};
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

/** Tokens of context shown either side of the first divergence. */
const DIVERGENCE_CONTEXT = 6;

/**
 * WHAT differs between the original source and the post-rename AST, in the
 * same token stream the structural signature hashes.
 *
 * Called only after `checkStructuralInvariant` or `checkResolvedSignature` has
 * already failed. The failure message they produce ("a statement, literal,
 * operator, or property access differs") names nothing, and every
 * investigation of it has therefore started from zero — including the standing
 * `runtime.js` failure, which has been red on 2 of the 4 eval pairs since
 * exp058 with no one able to say what diverged.
 *
 * Re-parses the ORIGINAL SOURCE rather than keeping a baseline stream: by the
 * time this runs, the pre-rename AST has been mutated in place, so those tokens
 * are gone. Doing it only on failure also keeps the success path free — holding
 * a token stream per file is unaffordable on a 14 MB bundle.
 *
 * The serialization options MUST match `computeRenameInvariantSignature`
 * exactly — literals preserved, private names slotted. A diff taken with
 * different settings would disagree with the check it is explaining, and would
 * name the wrong token as the cause.
 */
export function describeStructuralDivergence(
  afterAst: t.Node,
  originalCode: string
): string | undefined {
  // parseSourceAst THROWS on invalid input rather than returning null, and a
  // diagnostic that crashes replaces the failure it was explaining with its own
  // stack trace. It is only ever called on a path that is already failing.
  let originalAst: t.Node | null = null;
  try {
    originalAst = parseSourceAst(originalCode);
  } catch {
    return "could not re-parse the original source to localise the divergence";
  }
  const before = programTokens(originalAst);
  const after = programTokens(afterAst);
  if (!before || !after) {
    return "could not recover the token streams to localise the divergence";
  }

  const firstDiff = firstDifferingIndex(before, after);
  if (firstDiff === undefined) return undefined;

  const window = (toks: string[]): string =>
    toks
      .slice(
        Math.max(0, firstDiff - DIVERGENCE_CONTEXT),
        firstDiff + DIVERGENCE_CONTEXT + 1
      )
      .join(" ");

  const beforeTok = firstDiff < before.length ? before[firstDiff] : "<end>";
  const afterTok = firstDiff < after.length ? after[firstDiff] : "<end>";
  const lengths =
    before.length === after.length
      ? `${before.length} tokens each`
      : `${before.length} tokens before vs ${after.length} after`;

  return (
    // Every line INDENTED: the eval harness records an `ERROR:` line together
    // with the indented block beneath it, and an unindented line ends the
    // block. Without this the explanation is dropped from the committed run
    // status and survives only in a gitignored .stdout.
    `  first divergence at token ${firstDiff} of ${lengths}\n` +
    `    original: ${JSON.stringify(beforeTok)}\n` +
    `    output:   ${JSON.stringify(afterTok)}\n` +
    `    original context: ${window(before)}\n` +
    `    output context:   ${window(after)}`
  );
}

/** The Program's token stream, or undefined when there is no Program. */
function programTokens(ast: t.Node | null): string[] | undefined {
  if (!ast) return undefined;
  let tokens: string[] | undefined;
  traverse(ast, {
    Program(path) {
      // MUST match computeRenameInvariantSignature exactly. If the diagnostic
      // serialized private names verbatim while the check slotted them, a file
      // with both a legitimate private rename and a real structural change
      // would have the private rename reported as the cause — a red herring
      // pointing at the one thing that is fine.
      tokens = serializePathTokens(path, {
        preserveLiterals: true,
        privateNamesAsSlots: true
      });
      path.stop();
    }
  });
  return tokens;
}

/** Index of the first differing token, or undefined when the streams match. */
function firstDifferingIndex(a: string[], b: string[]): number | undefined {
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) if (a[i] !== b[i]) return i;
  return a.length === b.length ? undefined : n;
}
