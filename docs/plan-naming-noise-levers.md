# Implementation plan: cut cross-version naming noise (Lever 2 + split/alias stability)

Written 2026-07-21, off the measured residual after fix #1 (single-vote pin,
branch `feat/single-vote-binding-inherit`). Companion to
`issue-naming-instability-reconcile.md`. Scope: the two levers we're taking
forward — **(A) constrain the LLM when it must run**, and **(B) split/alias
stability**. The other Lever-1 ideas (close-match body-local inheritance,
function-local single-vote pin) are intentionally deferred and not covered here.

## The measurement that frames all of this

On the pinned 215→216 hop, the naming-churn subset of the diff is **~4,452
rename line-occurrences across 1,694 distinct bindings** (the rest of the
72k-line diff is real 216 feature change). Decomposed:

| category                                  | bindings | churn lines | share   |
| ----------------------------------------- | -------- | ----------- | ------- |
| descriptive synonym flip (fn-body locals) | 1,232    | 2,781       | 62%     |
| require-alias / split relocation          | 206      | 1,001       | **22%** |
| prefix/qualifier add                      | 83       | 316         | 7%      |
| trivial local (`i→idx`)                   | 114      | 182         | 4%      |
| suffix/decoration drift                   | 40       | 142         | 3%      |
| ordinal-only (`foo7→foo9`)                | 19       | 30          | 1%      |

**The decisive control:** two rebuilds of 216 with _identical code and identical
prior_ differ by **~19,855 lines / 1,782 bindings** — pure LLM nondeterminism,
no real change. The module-binding layer was byte-stable across both runs (539
pins + 592 mints each); **100% of that variance is function-local naming**, in
the 2,135 functions that reach the LLM (1,259 no-context + 876 close-match).
Temperature is already 0 (`openai-compatible.ts:125`), so this floor is
structural to concurrent batch serving — it cannot be sampled away. The only
durable wins are (A) making each LLM pick deterministic-by-constraint and (B)
keeping split assignments stable so path-derived aliases don't churn.

---

## Lever A — constrain the LLM to the expected name

Three sub-items, cheapest/highest-coverage first.

### A1. Per-identifier expected-name hints (the big one)

**Problem.** Today the only prior-name signal for the identifiers the LLM is
being asked to name is a flat bag: `prompts.ts:78` emits
`Reuse these names from the prior version for unchanged logic: a, b, c`. It does
not say _which_ minified identifier each name belongs to, so the model treats it
as loose inspiration and — under batch nondeterminism — flips
`caughtError→decisionOutcome`, `progressEventMessage→commandMeta`, etc.

**Evidence.** The 62% "descriptive synonym flip" bucket is predominantly
close-matched functions' body locals; the run1/run2 control proves the flip is
arbitrary, not driven by real change.

**Root cause + code location.**

- `src/llm/prompts.ts` `buildBatchRenamePrompt` / `renderAlreadyRenamed` — the
  prompt assembly. `priorVersionNames` arrives as a `string[]` (flat list).
- `src/rename/processor.ts:412` builds `priorStemIndex` from
  `fn.priorVersionNames` and only uses it for post-hoc _stem snapping_
  (`snapSuggestionToPrior`), not as a prompt-time per-identifier hint.
- The mapping we need already exists: for a close-matched function, the prior
  placeholder table (`CloseMatchInfo` in `prior-version.ts`, and
  `translatePriorNames`) knows _this_ minified slot was named `caughtError` last
  version — even for locals `statement-align` could not safely auto-transfer
  (changed enclosing statement). We currently throw that mapping away for the
  non-transferable locals; we should keep it as a _hint_ (not an apply).

**Change.**

1. Extend `CloseMatchInfo` (or a sibling map on `FunctionNode`,
   e.g. `priorNameHints: Record<minifiedName, priorName>`) to carry the
   per-identifier prior name for every slot the close-match resolved, including
   the ones that did **not** meet the auto-transfer precision gate.
