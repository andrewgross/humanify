/**
 * Naming-floor workstream 2: end-of-run coverage sweep.
 *
 * The deterministic passes (class-id derivation, decoration retry) leave a
 * residue of genuine minted survivors — function parameters, whole
 * declarations, and bare `var`/`let` bindings the matcher and LLM passes
 * never reached. This pass collects them and sends them to the LLM with a
 * code window, the same as a first-contact naming, then applies through the
 * validated path.
 *
 * The one precision surface is TARGETING: the census token shape
 * (`isBunToken`) over-counts on purpose, and force-naming a descriptive
 * name the census merely mis-shaped (`LZ77Compressor`, `is2017Api`,
 * `$context`, `OS_MODULE`) is exactly the self-inflicted noise this whole
 * campaign fights. `isSweepTarget` is therefore STRICTER than the census:
 * only genuinely minted-looking survivors — short, no embedded real word,
 * not a SCREAMING_CASE constant — are eligible. A missed true-minified
 * binding costs one more release of reroll noise; a swept good name costs
 * immediate genuine-looking noise. Precision over recall.
 */

import type * as t from "@babel/types";
import {
  type EvalWithTaint,
  isBindingEvalTaintFrozen
} from "../analysis/soundness.js";
import { generate } from "../babel-utils.js";
import { debug } from "../debug.js";
import type { LLMProvider } from "../llm/types.js";
import { createConcurrencyLimiter } from "../utils/concurrency.js";
import { MAX_CODE_LINES } from "./code-window.js";
import {
  collectMintedBindings,
  isBunToken,
  isHalfMintHead,
  type MintedBinding
} from "./minted-census.js";
import type { IsEligibleFn } from "./rename-eligibility.js";
import { strategyTrail } from "./strategy-trail.js";
import { attemptValidatedRename } from "./validated-rename.js";

/** Longest a minted survivor is expected to be after stripping `_`/`$`. */
const MAX_SWEEP_LENGTH = 4;

/**
 * True when `name` is a genuine minified survivor worth force-naming —
 * stricter than the census `isBunToken` shape.
 */
