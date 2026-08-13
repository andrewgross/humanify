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

## RESULTS (2026-08-13, exp061-lever-r1/r2 artifacts + one cold diagnostic self-hop)

All cluster counts are two-run stable; the A and D censuses are
BYTE-IDENTICAL across the two cold repeats — consistent with exp061's
finding that the churn mechanism is deterministic context, not draws.

### D. The "3,333 we can't nail down" — mostly not a matching failure

| cluster                                   |     r1 |     r2 | reading                                                                      |
| ----------------------------------------- | -----: | -----: | ---------------------------------------------------------------------------- |
| statement hash ABSENT from prior          |  2,796 |  2,795 | genuinely new/edited code — a fresh ask is CORRECT; nothing existed to match |
| ambiguous same-hash family (mostly ≥5×≥5) |    505 |    505 | identical-twin families; any assignment is a guess (zustand-class)           |
| 1:1 hash in both, still asked             | **27** | **27** | the only true matcher misses                                                 |

The unmatched tail is 84% legitimate, 15% structurally ambiguous, 0.8%
bug. "Fix the matcher" is the wrong frame for the first 84% — the churn
they cause is the LLM re-inventing words for genuinely-changed code,
which is a NAMING-stability problem (prompt context), not recall.

### A. Vote-state of the llm-settled (top clusters, per run)

~2,569 function-ask bare + ~254 module-batch bare (78–85% bare overall);
~150 unanimous votes dropped by pin gates (non-exact-source 69,
name-conflict 47, no-prior-role 36); ~98 mixed votes; ~95 no-renamed-row
(post-ask decoration). Minted-ordinal names are ~10% of module-batch
asks.

### B. The ambiguity frontier, corrected

exp064's "~855 ambiguous-join" conflated ZERO in-file candidates with
2+. Split: **no-trail 825/790 (70%)** — the churned line's deciding
binding lives elsewhere (cross-file member refs, emit-time aliases,
decorated names); unique-join 266/255; true in-file ambiguity is only
**86/88 occurrences (7%)**, of which ~35% are same-hash twin sets.

### C. Strict whole-pair ceilings (exp063/064 method)

| lever                                   |   ceiling (ledger ln) | vs 40–60 floor  |
| --------------------------------------- | --------------------: | --------------- |
| resolve ALL in-file ambiguity perfectly | 122 / 124 upper bound | barely 2× floor |
| identity-redistribution healable subset |           **52 / 48** | AT the floor    |

Even perfect in-file identity assignment is worth ~50–120 lines on the
noisy pair. The in-pair lever ladder is exhausted; remaining mass is
derived (importer lines, aliases) and naming-stability for new code.

### Q4 case study: caller sets adjudicate the duplicate-heir contest

Prior: ONE `createGetRoleCredentialsCommand`, every reference inside
`request-exception.js`. Fresh: copy 1 declared and referenced in
`request-exception.js` (caller-set continuity), copy 2 referenced only
from `http-request.js` + `aws-config-validator.js` (new callers).
**The cascade's assignment was RIGHT**; the misleading evidence was the
votes for copy 2 (its callers truthfully testified about the single
prior copy). Caller-set continuity would have gated the HINT, not the
cascade — reversing exp061's suspicion.

### Q2. The 7 self-hop LLM asks — dissected (fresh cold diagnostic run)

| binding            | trail                                                                                                                                | outcome with exp061's hint                                                                                                   |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| xRm, Rwc, cS, EH\_ | exact-match vote held the prior name; module-pin abstained `role-mismatch:no-content-evidence`; vote-suggest carried it into the ask | **all 4 landed EXACTLY on their prior names** — stable                                                                       |
| n0e                | THREE votes for `fs2`; binding-cascade AND module-vote refused `below-floor-prior-name`                                              | the guard refuses the pipeline's OWN prior residue → re-ask every hop (no diff this draw)                                    |
| ngi, tdd           | completely bare — invisible to every cascade tier                                                                                    | fresh roll each hop; `tdd` was a raw minified LEFTOVER in the prior (`var tdd = …`), so its naming is CONVERGENCE, not churn |

**The invariant's coordinate system, reconciled:** this run's self-hop
diff is **6 lines in the emitted src tree** (2 bindings) but **120
lines in the bundle** — exactly the harness's count, byte-reproducing
lever-r1's 120. The 72–182 "violation" band measures bundle
coordinates, where each residual binding is amplified through every
occurrence. Reviewer-facing idempotence is already ~6 lines; and part
of that is the prior's own minted leftovers getting named (improvement,
not wobble).

### Ranked fix list toward a byte-identical cold self-hop

1. **Nothing dominant remains.** With exp061's vote-suggest hint, the
   5 vote-backed asks pin to their prior names; the residue is 2 bare
   bindings + occasional single-param re-rolls (~6 tree lines/run,
   under any floor). ACCEPTANCE TEST for the next arc: a second-order
   self-hop (re-run on THIS run's output) should approach 0 tree lines
   as the leftovers converge.
2. **Below-floor refusal loop (n0e/`fs2`)**: the guard re-asks the LLM
   every hop rather than carrying its own prior residue. Fix shape:
   carry-then-improve (let coverage-sweep own the upgrade), sized ~0–6
   lines/run — floor-bounded, ship only as part of a bigger touch.
3. **Cascade-invisible bindings (ngi/tdd class)**: find why no tier ever
   attempts them (2 of 24,499 module bindings) — likely a declaration
   pattern outside the cascade's walk. Worth one look for completeness,
   not for lines.
4. **Bundle-coordinate amplification**: if the invariant should reflect
   reviewer experience, report BOTH numbers (bundle + tree) in
   run.sh — one-line instrumentation change.

## STATUS (2026-08-13): EXECUTED — the map is drawn; the premise "so many rows we can't nail down" did not survive

The unmatched tail is 84% genuinely-new code (fresh asks are CORRECT),
15% structurally-ambiguous twin families, 0.8% true matcher misses (27
bindings). In-file identity/ambiguity, perfectly resolved, is worth
~50–120 ledger lines (at the floor). The self-hop is already ~6 tree
lines from idempotent with exp061's hint, and its remaining mechanisms
are enumerated above with fix shapes. The next real lever is
NAMING STABILITY FOR GENUINELY-CHANGED CODE (84% of asks): making the
LLM's word choice for an edited function stable across context drift —
a prompting/anchoring problem, not a matching one.
