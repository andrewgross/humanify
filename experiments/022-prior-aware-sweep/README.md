# Experiment 022 — prior-aware coverage sweep: turn the sweep into a pure win

**Goal of this experiment:** make the naming-floor LLM sweep
(`--naming-floor-sweep`, exp021 WS2) **cross-version stable** by reusing
the prior version's name for a minted binding instead of asking the LLM
for a fresh one. Today the sweep clears minified leftovers but ADDS
lineage diff noise because it names the same binding differently in each
release; a prior-aware sweep removes the leftovers WITHOUT the noise —
converting exp021's trade-off into the pure win the deterministic floor
already is.

This doc is self-contained. Pick it up cold and run. Read the campaign
context, then the finding, then the design, then the runbook. Every code
anchor was verified on 2026-07-10 against branch `exp021-naming-floor`;
re-verify before editing.

---

## The end goal (the whole campaign)

Humanify two adjacent versions of a minified bundle (fixture: Claude
Code v119/v120, Bun single-file binary decompiled to
`.../binary-decompiled/src/entrypoints/index.js`) and make the `diff`
between the two humanified outputs contain **only genuine source
changes** — reviewable by a human like a real git diff. We measure on the
**shared-lineage diff**: each release is humanified incrementally on the
previous output (`--prior-version`), so both diffed legs inherit one
naming lineage — the production scenario.

### Campaign trajectory (lineage-diff noise hunks, steady state)

22,998 → exp014 6,206 → exp015 5,788 → exp016 2,960 → exp017 2,516 →
exp018 1,504 → exp019 1,288 → exp020 662 (`--reconcile-prior-diff`,
MERGED to main) → **exp021 deterministic floor 658** (`--naming-floor`,
pure win) / 709 with the sweep (`--naming-floor-sweep`, the regression
this experiment fixes). Genuine-change hunks have held in the
**1,868–1,975** band the entire campaign — that is the SIGNAL and must
never rise.

Read at minimum: `experiments/020-tail-polish/RESULTS.md` (the reconcile
pass — the asymmetric-tier matcher this experiment reuses) and
`experiments/021-naming-floor/RESULTS.md` (the floor, the sweep, and the
finding this experiment acts on).

---

## Why this change now — the exp021 finding

The naming floor splits into two parts (exp021 RESULTS, measured on the
both-legs-floored lineage pair):

| config                                          | census/leg | noise |
| ----------------------------------------------- | ---------- | ----- |
| deterministic floor (derive+undecorate) + recon | 744 → 411  | 658   |
| full floor (+ LLM sweep) + reconcile            | 744 → 211  | 709   |

The **deterministic floor is a pure win** (fewer leftovers AND less
noise) because the class-expression inner-id derivation is
cross-version STABLE: the same class structure yields the same derived
name in both legs, so a binding that was `uq`↔`vq` reroll noise becomes
`BaseError`↔`BaseError` — zero diff noise.

The **LLM sweep is a trade-off** because it is NOT stable: the LLM names
each leg independently, so a param that was `H`↔`H` (identical minified
token, no diff hunk) becomes `config`↔`handler` (a NEW hunk). It removes
~200 more leftovers (411 → 211) but adds ~50 noise hunks (658 → 709).

**The load-bearing insight:** the exp020 reconcile pass ALREADY snaps
minified→descriptive names confidently — that is its **asymmetric tier**
(`RenameKind = "asymmetric"`, the weaker-gate case in
`src/rename/diff-reconcile.ts`). A minified `H` in the new leg against a
descriptive `handler` in the prior leg is the EASY case it handles well.
The sweep defeats this by LLM-naming `H → config` FIRST, turning it into
the HARD `config`↔`handler` descriptive→descriptive case that the
reconcile pass's clean-declaration gate (correctly) refuses. The reconcile
pass caught only ~104 of the sweep's inconsistencies in exp021 (984 → 709);
the rest are swept params/vars it can't gate open.