2. Thread it into the batch request (`processor.ts buildFunctionCallbacks` →
   `buildRequest`) alongside the existing `alreadyRenamed`.
3. In `prompts.ts`, replace the flat "reuse these names" line with a per-id block:
   ```
   These identifiers were named as follows in the prior version. Reuse the exact
   name unless the variable's role changed:
     xK7  ->  caughtError
     m3   ->  toolResultMeta
   ```
   Keep it phrased as "unless the role changed" so a genuinely-repurposed binding
   can still diverge (precision > recall stays intact — a hint is not an apply).

**Measure.** Re-run 216 and diff against the run1/run2 floor established here
(`scratchpad/measure/diff-run1-run2.txt`). Success = the descriptive-flip bucket
shrinks AND a fresh run1'/run2' pair (identical code) shows less binding
variance than 1,782 — i.e. the hint reduces nondeterminism, not just this run's
luck. Guardrail: the pure-rename invariant and the wrong-pin discipline still
hold (a hint can't corrupt — validation still gates the actual rename).

**Risk.** Low. It's prompt-only; the model can still override on a real role
change. Watch prompt length on functions with many locals (cap the hint list,
prefer the highest-confidence slots).

### A2. Post-LLM synonym snapping

**Problem.** `snapSuggestionToPrior` (`processor.ts` / `prior-name-snap.ts`) only
snaps _same-stem decorations_ (`identityVal→identityVar`). It does not catch a
full synonym flip (`caughtError→decisionOutcome`) even when we're confident it's
the same binding.

**Change.** When a binding is a confident cross-version match (the matcher
mapped it, or A1's hint applied to this exact slot) and the LLM returns a name
that is neither the prior name nor a same-stem variant, snap it back to the prior
name. Gate on the SAME evidence fix #1 uses (agreeing role / exact-slot identity)
so we never snap a genuinely-changed binding. This is the safety net under A1:
A1 reduces the flips, A2 catches the residue.

**Code location.** `src/rename/prior-name-snap.ts` (`buildPriorStemIndex` /
`snapSuggestionToPrior`) + the `transformSuggestion` hook at
`processor.ts:418`.

**Measure/Risk.** Same harness as A1. Risk is the wrong-pin risk from the issue
doc — treat a snap onto a repurposed binding as a failure; keep the role gate.

### A3. Mechanical decoration-bug fixes (~11%, cheap)

**Problem.** Concrete, semi-deterministic bugs:

- **Double suffix:** `upstreamConfigVal → upstreamConfigValVal` (the naming floor
  re-appends `Val` to a name that already ends in it).
- **Suffix drift:** `errorMessage → errorMessageText`,
  `streamErrorOrTimeout → streamErrorOrTimeoutError`.
- **Ordinal reshuffle:** `foo7 → foo9`, `React219 → react23`.

**Code location.** The naming-floor / minted-name path
(`src/rename/class-id-floor.ts`, `coverage-sweep.ts`, `sweep-step.ts`) and the
decoration-retry logic (`decoration-retry.ts`). The double-`Val` is a missing
"already carries this suffix" guard; the ordinals are the mint counter not being
prior-seeded.

**Measure.** These are countable directly in the diff classifier
(`scratchpad/measure/classify-tokens.py`, the `suffix/decoration` and `ordinal`
categories). Target: drive both to ~0.

---

## Lever B — split / alias stability (the 22%)

**Problem.** A require-alias is a _pure function of the target file's path_
(`cjs-emit.ts` `nsCandidates` camelCases the path segments;
`error-messages/auth-manager.js → errorMessagesAuthManager`). This is stable
**by design** — it churns only when a module-level binding's **home file
changes** between versions. When it does, every importer's
`const <alias> = require("<path>")` rewrites, cascading across the whole tree:
`taskSerializer → errorMessagesAuthManager` alone is **497 diff lines** (11% of
all naming churn) from one relocated barrel of exports. The bucket is
power-law — a handful of heavily-imported modules dominate the 22%.

**Root cause + code location.** The split assigns each top-level statement to a
file in `src/split/stable-split.ts` `assignWithPrior`, via tiers:

1. **hash tier** (`hashTier`) — content-identity: the statement's
   rename-invariant hash occurs the same count in both releases and every prior
   occurrence was in one file → inherit it. Order-free, name-free, strongest.
2. **name-vote tier** — the statement's declared names vote for their prior file.
3. **ordinal tier** — kth declaration of a name inherits kth prior file.
4. **locality residue** — anything unresolved follows its preceding neighbor.

A binding relocates (and its alias churns) when it falls through hash+name+ordinal
to **locality residue**: its content changed (hash miss) _and_ its name flipped
(vote miss, often an LLM synonym flip — so Lever A indirectly helps here too),
so it lands wherever its neighbor did — a different file. The 216 split log shows
`inherited 34548/35903 (21223 hash, 894 ordinal, 1355 residue by locality)`; the
1,355 locality-residue statements are the relocation population.

**Change — add a binding-identity tier above locality residue.** The rename
layer already establishes cross-version binding identity: the fingerprint matcher
maps binding-X₂₁₆ ↔ binding-Y₂₁₅ (`matchPriorVersion` /
`project_binding_cascade_identity`, plus the `binding-role` work from fix #1).
That identity is exactly the signal the split's name-vote tier lacks when a name
flips. Feed the matcher's binding→prior-binding map into `assignWithPrior` as a
tier between ordinal and locality: if statement S declares a binding whose
matched prior counterpart lived in file F, inherit F. This is order-free and
name-free like the hash tier, but survives content changes the hash tier can't.

- Plumb the prior-match map from the rename pipeline (where
  `applyPriorVersionIfPresent` runs) through to the split stage
  (`stable-split.ts` is invoked later in `cjs-emit`/`commands/unified.ts`).
- Add `bindingIdentityTier(body, priorMatchMap, prior.nameToFiles)` returning a
  per-statement file vote; slot it into `assignWithPrior` before the locality
  fallback, with the same unambiguous/unanimous discipline the other tiers use.

**Contained fallback (if the plumbing is heavy).** Inherit the prior _alias_
for a require target when the target file's export set is ~unchanged, decoupling
importer churn from file naming. Weaker (fights the pure-path-function design)
and lower priority; prefer the binding-identity tier.

**Measure.** The require-alias share is directly measurable — see
`scratchpad/measure/` (`pairs-216.json` + the alias-classifier snippet): 206
bindings / 1,001 lines today. Target: cut the locality-residue count and the
alias-churn line count together. Guardrail: the split's own concat-equivalence /
`reconstructBody` invariant must still hold (a wrong file assignment would break
the byte-exact rebuild and fail loudly).

**Risk.** Medium — it touches the split assignment, which is load-bearing for
runnability. The binding-identity tier must keep the same "abstain on any
ambiguity → locality" discipline; a wrong inherit corrupts nothing structurally
(the invariant catches it) but could move a statement to a semantically-wrong
file. Gate on unique + unanimous matches only.

---

## Suggested sequencing

1. **A1 (per-identifier hints)** — cheapest, hits the dominant 62% bucket, and
   A/B-able against the run1/run2 floor already captured.
2. **A2 (post-LLM snapping)** — the safety net under A1; small, same harness.
3. **B (binding-identity split tier)** — biggest single-binding wins
   (`taskSerializer` = 497 lines), reuses fix #1's matcher identity, but touches
   the split so it's the heaviest.
4. **A3 (decoration bugs)** — cheap cleanup, do alongside whenever.

All measured on the benchmark hops (215→216, 202→203, 185→186) with the quiet
controls (213→214, 214→215) and the run1/run2 nondeterminism floor as the
"reducible vs irreducible" yardstick.
