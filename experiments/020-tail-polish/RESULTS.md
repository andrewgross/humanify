# Exp020 — text-diff reconciliation: noise share 40.8% → 26.7% (2026-07-09)

Branch `exp020-tail-polish`. Goal metric: shared-lineage diff
(cc-119-lineage vs cc-120, prior `/tmp/exp016-r1/cc-120/runtime.js`).

## Headline

Two fresh lineage legs were run back-to-back on this branch — identical
except for the new flag — plus the flag-off leg pins run-to-run LLM
jitter against exp019's number (±2.6%, far below the effect):

| lineage-diff metric | exp019 (main) | exp020 flag OFF | exp020 flag ON |
| ------------------- | ------------- | --------------- | -------------- |
| noise hunks         | 1,288         | 1,321           | **684**        |
| noise share         | 40.8%         | 40.8%           | **26.7%**      |
| rename occurrences  | 2,474         | 2,502           | **1,126**      |
| genuine hunks       | 1,868         | 1,917           | 1,878 ✓        |

−48% noise hunks in-pipeline (623 bindings snapped, 432 skipped);
genuine stayed inside the 1,868–1,975 tripwire band; output parses and
the structural invariant + output validation ran clean after the pass.

Campaign: noise hunks 22,998 → **684 (−97.0%)**; share 92.0% → 26.7%.
Noise is now ~a third of genuine.

## What was built

`src/rename/diff-reconcile.ts` — a deterministic, LLM-free pass over the
rendered-text diff between the new output and the prior version's
output. A change hunk whose sides tokenize 1:1 with every non-identifier
token byte-identical is rename noise; each differing identifier position
resolves through Babel scope to its binding; bindings need unanimous
agreement across all their positions; survivors are renamed once via
`attemptValidatedRename` (declaration + every reference + writes). This
uses the one signal the AST matcher throws away by construction —
LCS/positional locality in the rendered file — so it is complementary to
the matcher, aimed at exactly the artifact the user reviews.

Safety gates (all default to skip):

- JS-aware line tokenizer: strings/templates/regex/comments opaque,
  reserved words verbatim; non-self-contained lines (open template)
  reject the hunk. Its failure direction is safe — misreads produce
  mismatches or unresolvable positions, which skip.
- A differing position must be the binding's own declaration, reference,
  or write target. Property names, object keys, and free identifiers
  taint the whole hunk (`obj.fooOld→obj.fooNew` is a genuine change).
- Every occurrence of the binding must sit on a diff-covered line —
  renaming would otherwise CREATE hunks, and stray occurrences signal
  the pairing is an alignment artifact (caught a var-redeclaration case
  in tests).
- Rename-type tiers: minified→descriptive (asymmetric) overwrites
  nothing meaningful; descriptive→descriptive additionally requires the
  declaration line to be a clean pair differing only in the binding's
  own name or in dependencies already reconciled this run (fixpoint
  over rounds); renames TO minified names never fire (reroll/downgrade).
- eval/with-tainted scopes are frozen, mirroring the pipeline's
  soundness rule (the lineage artifacts carry zero such sites — checked).
- After the pass: `checkStructuralInvariant` against a local pre-pass
  baseline, then full output validation against the run's original
  baseline. Any violation is fatal, never shipped.

Pipeline: `createRenamePlugin({ reconcilePriorDiff: true })`, CLI
`--reconcile-prior-diff` (requires `--prior-version`; skipped under
`--source-map`). The plugin returns the reconciled code AND its AST plus
`priorDiffReconciled: { renames, skipped }`. Runtime cost on the 21MB
output: ~15s (re-parse + diff + one traversal + regenerate).

## Offline post-pass on the exp019 artifacts (same artifact, zero jitter)

Applied post-hoc to the exact pair exp019's 1,288 was measured on:
noise 1,288 → **678** (−47.4%), occurrences 2,474 → 1,232, share 26.5%,
genuine 1,877, 560 bindings (549 descriptive / 11 asymmetric, 1,245
corroborating votes). The two independent routes (post-hoc offline vs
in-pipeline fresh run) land within 1% of each other.

Skips (offline dump): decl-not-clean 227, target-in-scope 152 (mostly
true swap cycles, e.g. `buildQueryParam⇄buildQueryParamVal`),
occurrence-outside-diff 40, name-downgrade 27 (descriptive new name,
minified prior — refusing to destroy information), reroll 11,
disagreement 3.