So the fix is about ORDER and REUSE, not new naming: **prefer the prior
name over a fresh LLM name.** When the prior version already named this
binding, transfer that name (deterministic, stable); only ask the LLM for
bindings with NO prior counterpart (genuinely new code, whose fresh name
appears in one leg only and so creates no cross-leg noise).

---

## The design (implement this; directions, not a transcript)

### Step 0 — SIZE THE WIN FIRST (do this before building anything)

Of the ~273 sweep targets on a floored lineage leg, how many have a
**prior descriptive counterpart** (addressable by transfer) vs are
**genuinely new** (must go to the LLM)? This number is the ceiling on the
win and tells you whether the mechanism is worth it. Measure it with an
offline probe (imitate `experiments/021-naming-floor/run-floor.ts`):
deterministic-floor both legs, then for each new-leg sweep target, check
whether the prior leg has a descriptive name for the same binding
(positional/diff match, below). Report addressable vs new. Expectation:
most sweep targets that recur across releases are addressable; the win
should be most of the 709 → 658 gap plus a chunk of the residual.

### The mechanism — transfer prior names to minified sweep targets

The sweep targets escaped the STRUCTURAL matcher (that is why they are
still minified), so match them to the prior POSITIONALLY, the same way
the reconcile pass does: a minified binding on a line that is
identical-after-blanking to a prior line carrying a descriptive name is
the same binding. Concretely, before the LLM sweep:

1. Generate the current (post-deterministic-floor) output text.
2. `computeNormalDiff(priorVersionCode, currentText)` and
   `parseNormalDiff` (both exported from `diff-reconcile.ts`).
3. Resolve each sweep target's position to its prior name via the same
   position→binding logic the reconcile pass uses
   (`resolveCandidates`, `diff-reconcile.ts:670` — currently internal;
   export or generalize it, don't fork it — CLAUDE.md unify rule).
4. For a target whose prior counterpart is DESCRIPTIVE
   (`!isMinifiedName(priorName)`), transfer it via
   `attemptValidatedRename` — this is the asymmetric case, safe.
5. Only the residue (no prior match, or prior also minified) goes to
   `sweepMintedNames`' LLM path.

**The cleaner realization worth trying first:** the reconcile pass
(`--reconcile-prior-diff`) already does step 4 for you AFTER generate —
its asymmetric tier snaps minified→descriptive. So the minimal change may
be simply: **do not LLM-name a sweep target that has a prior descriptive
counterpart; leave it minified and let the existing reconcile pass
transfer it.** LLM only the residue. That reuses the entire reconcile
machinery and shrinks the new code to a prior-counterpart CHECK. Prototype
both; measure which lands the noise lower. (Beware: the reconcile pass
only runs under `--reconcile-prior-diff`; if the sweep relies on it, the
sweep must require it or fall back to transferring inline.)

### Thread the prior into the floor step

`maybeRunNamingFloor` (`plugin.ts:266`) and `NamingFloorDeps`
(`plugin.ts:251`) do NOT currently receive `priorVersionCode`. Add it (the
option exists at `plugin.ts:115`). The sweep becomes prior-aware only when
a prior is present; with no prior it falls back to today's LLM sweep (the
first-floored release, which stabilizes next hop).

### Gates (precision over recall — project law)

