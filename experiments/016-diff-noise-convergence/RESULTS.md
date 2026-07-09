# Exp016 round 1 — reroll eliminated, votes pin drifted names (2026-07-09)

Branch `exp016-diff-noise-convergence`, run artifacts `/tmp/exp016-r1/`.
Fixes under test: binding-keyed statement-align (`c1fb8b9`), classes in
the module pool (`25b8ae5`), function-name votes (`ebb565c`), plus the
exp015-tail MF5 blob cap.

## Headline

| metric               | exp015 r2 (baseline) | exp016 r1 | delta    |
| -------------------- | -------------------- | --------- | -------- |
| noise hunks          | 5,788                | **3,929** | **−32%** |
| noise share of hunks | 75.0%                | **67.2%** | −7.8pp   |
| genuine-change hunks | 1,934                | 1,919     | stable ✓ |
| rename occurrences   | 10,206               | 7,757     | −24%     |
| distinct bindings    | 3,366                | 2,854     | −15%     |

Campaign trajectory (noise hunks): 22,998 → 6,206 (exp014) → 5,788
(exp015) → **3,929** — −83% cumulative; noise share 92.0% → 67.2%.

Buckets (occurrences):

| bucket          | exp015 r2     | exp016 r1      | delta    |
| --------------- | ------------- | -------------- | -------- |
| transfer-gap    | 7,536 (73.8%) | 6,512 (83.9%)  | −14%     |
| asymmetric      | 1,259 (12.3%) | 1,106 (14.3%)  | −12%     |
| minifier-reroll | 1,411 (13.8%) | **139 (1.8%)** | **−90%** |

Family view (`classify-decl-kinds.py`):

| family              | exp015 r2 | exp016 r1 | delta |
| ------------------- | --------- | --------- | ----- |
| function-decl names | 1,470     | 784       | −47%  |
| class-decl names    | 1,326     | 230       | −83%  |
| other               | 7,410     | 6,743     | −9%   |

## What the fixes did (verified in the run)

- **`function serializeWithLengthLimit` is IDENTICAL in both legs** —
  the flagship 423-occurrence binding (five different names in five
  runs) is pinned by caller votes. 100 function names propagated via
  votes in the incremental leg; the family dropped 1,470 → 784, and
  what remains is mostly sub-floor (single-voter / unmatched-caller)
  cases plus asymmetric coverage gaps.
- **Classes:** 2,465 module-binding propagations now include the class
  population; `y6→C6`/`HK→qK`/`m3→a3` are gone. Residual 230 occ =
  first-contact naming drift (`BasePatternMatcher→PatternMatcher`) and
  a handful of still-minified pairs.
- **Reroll is effectively closed** (139 occ / 61 bindings, was 7,350 at
  the exp014 phase-6 baseline). Megafunction-range noise is down to
  1,013 occurrences, mostly transfer-gap naming drift now.
- **Run health:** parse + invariant clean; genuine 1,919; ZERO
  context-length failures in either leg (first fully clean run — the
  MF5 blob cap closed the last one; fresh-leg module "not renamed"
  dropped 26 → 16). Cost: fresh 44.6M tokens / 11m38s (cheapest yet),
  incremental **3.7M / 3m57s** (round-3 exp014 was 5.9M / 8m27s — the
  mechanical-transfer share keeps growing).

## What remains (7,757 occurrences)

The top of the noise list is now entirely LLM naming instability:

1. **Decoration/collision flips** — `error→errorVal`,
   `identityRef→identityVal`, `configVar→config`,
   `appStateVal→appStateVar`, `hashAlgorithmsVal→hashAlgorithmsVar`.
   Likely collision-suffix order-dependence (which binding wins the
   bare name differs per leg), not LLM whim — needs an autopsy before
   fixing.
2. **Ordinal churn on repeated base names** — `reactInstance28→React95`,
   `reactLib124→ink8`: collision ordinals assigned by naming-completion
   order, which is racy across legs.
3. **Synonym drift on close matches** — `userQueryMessageItem→messageItem`,
   `JsonRpcSchema→rpcRequestSchema` (the case-flip schema family).
4. **Asymmetric 1,106** — `index→i`, `processMessageVal→q`,
   `handleKeyEvent→c_`: one leg named, the other left minified —
   v120-side coverage holes to autopsy (which pass dropped them).
5. The deep reservoir: 4,111 fresh-pool v120 functions with exact hash
   twins stuck in m:n ambiguous buckets (exp014's cracking problem) —
   every one of them is a freshly-named function whose locals all
   drift. The biggest structural lever left.

## Next round candidates (autopsy first, offline)

- Root-cause the decoration flips (diag join on `error→errorVal` and
  friends: transfer rejected → suffixed? LLM suggestion suffixed by
  resolveConflict? which leg diverged from the prior name?).
- Asymmetric v120 coverage holes (`processMessageVal→q`).
- Ambiguous-bucket cracking (neighbor/position evidence) if the above
  two don't clear the reviewability bar.
