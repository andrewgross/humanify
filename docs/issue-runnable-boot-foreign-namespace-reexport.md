# Issue: runnable tree fails to boot at 2.1.172+ — foreign-namespace re-export runs before its target module initializes

Status: **RESOLVED** (branch fix/runnable-foreign-ns-reexport). Fix direction
(1) from the list below — co-locate the augmentation with its target — done at
the RUNNABLE-EMIT layer, where there is no contiguity constraint:
`relocateNamespaceAugmentations` in `src/split/cjs-emit.ts` moves every
top-level `copyProps(<ns>, { key: () => thunk, ... })` statement whose target
binding is declared in ANOTHER ledger file into the file that defines it. The
helper is SHAPE-matched (a ≥2-param function whose body for-in's over param 2
and calls defineProperty-ish with param 1 first), never name-matched. Because
file bodies emit in original statement order, ranges are contiguous, and the
definition preceded the augmentation in the bundle (it executed there), the
relocated call runs immediately after its target initializes — the original
semantics. The review tree, the shipped ledger, concat-equivalence, and
next-hop inheritance are all untouched: only the runnable form's
statement→file map changes.

Why the previous own-exports-first fix could not cover this: the module was
writing onto ANOTHER module's namespace member through a MIXED require cycle
(deferred edge one way, load-time edge back) — invisible to
assertLoadTimeAcyclic, and the accessor-first headers correctly make the
mid-cycle read yield a hoisting-faithful `undefined`, which this call then
passed to defineProperty.

Validation: TDD unit tests reproduce the mixed-cycle crash and assert
placement + a live boot through the cycle (src/split/cjs-emit.test.ts,
"foreign-namespace re-export relocation"); full `npm run check` green (1348
unit + 33 fingerprint). On real data: re-emitting the 2.1.174 tree from its
own humanified.js + ledger relocates exactly the one affected statement
(oauth-bridge → team-lead-detector, definition first), and applying that
exact transformation to a COPY of the shipped tree takes `bun run.cjs
--version` from the defineProperty TypeError to printing `2.1.174 (Claude
Code)` (exit 0; `--help` renders the full CLI). 172–174 outputs should be
rebuilt on the fixed code when the walk resumes; the original analysis
follows for history.

Last updated 2026-07-20 (found during RUN 4 of the claude-code version walk,
by boot-testing the newest hops).

## TL;DR

From **Claude Code 2.1.172 onward**, the split/runnable tree crashes at module
load under Bun with `TypeError: Properties can only be defined on Objects.`
Versions **≤ 2.1.170 boot fine.** The trigger is a **single call site** — the
only one of 457 `defineModuleExports(...)` calls whose target is a _foreign
module's namespace_ rather than a local object:

```js
// src/query-input/interface/oauth-bridge.js:35  (generated tree)
(0, resourceLifecycle.defineModuleExports)(teamLeadDetector.teammateContext, {
  waitForTeammatesToBecomeIdle: () => teamLeadDetector.waitForIdleTeammates,
  setDynamicTeamContext: () => teamLeadDetector.setCurrentAgent,
  ...
});
```

`teamLeadDetector.teammateContext` is `undefined` at the moment this line runs
(a require cycle leaves `team-lead-detector.js` partially initialized), so the
helper calls `Object.defineProperty(undefined, ...)` and throws. This is the
**teammates / subagent feature**, refactored into this shape in 2.1.171/172.

## NOT the per-AST cache swap (read this first)

