# Exp021 — naming floor: minted leftovers 744 → 211, and an honest split

Branch `exp021-naming-floor` (off `main` after exp020 merged). Goal:
leave **no minted (Bun-minted) identifiers** in the output — the user's
directive that flows from exp020's downgrade gate ("never rename TO a
minified name; the real fix is to not leave any minified in the first
place").

## Headline

The floor resolves the minted-token census that had held at **744**
bindings a full run left behind, and it splits cleanly into a
**deterministic part that is a pure win** and an **LLM part that is a
trade-off**:

| config (measured on the exp020 lineage pair, both legs)   | minted census / leg | lineage noise hunks | genuine |
| --------------------------------------------------------- | ------------------- | ------------------- | ------- |
| exp020 baseline — reconcile only, no floor                | 744                 | 662                 | 1,901   |
| **deterministic floor** (derive + undecorate) + reconcile | **744 → 411**       | **658**             | 1,900   |
| full floor (+ LLM sweep) + reconcile                      | **744 → 211**       | 709                 | 1,880   |

Genuine held in the 1,868–1,975 band throughout. All runs: structural
invariant + output validation clean, zero internal errors.

The table's steady-state numbers floor BOTH legs — the production scenario
where each release inherits the previous floored output as its prior. A
transitional run confirms why that matters (below).

### In-pipeline confirmation + the transitional "one-leg-floored" artifact

A real `--naming-floor --naming-floor-sweep --reconcile-prior-diff`
lineage leg through the actual CLI (against the exp016 UNfloored v120
prior) confirms the integration end to end: in-pipeline census **744 →
229**, floor ran (derived 328, undecorated 6, swept 182 / 100 skipped),
reconcile snapped 589, parse + invariant clean, genuine 1,905.

Its noise, however, is **1,626 hunks / 46% share** — much higher, and
NOT a regression. It is the first-floored-run artifact the brief
predicted: v119 is floored while its v120 prior is still minified, so
~500 freshly-named bindings become asymmetric noise, and reconcile's
downgrade gate correctly REFUSES to un-floor a descriptive name back to a
minified one. This is exactly why the steady-state measurement floors
both legs; in the production chain the floored v119 becomes v120's prior
next release, v120 floors too, and the two converge to the 658/709 band.

**The deterministic floor is strictly better than the exp020 baseline on
both axes** — fewer minified leftovers (744 → 411) AND marginally less
lineage noise (662 → 658) — because the class-expression inner-id
derivation is cross-version stable by construction, so those names
collapse to zero diff noise. It ships under `--naming-floor`.

**The LLM sweep is a trade-off**: it removes another ~200 leftovers
(411 → 211) but adds ~50 noise hunks (658 → 709), because it is not yet
prior-version-aware — it names the same binding differently across the
two legs, turning previously-identical minified tokens (invisible in the
diff) into fresh rename-noise hunks. It is opt-in under
`--naming-floor-sweep`.

## What was built (four workstreams, all `npm run check` green)

- **WS0 — truthful census** (`src/rename/minted-census.ts`). Walks the
  post-rename AST once, classifies surviving eligible minted bindings by
  family (classExprId / fnExprId / param / fnDecl / varOther), and
  reports them in `CoverageSummary.mintedCensus` (and `--diagnostics`).
  "Not renamed" stops lying: the exp020 run's own diagnostics claimed
  5 failed / 3+18 not-renamed while **744** minted bindings survived. The
  experiment census script shares this module.
- **WS1 — class-expression inner-id derivation** (the 328,
  deterministic, no LLM). `X = class q {}` → `X = class X {}` by copying
  the assignment target / declarator / property key. Its novel safety
  surface is `attemptShadowingRename` in validated-rename.ts: the derived
  name is the visible outer binding, which the standard path rejects as
  target-visible, so this variant permits the intentional shadow while
  adding a subtree-capture check (`class q extends X {}` → skip).
  Offline: **328/328 derived, zero skips**, names read as real classes
  (`AbortError extends HttpError`, `AnthropicClient extends
AnthropicClientVal`).
- **WS3 — decoration retry**. `initializeApp_` → `initializeApp` when the
  collision blocker has moved on, else left alone. 6 of 15 undecorated in
  a single leg.