Spot-checks: every sampled rename reads as the same binding — identical
inits with drifted names (`spaceString → setScrollHeight`, both
`Math.max(0, childYogaNodeHeight - horizontalClipRight)`), positional
noop-triplet alignment (`noOp2 → placeholderFunc`, middle of three
identical `() => {}` siblings the hash can never split), ordinal churn
(`initializeApp370 → initializeModule118`). Applied renames left zero
residue in the re-diff; the two name-pairs still present
(`context→request`, `state→context`) are different bindings at other
sites, correctly skipped as genuine.

## The honest caveat: cosmetic on THIS diff

The pass reconciles two specific rendered outputs. It makes the
v119↔v120 diff reviewable; it does NOT improve the naming lineage going
forward — the underlying naming choices are unchanged, and a binding
snapped this release can drift again next release if the matcher misses
it again. (In the production chain the reconciled output becomes the
next release's prior, so the snap does propagate forward through the
lineage — but the matcher gap that caused the drift remains.) It also
snaps to whichever name the PRIOR leg had, even when the new leg's name
was better: `assistantMessages → requestedModel` went the right way,
`spaceString → setScrollHeight` merely swapped one mediocre name for
another. Diff stability is the goal metric, so this is on-target — but
it is polish over the matcher's residue, not a matcher improvement.

## Review round (2026-07-10, 10-angle multi-agent review)

A max-effort review (5 correctness + cleanup/altitude/conventions angles,
one verifier, one split-coordinate verifier) surfaced and I fixed six
confirmed precision/robustness bugs before merge — commit `96434fa`:

- **write-target phantom votes** — `resolveOccurrence` matched by AST
  subtree containment, so `accountId = cfg.accountId`'s RHS property (a
  genuine change) voted as a binding occurrence and could certify a
  changed value as reconciled noise. Now matched by node identity via a
  shared `violationWriteTargets` (same set the no-new-hunks gate uses).
- **asymmetric decl in a genuine hunk** — a minified→descriptive rename
  could land on a single reference vote while its declaration line was
  genuinely changed. Now every tier requires the declaration to sit in a
  clean ALIGNED noise pair (`decl-not-aligned`).
- **export-involved bindings** — would fall through to Babel's
  `scope.rename`, which splits `export const X` and creates hunks. Skipped.
- **SCREAMING_CASE / `$` misrouting** — `isMinifiedName` classified
  `HTTP_STATUS` and `response$` as minified, routing deliberate names into
  the weaker asymmetric gate. Corrected; dead `length<=4` clause removed.
- **corpus-similarity gate** — abstain when too few prior lines survive
  unchanged (the multi-file-unpack case, where aligned pairs are
  coincidence). CRLF is normalized before diffing (autocrlf prior would
  otherwise be a silent total no-op).
- **best-effort containment** — a missing `diff` binary, an unparseable
  output, or an invariant violation now discards the reconciliation and
  ships the validated pre-reconcile output instead of aborting a completed
  multi-hour run or reporting a failure that describes discarded code. The
  CLI rejects `--reconcile-prior-diff` without `--prior-version`.

Split-coordinate concern (`result.ast` swapped to an output re-parse under
`--split`) was **refuted** with empirical proof: babel-generator line
numbers are AST-shape-driven and invariant across a pure-rename
regeneration, and the swap is what keeps `code`/`ast` consistent.

Re-validated offline on the exp019 pair after the fixes: **noise 662
hunks** (was 678 pre-fix), genuine 1,901 (in band), 563 bindings applied,
invariants clean, zero same-binding residue. The headline 684 above is the
pre-hardening in-pipeline number; the hardened pass measures ~662 offline
(within the ~2.6% leg jitter). Marginal asymmetric snaps dropped 11→6 —
the fixes removed thin-evidence renames, which is the point.

### Deferred review follow-ups (tracked, not blocking)

- **Diagnostics staleness** — `reports`/`coverageData` (hence
  `--diagnostics` JSON `newName`) reflect the pre-reconcile LLM names;
  `priorDiffReconciled.pairs` now carries the applied (new→prior) pairs so
  a consumer CAN reconcile, but the reports themselves aren't patched. The
  shipped output and the file-diff noise metric are correct; only
  diagnostics-joined tooling is affected.
