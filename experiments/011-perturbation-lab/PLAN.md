# Perturbation Lab — Matching Improvement Plan

## Long-term goal

Take minified library A v1 and A v1.1, humanify both, and the diff of the
humanified outputs should contain *only* the lines that actually changed
in the source between versions. Everything unchanged stays byte-identical.

This requires:
- **Deterministic, stable function identification** across minified versions
- **Cache-based renaming**: unchanged functions reuse their prior humanified
  names rather than re-running through the LLM
- **High-precision matching**: no wrong-twin swaps, no ambiguous pairings

## Current state (baseline)

### Harness

`experiments/011-perturbation-lab/` applies known AST transformations to
source, minifies both versions, runs `matchFunctions`, scores vs. source-
level ground truth. No source maps needed.

- `corpus.ts` — set of source files (currently r1b-synthetic, disambiguation)
- `perturbations/` — AST transforms: identity, addConsoleLogTo
- `ground-truth.ts` — computes expected matches via source-level exactHash
- `scoring.ts` — per-v1-function TP/FN/TN classification
- `run.ts` — CLI, writes `results/<name>.json`

Run: `npx tsx experiments/011-perturbation-lab/run.ts --name <label>`

### Baseline numbers (committed at `results/baseline.json`)

28 runs (7 perturbation plans × 4 minifiers). Avg accuracy 92.3%, F1 94.7%.

- **terser / esbuild / bun-default: 100%** on every perturbation, including
  identity sanity check.
- **SWC: stuck at 69% (9/13) across the board.** Identical failure profile
  regardless of which function is perturbed, or even under identity — means
  SWC has an irreducible floor from inlining.

### Root cause of the SWC floor

SWC inlines calls to the paired getters/setters (`getCount`, `getLabel`,
`setCount`, `setLabel`). The functions remain in the returned object literal
(public API), but nothing in-module calls them anymore. Consequences:

- `callerShapes` = ∅ on both twins → identical
- `calleeShapes` = ∅ on both twins → identical
- Same `exactHash`, same features → `stillAmbiguous = 4`

## Key insight

Property keys in object literals / class methods / module exports are
preserved by **every minifier by default** (property mangling is off by
default because it breaks consumers). In SWC's output:

```js
return {
  getCount: function(){ return t },
  getLabel: function(){ return t },
  ...
}
```

The string `"getCount"` and `"getLabel"` are *right there*. We just don't
currently extract them as a fingerprint feature.

## Plan

### Phase 1 — Add `memberKey` fingerprint feature

Extract the property key a function expression is assigned to. Applies when
the function node is:

- Value of an `ObjectProperty` (e.g. `{ getCount: function() {...} }`)
- Body of an `ObjectMethod` or `ClassMethod` (e.g. `{ getCount() {...} }`)
- Right-hand side of an `ExportDefaultDeclaration` / `ExportNamedDeclaration`
  binding

Store on `FunctionFingerprint` as `memberKey?: string`.

**Cascade placement**: right after exactHashUnique, before calleeShapes.
Property keys are more discriminating than shapes and O(1) to compare.
Add new resolution stage `memberKeyResolved`.

**Files**:
- `src/analysis/types.ts` — add `memberKey` to `FunctionFingerprint` and
  `memberKeyResolved` to `ResolutionStats`
- `src/analysis/function-fingerprint.ts` — extract and populate `memberKey`
  in `buildFullFingerprint`
- `src/analysis/fingerprint-index.ts` — add filter function, new stage in
  cascade, increment counter
- `src/analysis/fingerprint-index.test.ts` — unit tests for the new stage
- `test/e2e/harness/snapshot.ts` + snapshot JSONs — field rename / addition

### Phase 2 — Verify with experiment harness

**Add perturbations**:
- `renameProperty(key, newKey)` — swap a property key. Expected: that one
  function unmatched, rest matched. Tests we *use* keys, don't over-match.
- `swapPropertyOrder` — reorder keys in an object literal. Expected: all
  match. Tests we don't accidentally depend on sibling order.

**Run and compare**:
```
npx tsx experiments/011-perturbation-lab/run.ts --name baseline   # exists
npx tsx experiments/011-perturbation-lab/run.ts --name member-key
npx tsx experiments/011-perturbation-lab/compare.ts baseline member-key
```

