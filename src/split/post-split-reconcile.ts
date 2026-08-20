/**
 * Post-split prior-diff reconciliation (exp054).
 *
 * `diff-reconcile` already exploits the one signal no per-function matcher has:
 * LCS alignment anchored on identical neighbouring lines. Until now it only saw
 * the BUNDLE — one flat ~800k-line file — where a name-masked statement is
 * compared against ~60,000 candidates. The split assigns every statement a file
 * using hash inheritance, name votes and content anchors, and the pipeline then
 * threw that identity evidence away. Running the same tiers over the emitted
 * TREE scopes each diff to one file, ~20 candidates, and the alignment's
 * neighbours become real neighbours.
 *
 * Measured ceiling before any of this was written (054 task 0, four cold gate
 * hops, trees already on disk, git-capped by construction):
 * 1,162 / 80 / 2,028 / 1,674 = **4,944 git lines removed, 0 created**.
 *
 * ## Where it runs, and why there
 *
 * After the emit produced the tree, before the tree is written. The rename is
 * applied to the file's AST and then to its TEXT, at the identifier positions
 * whose name changed — never by re-generating. Re-generating an emitted file
 * would reflow the assembler's require headers and export blocks and reorder
 * nothing usefully; a textual rewrite at the renamed locs is provably a pure
 * rename, so split formatting and emission order cannot move. Every file is
 * re-parsed afterwards and must still carry the pre-rename structural
 * signature, or its reconciliation is dropped and the original text ships.
 *
 * ## Ledger coherence
 *
 * `emitHashes` is a `statementHash` array and `statementHash` MASKS identifier
 * names, so a pure rename cannot move it — which is what makes patching
 * `emitNames` alone safe here (writing one without the other is the lockstep
 * bug 050 nearly shipped, and it only bites when both really change). Only
 * TOP-LEVEL renames touch the ledger: `emitNames` and `nameToFiles` describe
 * declared statements, and an inner local that happens to share a spelling with
 * some other file's top-level declaration must not rewrite that slot.
 *
 * Kill switch: `--disable post-split-reconcile`.
 */
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import {
  parseSourceAst,
  renameSubstitutionText,
  traverse
} from "../babel-utils.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant
} from "../output-validation.js";
import {
  collectWordTokens,
  computeNormalDiff,
  type RenameKind,
  reconcileDiffNoise
} from "../rename/diff-reconcile.js";
import type { IsEligibleFn } from "../rename/rename-eligibility.js";
import type { StableSplitLedger } from "./stable-split.js";
import { applySubstitutions, type Substitution } from "./substitutions.js";
import { switchOn } from "../kill-switches.js";

export interface PostSplitReconcileInput {
  /** Patched in place when a TOP-LEVEL declaration is renamed. Its `files` are
   * also the scope: vendor bodies are never named and `.humanify/` is
   * metadata. */
  ledger: StableSplitLedger;
  /** The FINAL emitted text of a ledger file — read on demand rather than
   * passed as a map, so the pass can run against the tree on disk without
   * holding a 46MB copy of it. */
  readFresh: (file: string) => string | undefined;
  /** The prior release's text for the same relative path, if it has one. */
  readPrior: (file: string) => string | undefined;
  isEligible: IsEligibleFn;
}

/** One rename this pass actually shipped. A pass with an empty trail cannot
 * have moved a KPI however the KPI reads (measurement-pitfalls rule 11). */
export interface PostSplitRename {
  file: string;
  fromName: string;
  toName: string;
  kind: RenameKind;
  votes: number;
  /** True when the renamed binding is a top-level declaration of its file. */
  topLevel: boolean;
  /**
   * Where this binding is, in terms the BUNDLE also understands, or undefined
   * when it could not be located (the carry then abstains for this rename).
   *
   *   `bodyOrdinal` — which of the file's ledger statements the declaration is
   *   in. An emitted file is a header (directives, export accessors, requires)
   *   followed by exactly the file's ledger statements in emitted order, so the
   *   ordinal indexes `emitIndexes` and yields the bundle statement.
   *
   *   `nameOrdinal` — which declaration of that name inside the statement, for
   *   the case of two nested functions each declaring a local of the same name.
   *   The emit rewrites cross-file references (`f(x)` to `ns.f(x)`) but never
   *   adds or removes a binding, so the sequence is the same on both sides.
   */
  locator?: { bodyOrdinal: number; nameOrdinal: number };
}