- **WS2 — LLM coverage sweep** (`src/rename/coverage-sweep.ts`,
  opt-in). Force-names the remaining minted survivors (params, whole
  decls, var/let) one code-window request per enclosing-scope group,
  applied through the validated path. The precision surface is TARGETING:
  `isSweepTarget` is stricter than the census token shape — short,
  no-embedded-word, non-SCREAMING*CASE, no pure-`*`/`$` — so the census
false positives are never swept. Offline: **199 of 273 named**
(74 collisions / declines), and the names read well and context-aware
(`j2\_ → interopRequireWildcard`, `CR → createRequire`,
`U → commonJSModuleWrapper`, the same `H`→`isActive`/`config`/`apiCall`
  per function). No good-name-overwritten failure — the strict predicate
  held.

Also folded in a review follow-up: the eval/with freeze rule is one
shared predicate (`isBindingEvalTaintFrozen`), consumed by the
diff-reconcile and all floor passes.

## The honest finding: the sweep needs to be prior-aware

The class-id derivation wins on the diff because it is DETERMINISTIC —
the same class structure yields the same name in both legs, so a binding
that was `uq`↔`vq` reroll noise becomes `BaseError`↔`BaseError` (zero
noise). The sweep does not have that property: the LLM names each leg
independently, so a param that was `H`↔`H` (identical, no hunk) can
become `config`↔`handler` (a new hunk). The exp020 reconcile pass catches
some of it (it snapped 104 bindings, 984 → 709) but not all — the swept
params/vars mostly don't meet the descriptive tier's clean-declaration
gate.

The fix is clear and is the natural next step: make the sweep
**prior-version-aware** — feed the prior leg's swept name for the same
binding (the exact mechanism the main pipeline uses to carry names
across releases). That converts the sweep from a trade-off into a pure
win, because both legs would then agree. Until then, `--naming-floor`
(deterministic) is the default; `--naming-floor-sweep` is opt-in for when
output completeness matters more than diff noise.

## Why minted leftovers are worth removing at all (recap)

Every minted token in an output is future cross-version noise: Bun
re-mints tokens between builds (reroll), and a binding one leg names
while the other leaves minified becomes asymmetric noise or a downgrade
conflict. The lineage carries names forward — and carries GAPS forward
too: an exact-match transfer replays the prior leg's names, a pair whose
old and new names are equal is skipped, and the function is settled, so a
param the prior left minified re-settles minified every release. Flooring
is a one-time cost per binding.

## Remaining minted after the full floor (211 / leg)

- ~137 the strict sweep predicate deliberately keeps minified — they are
  descriptive names the census over-counted (`RP_ConstructorKey`,
  `OS_MODULE`, `ec2Metadata*`, `s3Config`, `X_CLIENT_*`, `ID_TOKEN`) plus
  SCREAMING_CASE constants. Correctly left alone.
- ~74 sweep targets skipped this round (collision with an in-scope name,
  or the LLM returned the original).

## Next candidates

1. **Prior-aware sweep** — the headline follow-up above. Turns
   `--naming-floor-sweep` from a diff-noise trade-off into a pure win.
2. **Decoration retry across the lineage** — 9 of 15 stayed decorated
   because the blocker persisted in the single leg; a second hop frees
   more.
3. The exp020 deferred items still stand (swap-cycle temp-name renames in
   reconcile; prior-aware reconcile diagnostics).

## Reproduce

```bash
# census any output
npx tsx experiments/021-naming-floor/census-minted-tokens.ts <output.js>

# offline floor (deterministic) + re-census + invariant check
npx tsx experiments/021-naming-floor/run-floor.ts <output.js> --out floored.js
# add --sweep to also run the LLM coverage sweep

# fresh lineage leg through the real pipeline
EXTRA_HUMANIFY_FLAGS="--naming-floor --reconcile-prior-diff" \
  CHAIN_OUT=/tmp/exp021-chain \
  bash experiments/016-diff-noise-convergence/run-chain.sh
```