export function isSweepTarget(name: string): boolean {
  if (!isBunToken(name)) return false;
  // Pure `_`/`$` sequences are idiomatic "ignore"/placeholder markers, not
  // minted names to reconstruct — leave them (`_`, `$`, `__`).
  if (/^[_$]+$/.test(name)) return false;
  // Camel half-mints (do7Function, T7Class) are archive fossils: a derived
  // kind word glued onto a mint stem by an old pass, then re-inherited by
  // the reconcile every hop (exp035 task C). The tail is mechanical, not
  // meaning — sweep them despite the embedded word.
  if (isHalfMintHead(name)) return true;
  // A run of three lowercase letters is a real word (Compressor, context,
  // Function) — descriptive, never sweep.
  if (/[a-z]{3}/.test(name)) return false;
  // SCREAMING_CASE constants are deliberate.
  if (/^[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+$/.test(name)) return false;
  // Minted survivors are short; a long no-word token is more likely an
  // acronym-y real name (is2017Api, X509Certificate).
  return name.replace(/[_$]+$/, "").length <= MAX_SWEEP_LENGTH;
}

/**
 * Remaining eligible minted bindings that pass the strict sweep predicate
 * and are not eval/with-frozen. Run AFTER the deterministic floor passes
 * (their renames are already applied, so those bindings no longer appear).
 */
export function collectSweepTargets(
  ast: t.Node,
  isEligible: IsEligibleFn,
  taint: EvalWithTaint
): MintedBinding[] {
  return collectMintedBindings(ast, isEligible).entries.filter(
    (entry) =>
      isSweepTarget(entry.name) &&
      !isBindingEvalTaintFrozen(entry.binding, taint)
  );
}

// ---------------------------------------------------------------------------
// LLM sweep
// ---------------------------------------------------------------------------

export interface SweepResult {
  /** Bindings force-named and applied through the validated path. */
  named: number;
  /** Targets the LLM declined, returned unchanged, or that were rejected. */
  skipped: number;
  /** Number of code-window groups sent to the LLM. */
  groups: number;
}

interface SweepGroup {
  code: string;
  targets: MintedBinding[];
  /** Names visible in the group's scope, as an avoid-hint for the LLM. */
  usedNames: Set<string>;
}

/**
 * The node whose rendered code frames a target for the LLM: its own body
 * for a function/class declaration, the enclosing function for a param or
 * nested binding, else the top-level statement that declares it. Grouping
 * by this node names a function's minted param + locals together in one
 * request, with the function as context.
 */
function groupKeyNode(target: MintedBinding): t.Node {
  const path = target.binding.path;
  if (
    path.isFunctionDeclaration() ||
    path.isClassDeclaration() ||
    path.isFunctionExpression() ||
    path.isClassExpression()
  ) {
    return path.node;
  }
  const fnParent = target.binding.scope.getFunctionParent();
  if (fnParent) return fnParent.block;
  const statement = path.getStatementParent();
  return statement?.node ?? target.binding.scope.getProgramParent().block;
}

/** Cap the rendered code to the LLM window budget (params/signature survive). */
function capCode(code: string): string {
  const lines = code.split("\n");
  if (lines.length <= MAX_CODE_LINES) return code;
  return lines.slice(0, MAX_CODE_LINES).join("\n");
}

function buildGroups(targets: MintedBinding[]): SweepGroup[] {
  const byKey = new Map<t.Node, MintedBinding[]>();
  for (const target of targets) {
    const key = groupKeyNode(target);
    const bucket = byKey.get(key);
    if (bucket) bucket.push(target);
    else byKey.set(key, [target]);
  }
  const groups: SweepGroup[] = [];
  for (const [keyNode, bucket] of byKey) {
    const scope = bucket[0].binding.scope;
    groups.push({
      code: capCode(generate(keyNode as t.Node).code),
      targets: bucket,
      usedNames: new Set(Object.keys(scope.getAllBindings()))
    });
  }
  return groups;
}

async function nameGroup(
  group: SweepGroup,
  provider: LLMProvider
): Promise<{ named: number; skipped: number }> {
  const response = await requestGroupNames(group, provider);
  return applyGroupResponse(group, response.renames);
}

/** The group's (pre-built) LLM request — prompt content never depends on
 * other groups' completions. */
function requestGroupNames(group: SweepGroup, provider: LLMProvider) {
  return provider.suggestAllNames({
    code: group.code,
    identifiers: group.targets.map((target) => target.name),
    usedNames: group.usedNames,
    calleeSignatures: [],
    callsites: []
  });
}

/** Apply one group's suggestions through the validated path. */
function applyGroupResponse(
  group: SweepGroup,
  renames: Record<string, string>
): { named: number; skipped: number } {
  let named = 0;
  let skipped = 0;
  for (const target of group.targets) {
    const newName = renames[target.name];
    if (!newName || newName === target.name || isBunToken(newName)) {
      // A suggestion that still fails the floor (stem echo like
      // h06Result → h06CommandResult) would re-flag and re-roll every
      // hop — refuse it; the binding keeps its current name this run.
      skipped += 1;
      strategyTrail.recordPostPass(target.binding, target.name, {
        strategy: "coverage-sweep",
        outcome: "abstained",
        reason:
          newName && newName !== target.name
            ? "still-below-floor"
            : "llm-declined"
      });
      continue;
    }
    const attempt = attemptValidatedRename(
      target.binding.scope,
      target.name,
      newName
    );
    if (attempt.applied) {
      named += 1;
      strategyTrail.recordPostPass(target.binding, target.name, {
        strategy: "coverage-sweep",
        outcome: "applied",
        newName
      });
    } else {
      skipped += 1;
      strategyTrail.recordPostPass(target.binding, target.name, {
        strategy: "coverage-sweep",
        outcome: "rejected",
        reason: attempt.reason,
        newName
      });
    }
  }
  return { named, skipped };
}

/**
 * Force-name the minted survivors the deterministic floor left behind, one
 * LLM request per enclosing-scope group, applying through the validated
 * path (collisions and invalid names are rejected → skipped). Best-effort:
 * a failed group is logged and skipped, never fatal.
 */
export async function sweepMintedNames(
  ast: t.Node,
  provider: LLMProvider,
  isEligible: IsEligibleFn,
  taint: EvalWithTaint,
  opts: { concurrency?: number; deterministicApply?: boolean } = {}
): Promise<SweepResult> {
  const targets = collectSweepTargets(ast, isEligible, taint);
  if (targets.length === 0) return { named: 0, skipped: 0, groups: 0 };

  const groups = buildGroups(targets);
  const limit = createConcurrencyLimiter(opts.concurrency ?? 20);
  const outcomes = opts.deterministicApply
    ? await sweepDeterministic(groups, provider, limit)
    : await Promise.all(
        groups.map((group) =>
          limit(async () => {
            try {
              return await nameGroup(group, provider);
            } catch (err) {
              logSweepGroupFailure(err);
              return { named: 0, skipped: group.targets.length };
            }
          })
        )
      );

  const named = outcomes.reduce((sum, out) => sum + out.named, 0);
  const skipped = outcomes.reduce((sum, out) => sum + out.skipped, 0);
  return { named, skipped, groups: groups.length };
}

/**
 * Wave-scheduling variant: collect ALL responses first (prompts are
 * pre-built, so completion order cannot shape them), then apply in
 * group-build order — cross-group conflicts (two groups suggesting one
 * name into a shared scope) resolve by group order instead of by
 * whichever response happened to land first.
 */
async function sweepDeterministic(
  groups: SweepGroup[],
  provider: LLMProvider,
  limit: ReturnType<typeof createConcurrencyLimiter>
): Promise<Array<{ named: number; skipped: number }>> {
  const responses = await Promise.all(
    groups.map((group) =>
      limit(async () => {
        try {
          return await requestGroupNames(group, provider);
        } catch (err) {
          logSweepGroupFailure(err);
          return null;
        }
      })
    )
  );
  return groups.map((group, i) => {
    const response = responses[i];
    if (!response) return { named: 0, skipped: group.targets.length };
    return applyGroupResponse(group, response.renames);
  });
}

function logSweepGroupFailure(err: unknown): void {
  debug.log(
    "naming-floor",
    `sweep group failed: ${err instanceof Error ? err.message : String(err)}`
  );
}