- Transfer a prior name only on an EXACT positional match (the reconcile
  pass's existing 1:1-after-blanking + unanimous-per-binding gates). On
  any ambiguity, skip to the LLM or leave minified.
- Every rename goes through `attemptValidatedRename`
  (`validated-rename.ts:168`); rejection means skip.
- Never transfer a prior name that is itself minified (that is reroll —
  leave it for the LLM or a later floor).
- Respect eval/with taint (`isBindingEvalTaintFrozen`,
  `soundness.ts`) — already honored by `collectSweepTargets`.
- The structural invariant + output validation run after the floor
  (`plugin.ts`, unchanged) and must stay clean.

---

## Runbook

### Checks (before every commit — CLAUDE.md)

```bash
npm run check   # typecheck + prettier + biome (complexity ≤ 15) + unit + fingerprint
```

Red/green TDD is the house rule: failing test first, then the fix.

### The LLM box

Owned hardware — do NOT ration tokens, only wall-clock time matters.
Endpoint `http://<llm-host>:8000/v1`, model `openai/gpt-oss-20b`,
`HUMANIFY_API_KEY=local`. Check: `curl -s -m 3
http://<llm-host>:8000/v1/models`. Knobs: `HUMANIFY_CONCURRENCY=120`,
`HUMANIFY_REASONING_EFFORT=low`.

### The steady-state measurement protocol (BOTH legs floored)

The sweep's noise is only visible when both legs are floored (a one-leg
run against an unfloored prior is the transitional 1,626-hunk artifact —
see exp021 RESULTS; do not measure on it). Reuse the exp021 offline
harness — it is faithful (identical passes on the real artifacts, no
re-humanify needed):

```bash
# 1. floor+sweep both legs offline (add prior-awareness to run-floor.ts, or
#    a new run-floor that takes --prior)
npx tsx experiments/021-naming-floor/run-floor.ts \
  /tmp/exp019-chain/cc-119-lineage/runtime.js --sweep --out /tmp/e022/119.js
npx tsx experiments/021-naming-floor/run-floor.ts \
  /tmp/exp016-r1/cc-120/runtime.js          --sweep --out /tmp/e022/120.js
# 2. reconcile the new leg against the prior (exp020 harness)
npx tsx experiments/020-tail-polish/run-reconcile.ts \
  --new /tmp/e022/119.js --prior /tmp/e022/120.js --descriptive --apply \
  --out /tmp/e022/119-reconciled.js
# 3. diff + classify
diff /tmp/e022/119-reconciled.js /tmp/e022/120.js > /tmp/e022/diff.txt
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/e022/diff.txt 10
python3 experiments/021-naming-floor/census-minted-tokens.ts /tmp/e022/119-reconciled.js
```

The catch: prior-awareness needs the prior leg ALREADY floored to
transfer FROM. So floor leg 120 first (no prior), then floor leg 119 with
120-floored as its prior. That is the two-hop the exp021 RESULTS
describes; wire it into the harness.

If `/tmp/exp019-chain` or `/tmp/exp016-r1` are gone, regenerate per
exp020's README runbook (`run-phase2.sh` + `run-chain.sh`).

### Baseline (exp021, branch `exp021-naming-floor`)

- deterministic floor + reconcile: **658 noise / 411 census / leg**.
- full floor (+ sweep) + reconcile: **709 noise / 211 census / leg**.
- sweep: 199 of 273 named, names read well and context-aware.
- genuine held **1,868–1,975**.

### Success criteria

- `--naming-floor-sweep` steady-state noise **≤ deterministic floor
  (~658)** — i.e. the sweep stops adding noise — while keeping the census
  win (~211). The ideal: both metrics better than exp021 simultaneously.
- Genuine stays **1,868–1,975**. A rise means a prior name was
  transferred onto the wrong binding — investigate before celebrating.
- Parse + `checkStructuralInvariant` + output validation clean; zero
  internal errors.
- Spot-check transferred names: each should be "the same binding named
  the same way as last release."

### Tripwires

- Genuine rises above ~1,975 → a positional match transferred a prior
  name onto a genuinely different binding. The exact-match + unanimity
  gates should prevent it; if they don't, tighten, don't loosen.
- Noise does not drop → the addressable population (Step 0) was small, or
  the transfer isn't firing; re-measure Step 0 before adding scope.

---

## Guardrails / issues to avoid (project law)

- **Precision over recall.** A wrong name applied confidently is worse
  than a minified leftover. Every gate defaults to skip; log what you
  skip so "covered everything" is never implied.