- **Unify the eval/with freeze rule** — `isEvalTaintFrozen` restates the
  graph-level `markEvalWithTaintPreDone` rule at binding altitude; a shared
  predicate in `soundness.ts` would prevent drift (guarded today: minified
  in both legs → reroll/downgrade gates block anyway).
- **Parse/`canon` duplication** — one `parseUnambiguous` helper in
  babel-utils and one shared test `canon` (5 copies across the branch).
- **Efficiency** — three full 21MB parses per reconciled run (validate,
  step re-parse, post-recheck) could share one AST; `resolveOccurrence`
  and per-round `collectOccurrenceLines` could memoize. ~seconds on the
  real bundle, not correctness.
- **Offline-harness divergence** — `run-reconcile.ts` re-implements the
  step sequence; should call `runPriorDiffReconciliation` so the measured
  pass is exactly the shipped one.

## What remains (662 hunks / 1,216 occ, hardened)

- `identityVar→identityVal` (24 occ) — unequal-count clone groups the
  ordinal gate refuses; unchanged from exp019.
- Swap cycles blocked by target-in-scope (152 bindings) —
  `h⇄y`, `buildQueryParam⇄buildQueryParamVal`; need two-phase temp-name
  renames.
- decl-not-aligned (104) + decl-not-clean (136) — declarations that are
  themselves genuinely changed, or whose other differing tokens never
  reconcile (drifted inits, property drift).
- Drift embedded in genuine hunks — names that only appear inside
  structurally-changed hunks have no clean pair to vote from.
- The reroll floor (46 occ) and downgrade-refusals (19 bindings, the
  descriptive→minified direction we refuse).

## Next candidates

1. **Naming floor (user directive 2026-07-10): leave NO minted tokens in
   the output.** The reconcile downgrade-gate rightly never reverts a
   name to a minified token — but the reason it ever has to decide is
   that runs still emit minified leftovers. Measured on the flag-on
   output (`744` surviving Bun-token bindings, shape-verified families):
   - **Class-expression inner ids: 328 (44%)** —
     `BaseError = class uq extends Error {}`: the outer binding is
     named, the expression's own id stayed minted. Deterministic fix,
     no LLM: derive the inner name from the assignment target /
     property key. Stable across versions by construction, so this also
     starves the reroll bucket at its source.
   - **Params of named functions: 112** — `updateReplBridgeState(H)`.
     Coverage sweep: end-of-run batch over every remaining eligible
     minted binding with code windows (~1/30th of current run cost,
     one-time — the lineage carries the names forward).
   - **Whole minified function/class decls: 16** (`j2_`, `FH3`) — same
     sweep.
   - **var/let leftovers: ≤282** (pattern-contaminated upper bound —
     `ec2MetadataServiceEndpointSelector` false-positives; includes
     collision decorations like `initializeApp_`) — sweep + undecorate
     retry.
     With the floor in place, the downgrade bucket (27 bindings) and the
     reroll floor (54 occ) both decay to zero over one release cycle.
2. Two-phase (temp-name) swap renames inside `runReconcileRounds` —
   mechanical, validated at each step, ~150 bindings / ~22+ occ for the
   top two pairs alone.
3. Tier 3 from the brief: signature-line param reconciliation for
   matched functions with drifted bodies (hunk is genuine, but the
   signature line is norm-clean in param positions only).
4. Runtime verification (user note, exp019): humanify recent npm cli.js
   versions and execute `--version`/`--help` as an executable complement
   to the structural invariant.

## Reproduce

```bash
# fresh in-pipeline lineage leg (flag on)
EXTRA_HUMANIFY_FLAGS="--reconcile-prior-diff" CHAIN_OUT=/tmp/exp020-chain-on \
  bash experiments/016-diff-noise-convergence/run-chain.sh
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/exp020-chain-on/runtime-diff.txt 10

# offline post-pass over existing artifacts (no LLM)
npx tsx experiments/020-tail-polish/run-reconcile.ts \
  --new /tmp/exp019-chain/cc-119-lineage/runtime.js \
  --prior /tmp/exp016-r1/cc-120/runtime.js \
  --descriptive --apply --out /tmp/exp020-recon/runtime-full.js \
  --dump /tmp/exp020-recon/renames-full.json
```
