# The close-match anomaly: root cause, quantified

Date: 2026-07-05, branch `fix/transfer-validation`. Follow-up to
PHASE2-RESULTS.md item "the 40% close-match rate is the headline problem
and is suspiciously high".

**TL;DR: name-keyed placeholder assignment makes structural hashes
unstable under renaming. Humanifying the prior changes 18.3% of function
hashes, which directly unmatches 7,088 target functions and starves the
cascade/propagation out of another 4,489. That is the entire
5,884→17,445 close-match jump. The fix is binding-keyed placeholders
with property names as content — it eliminates both the instability and
the collision families behind review finding C4.**

## Correction to prior docs: the "14% vs 40%" comparison was invalid

The pipeline beautifies code BEFORE the rename plugin ever sees it
(`createBabelPlugin`: bautifier statement-splitting, `void 0`→
`undefined`, literal-comparison flips). The humanified prior is
beautified output. Every "standalone matching" number in
REMEASURE-AFTER-CASCADE.md (37,086 exact / 5,884 close) was measured
raw↔raw — a different representation than the full run matches.
The comparison was apples-to-oranges; only the corrected A/B below
isolates the real effect. (Empirically the raw↔raw and
beautified↔beautified rates come out nearly identical — but that had to
be measured, not assumed.)

Found the hard way: matching a RAW target against the beautified prior
exact-matches ~nothing, and the close-match stage then crashed the
harness — see "latent crash" below.

## Corrected A/B (measure-close-match-anomaly.ts)

Target: beautified v120 runtime.js (43,198 fns) — the pipeline's true
input. Only the prior varies. `matchPriorVersion` is called directly
(production entry point).

| prior                           | exact  | close  | none |
| ------------------------------- | ------ | ------ | ---- |
| beautified v119 (NOT renamed)   | 37,056 | 5,883  | 259  |
| humanified v119 (Run A′ output) | 25,479 | 17,445 | 274  |

- The humanified leg reproduces the real Run B′ diagnostics exactly
  (close = 17,445; exact 25,479 ≈ diag's 24,463 cached + ~1K
  alreadyNamed).
- The control leg matches the raw↔raw baseline — beautification is
  deterministic and version-stable.
- **Renaming the prior, alone, moves 11,577 functions out of exact
  match** (11,488 exact→close, 89 exact→none).

### Where the 11,577 went

| bucket                                                 | count | share |
| ------------------------------------------------------ | ----- | ----- |
| target hash ABSENT from humanified prior (instability) | 7,088 | 61%   |
| hash present, cascade/propagation failed (knock-on)    | 4,489 | 39%   |

The knock-on bucket is the same root cause once removed: resolution
stats show propagation collapsed 7,313→2,935 (−4,378) and unmatched
exploded 676→8,194. Hash-unstable functions vanish as call-graph
anchors; `calleeHashes` filters compare callee hashes that are
themselves unstable; the moved samples sit in 47–98-member collision
buckets where the starved tiebreakers can no longer discriminate.

## Mechanism, decomposed (classify-anomaly-mechanisms.ts)

Aligned beautified-v119 ↔ humanified-v119 function pairs (same
beautified AST, the ONLY difference is renaming; index-aligned, counts
equal at 42,981):

- hash same: 35,111 — hash changed: **7,870 (18.3%)**
- Among changed: split-only 2,167 · merge-only 3,292 · both 2,411 ·
  **unexplained 0 · structure-differs 0** — the model is complete.
- **5,482 of the 5,703 merge cases (96%) involve a property/object-key
  position.**

Terms (placeholders are keyed by NAME STRING in `normalizeAST`,
structural-hash.ts:505-513, one shared counter per function, property
names and object keys included):

- **split** — the minifier reused one name for two bindings (sibling
  scopes); humanify diversifies them → one placeholder becomes two →
  every later ordinal shifts → different hash.
- **merge** — two different names become one: the LLM names two
  bindings identically (`error`, `cache`), or — the dominant case — a
  renamed binding collides with a STABLE property name that shares the
  counter (`cache` the variable vs `.cache` the property).

## Latent production crash found en route

`scorePairs` (src/analysis/close-match.ts:148) materializes every
≥0.8-cosine pair into one array. With both unmatched sides at ~40K
(as happens when exact matching underperforms), the candidate array
exceeds V8's max array length → `RangeError: Invalid array length`.
The real Run B′ survived only because exact matching kept unmatched
sets at ~18K. Fix independently of the hash work: bounded top-K per
old id (heap or per-old best-N), which also removes the quadratic
memory cliff.

## The fix (review C10, now fully evidenced)

Key placeholders by **resolved binding identity**, not name string, and
stop placeholder-ing non-binding identifiers:

1. Identifier occurrences that resolve to a binding (params, locals,
   nested-fn locals, closure/module refs): placeholder per Binding
   object, ordinals assigned by first occurrence in serialization walk
   order. Splits and merges both become impossible — same binding ⇒
   same placeholder, different bindings ⇒ different placeholders,
   regardless of names.