**Expected delta**: SWC 69% → 100% on r1b-synthetic and disambiguation.
Other minifiers stay at 100% (no regression).

**Build `compare.ts`** as part of this phase — takes two result JSONs, diffs
per-run TP/FN/accuracy, lists flipped pairs.

### Phase 3 — Audit humanify pipeline for property-key protection

Verify humanify's LLM renaming never touches property keys in preserved
positions. Keys that must NOT be renamed:

- Object literal keys (`{ foo: bar }` — `foo` is public)
- Class method names (`class X { foo() {} }`)
- Export binding names (`export { foo }`, `export function foo()`)
- Property access targets (`obj.foo` — can't rename without coordinated
  change of the object literal)

**Files to audit**:
- `src/rename/processor.ts`
- `src/rename/plugin.ts`
- Shadow-checking logic

If already safe, add an invariant test. If not, add a renamable predicate
that excludes these positions.

Separate audit: what happens when a local binding name *matches* a property
key (`const getCount = obj.getCount`)? The local binding is renamable, the
property key isn't.

### Phase 4 — Positional ground truth + A/B compare in the lab

Current scoring is hash-level only. It cannot detect "wrong twin" matches
(matcher picks `getCount ↔ getLabel` instead of the correct pairing) — both
have the same hash, so both count as TP.

**Add**:
- Tag each source function with a stable identity (e.g. AST path fingerprint
  from declaration site). Track through minification via source-map when
  available, or by best-effort position matching.
- Score per v1 function: did the matcher pick the *correct* v2 function, not
  just *any* same-hash v2 function?
- Report wrong-twin count separately (currently hidden inside TP).

**Success**: Phase 1 should show 0 wrong-twin matches. Without this
instrumentation we can't verify.

### Phase 5 — Cache-based renaming

Once matching is high-precision:

- Extract a `fingerprintHash → renameMapping` cache from humanified v1 output
- On v2: run matcher, apply cached renames to matched functions, only
  LLM-call on `unmatched` + `stillAmbiguous`
- Measure: % of LLM calls eliminated on minor version bumps

**Target**: ≥80% LLM call reduction on a minor version bump (e.g. preact
10.24→10.25).

### Phase 6 — End-to-end diff validation

The final test of the long-term goal:

1. Humanify v1 → output A
2. Humanify v2 using v1's rename cache → output B
3. `diff A B` should contain only source-level changes from v1→v2
4. Measure diff noise = (added/changed lines in diff) / (actual source diff lines)

**Fixtures to use**: existing ones. Commit a fixture-level assertion:
- `mitt 3.0.0 → 3.0.1`: no API changes expected → diff should be minimal
- `preact 10.24 → 10.25`: a few real changes → diff should show only those

## Success criteria per phase

| Phase | Metric | Target |
|---|---|---|
| 1 | Experiment F1 avg | ≥ 99% |
| 1 | SWC on r1b-synthetic, disambiguation | 100% |
| 2 | `compare.ts` shows 0 regressions vs baseline | yes |
| 3 | No property keys in LLM rename outputs | yes |
| 4 | Wrong-twin count visible in reports | yes |
| 4 | Wrong-twin count after Phase 1 | 0 |
| 5 | LLM calls avoided on minor version bump | ≥ 80% |
| 6 | Humanify diff / source diff noise ratio | ≤ 1.5× |

## Order of execution

Tight loop first, big bets later:

1. **Phase 1 + Phase 2** in one PR (small, concrete SWC win, verifiable)
2. **Phase 3 audit** after Phase 1 lands (may be zero work)
3. **Phase 4** anytime — standalone harness improvement
4. **Phase 5** after Phase 1-4 are solid
5. **Phase 6** as the end-to-end integration test

## How to resume after context clear

1. Read this file.
2. Read `experiments/011-perturbation-lab/results/baseline.json` for the
   current baseline numbers.
3. Read `src/analysis/fingerprint-index.ts` for the cascade.
4. Read `src/analysis/function-fingerprint.ts:77` (`buildFullFingerprint`) —
   this is where Phase 1's `memberKey` extraction lives.
5. Start Phase 1 unless numbers have changed materially.