This was found right after merging `feat/per-ast-analysis-cache` (`7b55f81`) into
the live walk, so the obvious suspicion is the swap. **It is not.** Proof:
**2.1.173, built entirely on the OLD code (`ade8eae`, before the merge), fails to
boot with the byte-identical error and stack.** The per-AST swap is
behavior-neutral (fingerprint snapshots are byte-identical; 174's split quality
matches 173's: 97.8% vs 98.4% inherited). Do not chase the analysis-cache change
for this bug — it is a pre-existing runnable-emit gap.

## Symptom

```
$ cd versions/claude-code-2.1.174 && bun run.cjs --version
...
14 | var defineModuleExports = (targetObject, sourceObject) => {
15 |   for (var propKey in sourceObject) (0, esmoduleConverter.defineProperty)(targetObject, propKey, {
                                                              ^
TypeError: Properties can only be defined on Objects.
    at defineModuleExports (src/array-builder/resource-lifecycle.js:15:61)
    at <anonymous>        (src/query-input/interface/oauth-bridge.js:35:23)
    at <anonymous>        (src/floor/markdown-renderer/crypto-contexts.js:25:7)
    at <anonymous>        (src/query-input/feedback/side-question-handler.js:33:7)
```

`--version` never prints; the crash is at module-graph load, before any command
runs. (Reviewers: `bun run.cjs --version` is the load gate in
`unpacked-claude-code/scripts/review-version.sh`.)

## Scope

Boot-test spread of RUN 4 trees (`bun run.cjs --version`):

| version         | boots?                                            |
| --------------- | ------------------------------------------------- |
| 2.1.89 (anchor) | ✓                                                 |
| 2.1.120         | ✓                                                 |
| 2.1.150         | ✓                                                 |
| 2.1.170         | ✓                                                 |
| 2.1.171         | (not a walkable version — no Bun bundle, skipped) |
| 2.1.172         | ✗                                                 |
| 2.1.173         | ✗ (old code — see "NOT the swap")                 |
| 2.1.174         | ✗ (new code)                                      |

So the break is introduced by the **2.1.172 source** and affects every version
from **172 through 211** (~40 hops) that inherits the teammates code. **The
readable/source deliverable is unaffected** — the split is clean (stable path,
97.8% inherited, zero minted paths, top-level dirs identical to anchor), and the
history repo is packaged source-only (`run.cjs`/vendor excluded). This blocks the
functional "runs under Bun" quality gate only.

## Root cause

The original bundle defines a `teammateContext` namespace object and, in the same
lexical scope, augments it with a batch of live-binding getters via the Bun
runtime helper (named `defineModuleExports` by the split namer — it is Bun's
`__reExport`/copy-props helper: for each key it does
`defineProperty(target, key, { get, set })`).

The split assigned the **definition** of `teammateContext` to
`src/config/process-alive/team-lead-detector.js` (exported there) and the
**augmentation** to `src/query-input/interface/oauth-bridge.js`, which reaches
the target cross-module as `teamLeadDetector.teammateContext` (required at
`oauth-bridge.js:22`). At oauth-bridge's init:

- `oauth-bridge.js` is pulled in through a require chain
  (`side-question-handler.js` → `crypto-contexts.js` → `oauth-bridge.js`) that
  is **cyclic** with `team-lead-detector.js`, so when the augmentation line runs,
  `team-lead-detector.js` is mid-initialization and `teammateContext` has not
  been assigned yet → `teamLeadDetector.teammateContext` is `undefined`.
- `defineModuleExports(undefined, {...})` → `defineProperty(undefined, ...)` →
  throw.

In the original single-file bundle this worked because target and augmentation
shared one scope and executed in source order; **splitting turned a same-scope
local reference into a cross-module property read whose module-load order is not
guaranteed.**

### Why the previous fix doesn't cover it

`docs/issue-runnable-trees-dont-run.md` (RESOLVED) fixed "circular requires saw
partial exports" by **emitting a module's own export accessors before its
requires** (`0e1f342`/`a300468`/`83c50c6`). That guarantees a module's _own_
`module.exports` are live getters early. This case is different: the module is
**writing onto another module's namespace object**, and the failure is that the
_target_ module's namespace member is not yet initialized — an ordering problem
the own-exports-first fix does not address.

## Where the emit lives

- `src/split/cjs-emit.ts` — `emitRunnableCjs` / `tryEmitRunnableCjs`; own-export
  accessors emitted at line ~1016 (`Object.defineProperty(module.exports, ...)`),
  require wiring and load ordering here.
- `src/split/emitter.ts`, `src/split/index.ts` — split assignment + re-export
  handling.
- The `defineModuleExports` helper itself is **app/runtime code preserved from
  the bundle**, not emitted by humanify — the bug is in _where the split places
  the augmentation call relative to its target_, and/or the require order, not in
  the helper.

## Fix directions (for the reviewing agent to weigh)

Only **one** call site is affected (verified: of 457 `defineModuleExports` calls
in the 174 tree, exactly 1 has a dotted foreign-namespace first argument), so a
targeted fix is viable. Candidates, roughly in order of appeal:

1. **Co-locate the augmentation with its target.** Detect
   `defineModuleExports(<foreignModule>.<member>, {...})` and keep that statement
   in the same split file as the module that _defines_ `<member>` (or pull the
   target's definition into the augmentation's file). Removes the cross-module
   ordering hazard entirely. Cleanest if the split assigner can express "pin
   these statements together."
2. **Defer foreign-namespace augmentations to a post-load pass.** Emit these
   specific calls into a small trailer that the entry file runs _after_ the whole
   module graph has loaded, when every namespace is fully initialized.
3. **Lazy target resolution / guard.** Wrap so the augmentation runs on first
   access of the target rather than at import time. More invasive; risks changing
   observable timing.
4. **Break the cycle by ordering** so `team-lead-detector.js` fully initializes
   before `oauth-bridge.js` — likely impossible if the cycle is genuine; verify
   the require graph first.

Recommendation: investigate (1) first — it is the most robust and the blast
radius is a single statement class. Confirm the require cycle with a load-order
trace before committing to an approach.

## Reproduction

```bash
cd /Users/andrewgross/Development/unpacked-claude-code/versions/claude-code-2.1.174
bun run.cjs --version          # crashes as above

# scope check across the walk:
for V in 2.1.170 2.1.172 2.1.174; do
  (cd versions/claude-code-$V && bun run.cjs --version 2>&1 | head -1)
done

# the single offending call:
grep -rnE 'defineModuleExports\)\([a-zA-Z_$][\w$]*\.[a-zA-Z_$]' \
  versions/claude-code-2.1.174/src
```

The generated crash files to read:

- `versions/claude-code-2.1.174/src/query-input/interface/oauth-bridge.js` (the
  augmentation call, line 35; target required line 22)
- `versions/claude-code-2.1.174/src/config/process-alive/team-lead-detector.js`
  (defines `teammateContext`)
- `versions/claude-code-2.1.174/src/array-builder/resource-lifecycle.js`
  (the `defineModuleExports` helper, line 14)

Input bundle (for re-running the split):
`/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.172/binary-decompiled/src/entrypoints/index.js`
(2.1.172 is the earliest failing version — fix and validate against it, not 174).

## Related

- `docs/issue-runnable-trees-dont-run.md` — RESOLVED; the own-exports-before-
  requires fix. This is a distinct, newer manifestation (foreign-namespace
  augmentation) that fix does not cover.
- `docs/issue-ephemeron-cache-thrash.md` — unrelated (the per-AST swap); noted
  here only to preempt the "did the swap cause it" question (it did not).
- Walk operating runbook: `unpacked-claude-code/RUNNER.md`. The walk is currently
  **paused at this finding** (last booting version 2.1.170; 172–174 built but do
  not boot).
