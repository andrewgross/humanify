# Exp017 — enclosing-statement cracking + ordinal pairing (2026-07-09)

Branch `exp017-keyless-clone-cracking`. Goal metric: the shared-lineage
diff (`experiments/016-diff-noise-convergence/run-chain.sh`) — the diff
a human reviews between adjacent releases sharing a naming lineage.

## Headline

| lineage-diff metric | exp016 end | + stmt stage | + ordinal pairing |
| ------------------- | ---------- | ------------ | ----------------- |
| noise hunks         | 2,960      | 2,989 (flat) | **2,516 (−15%)**  |
| noise share         | 60.8%      | 60.8%        | **57.1%**         |
| genuine hunks       | 1,909      | 1,925        | 1,890 (stable ✓)  |
| rename occurrences  | 5,893      | 5,773        | **4,499 (−24%)**  |

Campaign (fresh-vs-incr metric → lineage metric): 22,998 → 6,206
(exp014) → 5,788 (exp015) → 3,929 (exp016 A/B) / 2,960 (lineage) →
**2,516** — occurrence lines 47,761 → 4,499 (−91%).

## The two mechanisms

1. **Enclosing-statement cascade stage** (`e311b3b`): structurally
   identical clones carry no internal identity; the statement around
   them often does. New resolver after memberKey hashes the nearest
   statement ancestor (statement-align's normalization; lazy, cached,
   50-line cap). RESOLVER semantics (drifted context falls through);
   match required the hash unique on both bucket sides. Bundle scale:
   **3,226 resolutions** (third-largest cascade stage), fresh pool
   4,382 → 3,942. Lineage noise, however, stayed FLAT — the noisy
   clones were exactly the byte-identical-context ones the uniqueness
   guard refused.
2. **Ordinal pairing for interchangeable clones** (`8676dfa`): when the
   function hash AND enclosing-statement hash are identical and counts
   match on both sides, the members are semantically interchangeable —
   any bijection is correct, only determinism matters. Equal-count
   groups pair by source ordinal (statement-align's equal-count rule);
   unequal counts stay ambiguous. **This is a pinned POLICY CHANGE**:
   it replaces "equal evidence must not crack the bucket at the exact
   layer" — which left those bindings fresh-named every run (recurring
   noise) or order-paired by the close-match fallback with no content
   gate at all. The new gate (identical fn + identical context + equal
   counts) is strictly stronger than that fallback.

Verified kills in the lineage diff: the schema case-flip family
(`RpcRequestSchema→rpcRequestSchema` 19+19+15+14+14), decoration churn
(`appStateVal→appStateVar` 28), React collision ordinals — all gone.
`identityVar→identityVal` 32 → 24 (residual = unequal-count or
drifted-context groups).

Also en route: string literals contribute their LENGTH to the
rename-invariant hash (`S=__STR_5__`), so "alpha" vs "beta" is
distinguishing context — same-length strings are not. The negative
tests account for this.

## What the residual looks like (4,499 occurrences)

Top family is 32 occurrences; 1,945 distinct bindings average 2.3
occurrences each — a long, flat tail:

- `index→i` (32) + `H→builderContext` (14): asymmetric coverage
  (one leg named, one didn't) — 292 occ total.
- Synonym drift on close-match remainders
  (`lastAssistantMessageHash→messageItem`, `queryString→queryText`) —
  the LLM has prior context but picks a different surface; snapping
  can't help without a same-stem unique prior.
- Residual decorations where no unique-stem prior exists
  (`configVal→config`, `childOffsetY→childOffsetYVal`).
- Reroll is at its floor (110 occ).

## Next levers (each now worth ≲ a few hundred occurrences)

1. Per-identifier prior hints in FUNCTION prompts (module prompts
   already render "Prior version name:" per identifier) — attacks the
   synonym-drift tail.
2. Asymmetric autopsy (`index→i`) — one-leg coverage holes.
3. Close-match coverage for the remaining fresh pool (3,942 after this
   round, was 4,382).

## Reproduce

```bash
# lineage measure (~4 min, needs the prior leg at /tmp/exp016-r1/cc-120)
CHAIN_OUT=/tmp/exp017-chain2 bash experiments/016-diff-noise-convergence/run-chain.sh
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/exp017-chain2/runtime-diff.txt 12
```
