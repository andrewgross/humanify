# 066 — the zero-ask self-hop: only new code may reach the LLM

> **This is a BRIEF — a hypothesis, including its cautions.**
>
> Read exp065 + its match-census addendum first. Pre-LLM, the self-hop
> matcher is essentially perfect: 0 unresolved functions, 0
> ordinal-resolved, 6 unresolved module bindings (of ~23,600) which
> become the 7 LLM asks. The frontier invariant, set by Andrew: **a
> self-hop sends ZERO things to the LLM** — an ask is only ever earned
> by genuinely new code. Unlike the arc's three sized skips, this
> cannot die by ceiling: it is an invariant (0 asks), not a line count.

## The seven asks decompose into three fixes

1. **Prior-provenance name carry (design centerpiece, Andrew's rule).**
   A name read from OUR OWN prior output was already processed once;
   "looks minified" (`fs2`) is not grounds to refuse the carry — the
   current refusal buys a fresh LLM ask EVERY hop (instability) in
   exchange for hoping the name improves (it rarely does). Split the
   concerns: ALWAYS carry for stability; a separate deterministic
   improvement pass may upgrade a below-floor name when it has
   something strictly better. Zero-minted-leftovers is served by the
   improvement pass, never by refusing the carry.
2. **The dropped role records** (`no-content-evidence` ×4): on a
   self-hop content agreement is satisfiable by construction, so the
   role evidence was dropped, not absent. Find where
   priorBindingRoles loses these four; fix the plumbing.
3. **The two invisible bindings**: present in the naming pool, absent
   from every matcher tier's field of view — an enumeration gap
   (post-snapshot transforms? unwalked scope shape?). One case study,
   then the fix.

Plus one instrument: **per-rung counters in `propagate()`**
(matched-callee / matched-caller / scope-parent / scope-ordinal) — the
census proved the split exists nowhere, and the luck-prone ordinal rung
(282 functions on 85→86) must stay permanently visible. Fourteen
cascades, one counter precedent (`singletonUnguarded`) that paid for
itself; this is the second.

## Recorded for LATER (Andrew: "we can come back to this")

**Process-of-elimination naming for low-evidence bindings**: one vote +
the name has exactly ONE claimant anywhere + the name is dead
everywhere else in the new output ⇒ the assignment is forced; the
role-corroboration gate is redundant under total exclusivity. TWO
preconditions before building: the claim denominator must include NEW
code's potential claims (i36/Pd8: a truthful vote attached to the
wrong heir because a second copy existed), so the duplicate-family
check comes first; and the relaxation stays narrow (exclusivity must
be over the whole tree, not the file).

## Cautions pinned before measuring

- exp035 E precedent: decorated descriptives already carry; this
  extends carry to BELOW-FLOOR priors — the __m-poisoning class the
  refusal existed for. The improvement pass MUST demonstrably fire on
  carried below-floor names in a unit test, or we have frozen junk.
- The self-hop asks are 7; the fix list must not regress REAL-hop
  behavior to get them — every change gates on the full protocol
  (novel/realLn byte-exact, bands) plus the new invariant check.
- Rule 10: all verdicts cold. The acceptance test is a COLD self-hop
  with `LLM calls: 0` in its coverage summary and (second-order) a
  tree diff trending to 0 lines.

## Success criterion (fixed now)

A cold 2.1.216 self-hop reports **zero LLM calls** and its emitted-tree
diff is ≤ the current 6 lines (trending 0); `npm run check` green; one
cold scored run holds novel/realLn byte-exact with all KPIs in-band;
the per-rung propagation counters appear in stats/diag and a guard
test keeps them.
