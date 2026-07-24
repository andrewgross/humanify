/**
 * exp036 idea 8b — diff-objective family permutation (post-render pass).
 *
 * The combined ambiguity+diff pass. The matcher is ambiguity-aware but
 * pre-render (no diff yet); the reconcile is diff-aware but its
 * diff-hunk framing and corpus gate miss whole-bucket family rotation
 * (and it cannot run on shuffle pairs at all). This pass closes that
 * gap: it works on the RENDERED output where the diff lives, groups
 * top-level statements by statement hash (same hash ⇒ identical
 * structure incl. literals ⇒ provably interchangeable), and for each
 * equal-count bucket picks the assignment that reproduces the prior
 * with the least churn ([`assignFamilyBucket`](./family-permute.ts)),
 * transferring names only where every difference is a statement-local
 * binding ([`deriveLocalRenames`](./family-permute.ts) — the owner
 * gate). Applied as an atomic permutation through the validated-rename
 * path, with the reconcile step's pure-rename structural invariant as
 * the backstop: any violation discards the whole pass.
 *
 * Deterministic, no LLM. Best-effort and self-contained like the
 * reconcile step — parses privately, validates against a local
 * baseline, and returns undefined (ship the pre-pass output) on any
 * failure.
 */

import type { GeneratorOptions } from "@babel/generator";
import type { Binding } from "@babel/traverse";
import * as t from "@babel/types";
import { findWrapperFunction } from "../analysis/wrapper-detection.js";
import { generate, parseSourceAst, traverse } from "../babel-utils.js";
import { debug } from "../debug.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant
} from "../output-validation.js";
import { statementHash } from "../split/statement-hash.js";
import {
  assignFamilyBucket,
  deriveLocalRenames,
  reassignmentsOnly
} from "./family-permute.js";
import type { IsEligibleFn } from "./rename-eligibility.js";
import { attemptValidatedRename } from "./validated-rename.js";

export interface FamilyPermuteOutcome {
  /** Bindings whose names were permuted to match the prior. */
  applied: number;
  /** Buckets considered; reassignments proposed but gated out. */
  buckets: number;
  skipped: number;
  /** Regenerated code — set only when renames applied and the invariant held. */
  code?: string;
  ast?: t.File;
}

/** One rendered top-level statement with its declared local bindings. */
interface FreshStmt {
  text: string;
  hash: string;
  /** name → Binding for identifiers DECLARED in this statement. */
  locals: Map<string, Binding>;
}

interface StmtPath {
  node: t.Statement;
  scope: { getBinding(name: string): Binding | undefined };
}

/** The top-level statement paths of a humanified single-file output: the
 * wrapper IIFE body when present, else the Program body. Paths carry the
 * scope needed to resolve each statement's declared bindings. */
function topLevelPaths(ast: t.File): StmtPath[] {
  const wrapper = findWrapperFunction(ast);
  if (wrapper && t.isBlockStatement(wrapper.functionPath.node.body)) {
    return wrapper.functionPath.get("body.body") as unknown as StmtPath[];
  }
  let programBody: StmtPath[] = [];
  traverse(ast, {
    Program(path) {
      programBody = path.get("body") as unknown as StmtPath[];
      path.stop();
    }
  });
  return programBody;
}

/** Declared-binding map for a statement: names bound by this statement,
 * resolved to their Binding in the enclosing scope. */
function collectLocals(
  stmtNode: t.Statement,
  scope: { getBinding(name: string): Binding | undefined }
): Map<string, Binding> {
  const locals = new Map<string, Binding>();
  for (const name of Object.keys(t.getBindingIdentifiers(stmtNode))) {
    const binding = scope.getBinding(name);
    if (binding) locals.set(name, binding);
  }
  return locals;
}

/** Prior statements grouped by hash → the list of their rendered texts. */
function priorBuckets(priorCode: string): Map<string, string[]> {
  const ast = parseSourceAst(priorCode);
  const byHash = new Map<string, string[]>();
  if (!ast) return byHash;
  const wrapper = findWrapperFunction(ast);
  const body =
    wrapper && t.isBlockStatement(wrapper.functionPath.node.body)
      ? wrapper.functionPath.node.body.body
      : ast.program.body;
  for (const stmt of body) {
    if (stmt.start == null || stmt.end == null) continue;
    const text = priorCode.slice(stmt.start, stmt.end);
    const hash = statementHash(stmt);
    const list = byHash.get(hash);
    if (list) list.push(text);
    else byHash.set(hash, [text]);
  }
  return byHash;
}

