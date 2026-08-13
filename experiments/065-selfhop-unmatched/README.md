# 065 — the self-hop residue: what a perfect matcher still cannot match

> **This is a BRIEF — a hypothesis, including its cautions.**
>
> A self-hop (version N with its own output as prior) is the matcher's
> null test: EVERYTHING has an exact counterpart, so anything that
> reaches the LLM is a matching failure by definition. Today: 7 module
> bindings hit the live LLM on every 2.1.216 self-hop, and the cold
> self-hop diverges 72–182 lines across all observed runs — previously
> MASKED by the LLM cache replaying identical answers (rule 10's
> lesson: the cache made non-idempotence look like determinism).
> On real hops the same residue is ~3,333 fresh asks (3.0% of
> identifiers) producing ~44% of the hidden churn — the unmatched tail
> is expensive far beyond its size.

## Questions (in order)

1. **Who are the 7, exactly?** Full trail + tier-by-tier abstain
   reasons on a fresh cold self-hop; the diag already records this.
   Then the same census for the 3,333 on 85→86: cluster the unmatched
   by WHY (ambiguous twins? decorated-name drift? statements absent
   from the prior wrapper body? eval-taint frozen? vote floors?).
2. **Why does the self-hop wobble land where it lands** — which output
   lines differ between a version and its own re-run, and would
   exp062/063's levers (instance identity, contention adjudication)
   have absorbed them?
3. **What would drive the self-hop to byte-identical?** Rank the
   residue clusters by fix cost; a self-hop that is exactly idempotent
   cold is the end-state acceptance test for the whole matching stack.

## Cautions pinned before measuring

- Rule 10: every verdict here must be COLD; the cache is what hid this
  for months.
- Rule 11: the cold self-hop band is 72–182 lines across five runs —
  single-run deltas inside that band mean nothing; count MECHANISMS
  (which bindings, which trails), not lines.
- Count occurrences, not runs (exp059): 6 clean runs of 10 proved
  nothing.

## Success criterion (fixed now)

A complete unmatched-by-reason census for one self-hop AND one real
hop, two-run stable at the cluster level, with a ranked fix list.
Code changes, if any, ship under their own gates — this experiment's
deliverable is the map.
