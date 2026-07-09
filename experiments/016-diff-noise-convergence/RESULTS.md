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

## Round 2 — the metric that matches the goal, and prior-name snapping

**Decoration-flip autopsy** (`error→errorVal`): v119 fresh named it
`error` in round 1; v120 incremental named it `errorVal` in round 2 —
a RETRY after the suggestion `error` collided with a prior name already
transferred into the same function. The fresh leg never has that
collision landscape. Conclusion: much of the fresh-vs-incremental A/B's
noise is FIRST-CONTACT — one-time choices the production chain
inherits, not re-makes.

**Shared-lineage protocol** (`run-chain.sh`): the goal is reviewing
SUBSEQUENT-release diffs, where every release is humanified
incrementally on the previous one. v118 was never binary-decompiled,
so the lineage pair closes the loop: re-humanify v119 with the round-1
v120 output as prior; diff the two lineage-sharing legs.

| metric               | fresh-vs-incr (r1) | shared lineage |
| -------------------- | ------------------ | -------------- |
| noise hunks          | 3,929              | **2,995**      |
| noise share of hunks | 67.2%              | 60.3%          |
| genuine-change hunks | 1,919              | 1,973          |
| rename occurrences   | 7,757              | 5,893          |
| lineage-leg cost     | —                  | 4m08s total    |

Chain-diff composition: transfer-gap 5,504 (93.4%), asymmetric 253,
reroll 136 — the mechanical buckets are at their floors; what recurs
is LLM naming instability on bindings that fail to transfer every run
(close-match remainders + the ambiguous-bucket fresh pool). Same-stem
decoration/case/ordinal flips: 704 occ / 220 pairs (11.9%), the
entire top of the list (`identityVar→identityVal` 34,
`appStateVal→appStateVar` 34, `RpcRequestSchema→rpcRequestSchema` 19).

**Fix: prior-name snapping (`80492a5`)** — suggestions snap to the
unique same-stem prior name before validation (function path:
`fn.priorVersionNames` stem index; module path: per-identifier
`suggestedName`). Ambiguous stems never snap.

**Chain re-measure with snapping** (`/tmp/exp016-chain2/`): noise
2,995 → 2,960 hunks; same-stem flips 704 → 677. Snapping fired only
where prior context exists — the autopsy of the surviving flips
(`identityItem→identityVal` 32) shows they are ROUND-2 RETRIES on
`q => q` identity arrows and similar structurally-identical clones:
fresh-pool functions in m:n ambiguous hash buckets with NO prior
context at all. The decoration churn is a SYMPTOM; the reservoir is
the disease.

## Remaining levers after snapping — with honest sizing

**Parent-key probe** (`probe-parent-keys.ts`): 16,725 members in 2,206
multi-member buckets; 3,507 unique under (hash, parentKey). BUT the
existing memberKey stage already resolves almost exactly that
population (memberKeyResolved 3,186 — extractMemberKey covers
prop/method/assign keys). The genuinely NEW evidence (functions passed
as arguments to member calls, `arg:transform@0`) is only **287
members** — not worth a fingerprint change alone.

So the reservoir's still-ambiguous ~4,400 are KEYLESS clones (identity
arrows in argument/array positions, bare declarators). Cracking them
needs a different evidence class:

1. **Enclosing-statement-hash pairing — SIZED, GO**
   (`probe-enclosing-statement.ts`): 4,207 of 16,725 bucket members
   (25.2%) are unique under (bucketHash, enclosingStmtHash), keyless
   members included — every member has a hashable enclosing statement.
   This is statement-align's normalization applied ACROSS functions:
   add the enclosing-statement hash as a fingerprint field and a
   cascade stage after memberKey (strict-contradiction semantics), with
   cross-version uniqueness required on both sides. The exact increment
   over memberKey needs the combined probe (run both keys together),
   but even heavy overlap leaves this the largest remaining lever.
2. Synonym drift on close-match remainders where no same-stem prior
   exists (`lastAssistantMessageHash→messageItem`) — per-identifier
   prior hints in FUNCTION prompts (the module prompt already renders
   per-identifier "Prior version name:" lines).
3. Asymmetric residue (253) — ambiguous-bucket wrong-twin transfers
   keeping minified tokens stable on one side only.

The goal bar ("review like a real git diff") needs noise well under
genuine (~1,900): from 2,960, the keyless-clone reservoir is the only
family big enough to close the gap.