export interface PostSplitReconcileStats {
  considered: number;
  changed: number;
  corpusGated: number;
  /** Files whose reconciliation was dropped by an invariant check. */
  discarded: number;
  /** Ledger slots or name homes left describing a name no longer in the tree.
   * Must be 0: the next release aligns on these. */
  incoherent: number;
}

export interface PostSplitReconcileResult {
  /** ONLY the files whose text changed — the caller writes these back. */
  changed: Map<string, string>;
  renames: PostSplitRename[];
  stats: PostSplitReconcileStats;
}

const IDENT_AT = /^[A-Za-z_$][\w$]*/;

/**
 * Identifier positions whose name in the reconciled AST differs from the token
 * standing at that loc in the original text — exactly what the rename rewrote.
 * The old name is read back OUT of the text, so a position is only substituted
 * when the text really holds it there.
 */
function collectSubstitutions(ast: t.File, lines: string[]): Substitution[] {
  const subs: Substitution[] = [];
  traverse(ast, {
    Identifier(p: NodePath<t.Identifier>) {
      const loc = p.node.loc;
      if (!loc) return;
      const text = lines[loc.start.line - 1];
      if (text === undefined) return;
      const match = IDENT_AT.exec(text.slice(loc.start.column));
      if (!match || match[0] === p.node.name) return;
      subs.push({
        line: loc.start.line,
        col: loc.start.column,
        from: match[0],
        // Shorthand-aware: `{ count }` renamed to `tally` must become
        // `{ count: tally }`, never rewrite the key.
        to: renameSubstitutionText(p, p.node.name)
      });
    }
  });
  return subs;
}

/** Every binding DECLARATION in the file, as (top-level statement index, name,
 * source offset). One traversal; the alternative is one per rename. */
function declarationIndex(
  ast: t.File
): Array<{ stmt: number; name: string; start: number; line: number }> {
  const body = ast.program.body;
  const spans = body.map((s) => [s.start ?? -1, s.end ?? -1] as const);
  const statementOf = (pos: number): number => {
    let lo = 0;
    let hi = spans.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (pos < spans[mid][0]) hi = mid - 1;
      else if (pos > spans[mid][1]) lo = mid + 1;
      else return mid;
    }
    return -1;
  };
  const out: Array<{
    stmt: number;
    name: string;
    start: number;
    line: number;
  }> = [];
  traverse(ast, {
    Scopable(p: NodePath) {
      for (const [name, binding] of Object.entries(p.scope.bindings)) {
        if (binding.scope.block !== p.node) continue;
        const start = binding.identifier.start;
        const line = binding.identifier.loc?.start.line;
        if (start == null || line == null) continue;
        const stmt = statementOf(start);
        if (stmt >= 0) out.push({ stmt, name, start, line });
      }
    }
  });
  return out;
}

/**
 * Locate each rename against the file's LEDGER statements, so the bundle can be
 * given the same name. `ledgerStatements` is how many of the emitted file's
 * top-level statements are ledger statements; they are the last ones, after the
 * header the assembler prepends. A mismatch (a later pass restructured the
 * file) yields no locator and the carry abstains rather than guessing.
 */
function locateRenames(
  reparsed: t.File,
  renames: PostSplitRename[],
  declLines: number[],
  ledgerStatements: number
): void {
  const header = reparsed.program.body.length - ledgerStatements;
  if (header < 0) return;
  const decls = declarationIndex(reparsed);
  renames.forEach((rename, i) => {
    const declLine = declLines[i];
    // The renamed declaration carries the NEW name and sits on declLine.
    const self = decls.find(
      (d) => d.name === rename.toName && d.line === declLine && d.stmt >= header
    );
    if (!self) return;
    // Everything that still bears the OLD name in the same statement, plus this
    // one, in source order: the position of this one is the ordinal.
    const siblings = decls
      .filter(
        (d) =>
          d.stmt === self.stmt && (d.name === rename.fromName || d === self)
      )
      .sort((a, b) => a.start - b.start);
    const nameOrdinal = siblings.indexOf(self);
    if (nameOrdinal < 0) return;
    rename.locator = { bodyOrdinal: self.stmt - header, nameOrdinal };
  });
}

/** Names declared by the file's own top-level statements. */
function topLevelNames(ast: t.File): Set<string> {
  const names = new Set<string>();
  for (const stmt of ast.program.body) {
    for (const name of Object.keys(t.getBindingIdentifiers(stmt))) {
      names.add(name);
    }
  }
  return names;
}

/**
 * Point the cross-release memory at the names that shipped. Both fields
 * describe DECLARED statements, so only top-level renames apply; `emitHashes`
 * is deliberately untouched (see the module docstring).
 */