/** Fresh statements grouped by hash, carrying local bindings + text. */
function freshBuckets(ast: t.File, code: string): Map<string, FreshStmt[]> {
  const byHash = new Map<string, FreshStmt[]>();
  const paths = topLevelPaths(ast);
  for (const path of paths) {
    const stmt = path.node;
    if (stmt.start == null || stmt.end == null) continue;
    const text = code.slice(stmt.start, stmt.end);
    const hash = statementHash(stmt);
    const entry: FreshStmt = {
      text,
      hash,
      locals: collectLocals(stmt, path.scope)
    };
    const list = byHash.get(hash);
    if (list) list.push(entry);
    else byHash.set(hash, [entry]);
  }
  return byHash;
}

/** A concrete binding rename this pass will apply. */
interface PermRename {
  binding: Binding;
  fromName: string;
  toName: string;
}

/** Gather the permutation renames for one equal-count noisy bucket:
 * assign each fresh member a prior, then map local-binding slots — only
 * where every difference is a local binding (deriveLocalRenames), the
 * target name is eligible, and the fresh binding is not export-involved. */
function bucketRenames(
  fresh: FreshStmt[],
  priorTexts: string[],
  isEligible: IsEligibleFn
): PermRename[] {
  const assignment = assignFamilyBucket(
    fresh.map((f) => f.text),
    priorTexts
  );
  const renames: PermRename[] = [];
  for (const a of reassignmentsOnly(assignment)) {
    const f = fresh[a.freshIndex];
    const map = deriveLocalRenames(
      f.text,
      priorTexts[a.priorIndex],
      new Set(f.locals.keys())
    );
    if (!map) return []; // any non-permutable member voids the bucket
    for (const [fromName, toName] of map) {
      const binding = f.locals.get(fromName);
      if (!binding) return [];
      if (!isEligible(fromName)) return [];
      renames.push({ binding, fromName, toName });
    }
  }
  return renames;
}

/** Apply a set of renames as an atomic permutation: everyone to a unique
 * temp first, then temp→target — collision-free regardless of cycles.
 * Returns the count applied; a failed leg leaves the temp state for the
 * structural invariant to catch and discard. */
function applyPermutation(renames: PermRename[]): number {
  let applied = 0;
  const stage2: Array<{ binding: Binding; temp: string; toName: string }> = [];
  for (let i = 0; i < renames.length; i++) {
    const r = renames[i];
    const temp = `__fpTemp${i}$`;
    const first = attemptValidatedRename(r.binding.scope, r.fromName, temp);
    if (!first.applied) continue;
    stage2.push({ binding: r.binding, temp, toName: r.toName });
  }
  for (const s of stage2) {
    const second = attemptValidatedRename(s.binding.scope, s.temp, s.toName);
    if (second.applied) applied++;
  }
  return applied;
}

function familyPermuteInternal(
  code: string,
  priorCode: string,
  isEligible: IsEligibleFn,
  genOpts: GeneratorOptions
): FamilyPermuteOutcome | undefined {
  const ast = parseSourceAst(code);
  if (!ast) return undefined;
  const baseline = captureSemanticBaseline(ast);

  const prior = priorBuckets(priorCode);
  const fresh = freshBuckets(ast, code);

  const renames: PermRename[] = [];
  let buckets = 0;
  for (const [hash, members] of fresh) {
    const priorTexts = prior.get(hash);
    if (!priorTexts || priorTexts.length !== members.length) continue;
    if (members.length < 2) continue; // families only; twins handled upstream
    if (members.every((m, i) => m.text === priorTexts[i])) continue;
    buckets++;
    renames.push(...bucketRenames(members, priorTexts, isEligible));
  }
  if (renames.length === 0) return { applied: 0, buckets, skipped: 0 };

  const applied = applyPermutation(renames);
  if (applied === 0) return { applied: 0, buckets, skipped: renames.length };

  const failure = checkStructuralInvariant(ast, baseline);
  if (failure) {
    debug.log(
      "family-permute",
      `discarded: pure-rename invariant violated (${failure.message})`
    );
    return undefined;
  }
  return {
    applied,
    buckets,
    skipped: renames.length - applied,
    code: generate(ast, genOpts).code,
    ast
  };
}

/**
 * Best-effort post-render family permutation. Returns undefined when the
 * pass could not run, applied nothing that survived validation, or hit
 * any error (never throws — an optional pass must not abort a completed
 * run).
 */
export function runFamilyPermute(
  code: string,
  priorCode: string,
  isEligible: IsEligibleFn,
  genOpts: GeneratorOptions
): FamilyPermuteOutcome | undefined {
  try {
    return familyPermuteInternal(code, priorCode, isEligible, genOpts);
  } catch (err) {
    debug.log(
      "family-permute",
      `skipped: ${err instanceof Error ? err.message : String(err)}`
    );
    return undefined;
  }
}