- **Reuse, don't fork.** The positional matcher already exists in
  `diff-reconcile.ts` (`resolveCandidates`, the asymmetric tier). Export
  or generalize it; do not write a second one. CLAUDE.md: "Before writing
  new helpers, check if an existing one can be reused or generalized."
- **Every rename goes through `attemptValidatedRename`.** Never raw
  `scope.rename`, never textual replacement.
- **Deterministic where possible.** The whole point is to replace an
  unstable LLM name with a stable prior name; the LLM is the FALLBACK for
  genuinely-new bindings only.
- **Biome cognitive complexity ≤ 15**; colocated `*.test.ts`;
  `npm run check` green before every commit; commit incrementally; work
  on a NEW branch `exp022-prior-aware-sweep` off `main` AFTER exp021
  merges (if exp021 is still unmerged when you start, branch from
  `exp021-naming-floor` and note it). Do NOT merge — Andrew reviews.

## Code anchors (verified 2026-07-10 on `exp021-naming-floor`)

- Sweep: `src/rename/coverage-sweep.ts` — `isSweepTarget` :47,
  `collectSweepTargets` :67, `sweepMintedNames` :185 (the async LLM sweep
  to make prior-aware), `nameGroup` :149 (per-group request), grouping by
  enclosing scope.
- Reconcile machinery to reuse: `src/rename/diff-reconcile.ts` —
  `computeNormalDiff` :160, `parseNormalDiff` :215, `resolveCandidates`
  :670 (position→binding, internal — export/generalize),
  `isMinifiedName` :725, `RenameKind`/asymmetric tier :100 + gate logic
  around :918. Entry `reconcileDiffNoise` :1122.
- Floor wiring: `src/rename/plugin.ts` — `maybeRunNamingFloor` :266,
  `NamingFloorDeps` :251 (add `priorVersionCode`), `priorVersionCode`
  option :115, `namingFloorSweep` :139. Pass order: deterministic floor →
  sweep → generate → validate → reconcile (`maybeReconcilePriorDiff`).
- Validated rename: `src/rename/validated-rename.ts` —
  `attemptValidatedRename` :168, `attemptShadowingRename` (exp021),
  `getRenameRejection` :83.
- Census/targets: `src/rename/minted-census.ts` —
  `collectMintedBindings`, `isBunToken`; `isBindingEvalTaintFrozen` in
  `src/analysis/soundness.ts`.
- Prior-version transfer (the structural matcher these bindings escaped,
  for reference): `src/rename/prior-transfer.ts:329`
  `applyPriorVersionIfPresent`; `src/prior-version/prior-version.ts:102`
  `matchPriorVersion`.
- Reconcile step (post-generate): `src/rename/reconcile-step.ts:94`
  `runPriorDiffReconciliation`.
- Offline harnesses: `experiments/021-naming-floor/run-floor.ts`
  (floor + optional `--sweep`), `experiments/020-tail-polish/run-reconcile.ts`
  (reconcile), `experiments/021-naming-floor/census-minted-tokens.ts`.
- Metrics: `experiments/014-rename-noise-elimination/attribute-noise.py`,
  `experiments/013-bun-cjs-classification/classify-diff.py`.
- Lineage protocol: `experiments/016-diff-noise-convergence/run-chain.sh`
  (supports `EXTRA_HUMANIFY_FLAGS`).

## Adjacent, explicitly OUT of scope

- Improving the STRUCTURAL matcher to catch these bindings (the sweep is
  a coverage pass; positional prior-transfer is the tool here).
- The deterministic floor (WS1/WS3) — a pure win already, do not touch
  it beyond reading it.
- exp020 deferred items (swap-cycle temp-name renames; reconcile Tier 3;
  prior-aware reconcile diagnostics).
- The inlined-constant duplication finding (159 copies of the version
  metadata object → ~470 genuine-but-duplicated hunks): a real ceiling on
  the reviewable diff, but a DIFFERENT technique (diff-presentation
  collapse, or de-inlining) — not a naming pass. Note it in RESULTS if
  relevant; do not build it here.