function patchLedger(
  ledger: StableSplitLedger,
  file: string,
  renames: PostSplitRename[]
): void {
  const applicable = renames.filter((r) => r.topLevel);
  if (applicable.length === 0) return;
  const byFrom = new Map(applicable.map((r) => [r.fromName, r.toName]));
  patchEmitNames(ledger, file, byFrom);
  patchNameHomes(ledger, file, applicable);
}

/**
 * Move each renamed name's home entry, ALL REMOVALS FIRST.
 *
 * The reconcile rounds produce chains — `oversize-report.js` shipped
 * `readSessionTemplate -> loadSessionTemplate` and then
 * `fetchSessionNotesPrompt -> readSessionTemplate`, the second unblocked by the
 * first freeing the name. Applied one rename at a time, the later removal
 * deletes the home entry the earlier addition just wrote. Splitting the phases
 * makes the result independent of the order the renames arrive in, rather than
 * relying on `runReconcileRounds` to emit blockers first.
 *
 * Counts are preserved: a name declared by two `var` statements in one file has
 * two entries, and renaming its (single) binding has to move both.
 */
function patchNameHomes(
  ledger: StableSplitLedger,
  file: string,
  applicable: PostSplitRename[]
): void {
  const toAdd = new Map<string, number>();
  for (const rename of applicable) {
    const homes = ledger.nameToFiles[rename.fromName];
    if (!homes) continue;
    const kept = homes.filter((f) => f !== file);
    const removed = homes.length - kept.length;
    if (removed === 0) continue;
    if (kept.length > 0) ledger.nameToFiles[rename.fromName] = kept;
    else delete ledger.nameToFiles[rename.fromName];
    toAdd.set(rename.toName, (toAdd.get(rename.toName) ?? 0) + removed);
  }
  for (const [toName, count] of toAdd) {
    const list = ledger.nameToFiles[toName] ?? [];
    for (let i = 0; i < count; i++) list.push(file);
    ledger.nameToFiles[toName] = list;
  }
}

/**
 * Re-point this file's slots at the names that shipped.
 *
 * A slot's key is `statementAlignName`: EVERY name the statement declares,
 * sorted and comma-joined, because `var a, b, c;` and `var d, e, f;` share a
 * statement hash and have to key apart. So the key is decomposed, substituted
 * member-wise, and re-sorted — comparing the whole key against a bare name
 * would silently miss any rename inside a multi-declarator statement and leave
 * the next release aligning on a name no longer in the tree.
 */
function patchEmitNames(
  ledger: StableSplitLedger,
  file: string,
  byFrom: ReadonlyMap<string, string>
): void {
  const emitNames = ledger.emitNames;
  if (!emitNames) return;
  for (let slot = 0; slot < emitNames.length; slot++) {
    if (ledger.order[slot] !== file) continue;
    const current = emitNames[slot];
    if (current === null) continue;
    let changed = false;
    const parts = current.split(",").map((name) => {
      const next = byFrom.get(name);
      if (next === undefined) return name;
      changed = true;
      return next;
    });
    if (changed) emitNames[slot] = parts.sort().join(",");
  }
}

interface FileOutcome {
  text?: string;
  renames: PostSplitRename[];
  corpusGated: boolean;
  discarded: boolean;
}

const CLEAN: FileOutcome = {
  renames: [],
  corpusGated: false,
  discarded: false
};

