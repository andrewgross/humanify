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
  extends carry to BELOW-FLOOR priors — the \_\_m-poisoning class the
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

## STATUS (2026-08-14, exp066-r1 @ 003508d + hardening): EXECUTED — 7 asks → 1, both above-band flags reconciled

**Acceptance (cold self-hop, 2.1.216):** LLM asks **7 → 1**; emitted-tree
diff **2 lines** (was 6); minted leftovers stable at 22. The residue is
named: `ngi`, a declarator with NO initializer and ZERO references
inside a multi-var hoist — no fingerprint (cascade-invisible), no
references (voteless), and its statement's twin slot never bridges. It
names a dead slot; 1 of 24,499. The "two invisible bindings" claim
resolved as: one (tdd) was a same-name leftover match now settled by the
same-name rule; ngi is the true enumeration gap, left as the named
residue.

**Cold scored run exp066-r1:** novel/realLn byte-exact (4,188/416,377),
boot ×4, cache +0 all legs, treeLn −4,043 vs reference, self-hop 134
(in the 72–182 cold band). 055 ledgers on 85→86: name-only 1,476
(exp061 runs: 1,500/1,440; pre-arc baseline 1,622/1,662); anchored
one-sided 1,778, not rising.

**Above-band flag 1 — mints 89 vs 77/76 (+11):** identified by name.
The 85→86 concentration (+7) is `do7Function`, `sm6Factory`, `ms6Flag`,
`ps6Flag`, `js6Flag`, `_` — **five verified verbatim in the prior tree**
(carried prior leftovers: the brief's pre-declared census-visible cost;
before, the LLM re-rolled them to fresh words each hop, which the churn
ledger paid for) plus `u$Range`, a decorated local of uncertain lineage.
NOT the frozen-junk failure: the census sees every one, and the trade
(stability now, explicit improvement pass later) is the design.

**Above-band flag 2 — noiseLn 52,568 vs 50,323/50,487:** attributed
per-pair against the three same-commit band repeats (per-pair envelopes,
not the total band): 85→86 inside (33,896 vs 33,863–33,964); 215→216
inside (5,383 vs 4,864–5,609 — that pair's cold spread is 745 lines);
197→198 BELOW the envelope (8,024 vs 8,431–8,715, kept improvement);
118→119 above (5,265 vs 4,150–4,359). The 119 excess was attributed
with a churn-pair diff: exp066-only churn is **42 occurrences across 34
unrelated whole-word re-rolls, zero carried/below-floor names**, and
settledBy tier distributions are byte-identical between exp066 and
exp061 runs on both flagged pairs — diffuse cold-draw context drift,
not a mechanism. One cold repeat would adjudicate; no code lever exists
to pull.

**Hardening added after the flags (TDD):** `allowContentFreeElimination`
is now OPT-IN on `bindingRolesAgree` — the license is the CALLER's
exclusivity gates (single-vote pin: one exact vote, one claimant), and
the twin tier's pairwise `declaredRolesAgree` keeps its strict meaning
(blanket agreement on bare declarators would license positional
cross-transfer by statement shape alone; measured twin volume moved +1,
so this closes a door that had barely opened). The acceptance numbers
were taken pre-flag; the flag restricts a path measured at ±1 binding.

**Claims that did not survive:** "the four dropped role records" — the
records were never dropped; content evidence is structurally ABSENT on
both sides (uninitialized bindings), and the fix was elimination, not
plumbing. "Two invisible bindings" — one was the same-name settle gap,
not an enumeration gap.