2. Non-computed member property names and object keys: keep VERBATIM as
   content. Minifier-stable, so cross-version safe; removes the
   dominant merge class (96%); and it shatters the property-erased
   collision families (`(e)=>cache.get(e)` vs `(t)=>registry.delete(t)`
   currently hash identically — review C4's blind-match enabler).
3. Free identifiers (no binding: true globals, `undefined`): verbatim.
4. Labels: placeholder in a separate `label:` namespace (minifiers
   rename labels; humanify doesn't).

Requires `computeStructuralHash`/`buildPlaceholderMapping`/
`computeBindingFingerprint` to take a NodePath (all call sites have
one). Changes every hash — fingerprint snapshots re-baseline
deliberately.

### Expected impact (to re-measure after the fix)

- Prior hash instability 18.3% → ~0.
- Humanified-prior exact recovers toward the 37K control; close falls
  toward the ~5.9K floor. Direct effect ≈ +7K exact; knock-on recovery
  of anchors/propagation should claim much of the remaining 4.5K.
- Every recovered function transfers its FULL body name set instead of
  name+params — the body-local LLM drift that dominates the
  v119→v120 diff noise (PHASE2-RESULTS) disappears for that
  population, and ~7K fewer LLM calls per run.
- Bucket shattering from property-name content should also cut
  stillAmbiguous (9,263 in the humanified leg) and reduce C4
  singleton-blind-match exposure — measure `structuralHashUnique`
  before/after.

## RESULTS AFTER THE FIX (2026-07-05, same session)

Binding-keyed placeholders + property/free-name content landed in
`src/analysis/structural-hash.ts` (all call sites take NodePaths now).
Same A/B, same artifacts, new hashing:

| prior                           | exact      | close     | none |
| ------------------------------- | ---------- | --------- | ---- |
| beautified v119 (control)       | 35,594     | 7,346     | 258  |
| humanified v119 (Run A′ output) | **35,241** | **7,682** | 275  |

- **Prior hash instability under rename: 18.3% → 0.4%** (7,870 → 190 of
  42,981 functions). Hashing is cross-process deterministic (0/43,198
  mismatches between the two legs' independent target builds).
- **Renaming-induced exact→close movement: 11,577 → 354** (97%
  eliminated). The renaming penalty on close-matches is now 336
  functions (7,682 vs 7,346 control), down from 11,562.
- Projected pipeline effect: exact/cached 24,463 → ~35,241 (+44%), the
  drifting close+fresh population ~24,252 → ~7,957 (−67%) — the
  dominant diff-noise term — plus ~10K fewer LLM calls per run.
  Needs the full LLM run to confirm actual diff-line numbers.
- The control leg's exact count dropped 37,056 → 35,594: the stricter
  hash refuses ~1,460 v119↔v120 pairs whose property names or free
  identifiers GENUINELY differ (real cross-version changes the
  property-blind hash used to paper over — each was a stale-name
  transfer risk). They now close-match with prior context instead.
  Verified instance in the preact e2e fixture: 10.24.0's `e.some(...)`
  exact-matched 10.25.0's `e.forEach(...)` under the old scheme; the
  new scheme separates them (snapshot re-baselined, one function).
- Cross-minifier match rates in all other e2e fixtures unchanged
  (snapshots passed without update).

### Residual (follow-up, low priority)

354 functions still move exact→close under a humanified prior:
158 hash-absent (the 0.4% residual instability — mechanism not yet
classified; the split/merge classifier models the OLD name-keyed
scheme, so its labels no longer apply) + 196 cascade/ambiguity effects
(large collision buckets, e.g. a 44-member and a 14-member bucket in
the samples). Also `stillAmbiguous` rose (5,219 → 5,807 control) —
bucket shattering resolves more functions as unique (22,770 → 25,247)
but leaves fewer disambiguation signals for what remains; C4/C5/C7
cascade fixes are the right tool there.

## Reproduction commands

```bash
# A/B legs + transition matrix (~3 min/leg, no LLM)
node --max-old-space-size=16384 --expose-gc --import tsx/esm \
  experiments/013-bun-cjs-classification/measure-close-match-anomaly.ts prep
... run --prior beautified --out /tmp/exp013-anomaly/leg-beautified.json
... run --prior humanified --out /tmp/exp013-anomaly/leg-humanified.json
... compare /tmp/exp013-anomaly/leg-beautified.json /tmp/exp013-anomaly/leg-humanified.json

# Mechanism decomposition (~16s)
node --max-old-space-size=16384 --import tsx/esm \
  experiments/013-bun-cjs-classification/classify-anomaly-mechanisms.ts
```

Inputs: `/tmp/exp013-remeasure/v11{9,20}/runtime.js` (raw unpack cache),
`/tmp/exp013-phase2/cc-119/runtime.js` (Run A′ humanified output).