function reconcileOneFile(
  file: string,
  freshText: string,
  priorText: string,
  isEligible: IsEligibleFn,
  ledgerStatements: number
): FileOutcome {
  const diffText = computeNormalDiff(priorText, freshText);
  if (diffText.length === 0) return CLEAN;
  const ast = parseSourceAst(freshText, { filename: file });
  if (!ast) return CLEAN;

  const baseline = captureSemanticBaseline(ast);
  const result = reconcileDiffNoise(ast, diffText, {
    apply: true,
    descriptiveTier: true,
    consumerTier: true,
    // exp061: hash-flipped statements drag their name churn into hunks
    // with real edits; the mixed tier admits the clean pairs under a
    // stricter all-occurrences-clean gate. Offline A/B on the noise-band
    // trees: −108/−110 ledger lines per hop, 0 created, two-run stable.
    mixedHunkTier: true,
    lastResortTier: true,
    skipImportDeclarations: true,
    priorNames: collectWordTokens(priorText),
    isEligible,
    priorLineCount: priorText.split("\n").length
  });
  if (result.priorTooDissimilar) return { ...CLEAN, corpusGated: true };
  if (result.renames.length === 0) return CLEAN;
  if (checkStructuralInvariant(ast, baseline)) {
    return { ...CLEAN, discarded: true };
  }

  const lines = freshText.split("\n");
  const rewritten = applySubstitutions(lines, collectSubstitutions(ast, lines));
  // The saving is only real if the rewritten TEXT is still the same program:
  // a botched substitution that dropped a token would otherwise ship.
  const reparsed = parseSourceAst(rewritten, { filename: file });
  if (!reparsed || checkStructuralInvariant(reparsed, baseline)) {
    return { ...CLEAN, discarded: true };
  }
  const declared = topLevelNames(reparsed);
  const renames: PostSplitRename[] = result.renames.map((r) => ({
    file,
    fromName: r.fromName,
    toName: r.toName,
    kind: r.kind,
    votes: r.votes,
    topLevel: declared.has(r.toName)
  }));
  locateRenames(
    reparsed,
    renames,
    result.renames.map((r) => r.declLine),
    ledgerStatements
  );
  return { text: rewritten, corpusGated: false, discarded: false, renames };
}

/** One file's reconciliation, with the "an optional pass must never lose a
 * completed run" contract: any throw is a discard, never a failure. */
function safeReconcile(
  file: string,
  freshText: string,
  priorText: string,
  isEligible: IsEligibleFn,
  ledgerStatements: number
): FileOutcome {
  try {
    return reconcileOneFile(
      file,
      freshText,
      priorText,
      isEligible,
      ledgerStatements
    );
  } catch {
    return { ...CLEAN, discarded: true };
  }
}

/**
 * Reconcile every emitted split file against the prior release's file at the
 * same path. Best-effort throughout: a file that cannot be diffed, parsed, or
 * proved a pure rename ships exactly as the emitter produced it.
 */
export function postSplitReconcile(
  input: PostSplitReconcileInput
): PostSplitReconcileResult {
  const changed = new Map<string, string>();
  const renames: PostSplitRename[] = [];
  const stats: PostSplitReconcileStats = {
    considered: 0,
    changed: 0,
    corpusGated: 0,
    discarded: 0,
    incoherent: 0
  };
  if (switchOn("post-split-reconcile")) {
    return { changed, renames, stats };
  }
  const ledgerStatements = new Map<string, number>();
  for (const file of input.ledger.order) {
    ledgerStatements.set(file, (ledgerStatements.get(file) ?? 0) + 1);
  }
  for (const file of input.ledger.files) {
    const freshText = input.readFresh(file);
    const priorText = input.readPrior(file);
    if (freshText === undefined || priorText === undefined) continue;
    stats.considered++;
    const outcome = safeReconcile(
      file,
      freshText,
      priorText,
      input.isEligible,
      ledgerStatements.get(file) ?? 0
    );
    if (outcome.corpusGated) stats.corpusGated++;
    if (outcome.discarded) stats.discarded++;
    if (outcome.text === undefined) continue;
    changed.set(file, outcome.text);
    stats.changed++;
    renames.push(...outcome.renames);
    patchLedger(input.ledger, file, outcome.renames);
    stats.incoherent += countStaleLedgerEntries(
      input.ledger,
      file,
      outcome.renames
    );
  }
  return { changed, renames, stats };
}

/**
 * Ledger entries still naming a binding this pass renamed away. The patch is
 * name-keyed, so this is the check that it actually landed: a surviving entry
 * would point the NEXT release's emission aligner and name votes at a name
 * that is not in the tree, which degrades silently rather than failing.
 */
function countStaleLedgerEntries(
  ledger: StableSplitLedger,
  file: string,
  renames: PostSplitRename[]
): number {
  const topLevel = renames.filter((r) => r.topLevel);
  const targets = new Set(topLevel.map((r) => r.toName));
  // A chain hands a name BACK: after `read -> load` and `fetch -> read`, the
  // tree really does declare `read` again, so its ledger entries are correct
  // even though `read` is also something's fromName. Only a name that left and
  // did not return can be stale.
  const gone = topLevel
    .map((r) => r.fromName)
    .filter((name) => !targets.has(name));
  let stale = 0;
  for (const name of gone) {
    if (ledger.nameToFiles[name]?.includes(file)) stale++;
    ledger.emitNames?.forEach((entry, slot) => {
      if (ledger.order[slot] !== file || entry === null) return;
      if (entry.split(",").includes(name)) stale++;
    });
  }
  return stale;
}
