# Measurement pitfalls — rules that cost retractions

Not tied to any experiment number, because experiment dirs get archived and
these keep applying. Rules 1–7 were each learned by publishing a wrong number
first; rule 8 is about a number that was never wrong but never looked in the
right place; rule 9 is about meeting one of these numbers later, in the document
that still contains it; rule 10 is about a number measured through an instrument
that had been told not to vary; rule 11 is about a number from an instrument that
was free to vary by more than the thing being measured. Each rule names its case,
so you can check whether your situation rhymes.

## 1. Never trust a match you have not eyeballed

The standing rule of this project, and it has now refuted **seven** hypotheses
across exp040–045 — including two that came from an experiment brief's own
stated premise, and one that fit the arithmetic perfectly.

- **exp042's brief** said `managedAgentsReadme` / `managedAgentsDocsVal` were
  meaningful, stable names where the anchor and the name vote were equally
  credible, and fenced them off. Reading the pairs showed the name had rotated
  between SIBLING DOCUMENTS (the Go README's name went to the Java one), the
  anchor twin differed by 2 lines out of 565, and preferring the anchor was not
  a coin flip at all. exp043 shipped it for −1,807 lines on that hop.
- **exp039's correspondence hypothesis** — that six names changing at once meant
  the renamer followed a different prior statement — survived years of citation
  and died on one look at the substitution list, which was full of cycles
  (`commandExtractor → commandRunner → commandValidator → commandExtractor`).

A brief's caution is a hypothesis, not a constraint. So is your own.

## 2. An explanation from the largest example is about that example

Until the population is measured, a mechanism inferred from the biggest single
case is a hypothesis about that case only.

- **exp045** found `files-api.js` charged 332 reorder lines where git prints 4 —
  an 83× over-charge — and inferred the reorder KPI was systematically inflated.
  Measured across all files whose only change is order: **1.0× / 1.0× / 1.0×**
  on three hops of four. The metric is sound; that one file is pathological.
- **exp044** read the top-N drifted names on 85→86, saw `React123 → React93`
  everywhere, and nearly built a vendor-counter stabiliser. The whole counter
  class is 320 git lines, 4% of naming. The top-N list is not the mass.

## 3. A sizing predicate that "looks right" fails silently

Two classifiers in exp044 produced confident, wrong numbers because the
predicate did not test what its name implied.

- **Alias churn "38% of naming"** → actually **7.2%**. The test asked whether the
  prior name was an alias and the fresh name was an alias, and never that the two
  named the SAME module — so it counted every statement that legitimately calls
  something else.
- **"93% of the 6+ substitution bucket is alias churn"** → the reverse, 98% is
  NOT. Same flaw.

Before believing a decomposition, state in one sentence what the predicate
actually tests, and check that sentence is the claim you are making.

## 4. A syntactic proxy for a semantic property can be biased in a direction you did not predict

**exp045 task A** approximated a load-order barrier as "any top-level
`ExpressionStatement`", reasoned explicitly that this OVER-counts constraint, and
published ~2,800 recoverable lines as a **lower** bound. Running the shipped
model (`bundleLoadOrderFacts`) showed it UNDER-counted — a variable declaration
whose initializer calls something impure is a barrier too — and the real figure
was 1,176. Reasoning about the direction of your approximation's error is not a
substitute for using the real thing when the real thing is one import away.

## 5. A damage ceiling scoped to the directly-affected population under-predicts

**exp044's alias reservation** measured its own damage carefully and correctly:
zero stable bindings destabilised, exactly as its scoping promised. It then cost
**+3,742 git lines** and made its own target metric worse.

The ceiling counted which bindings would be deflected and assumed a deflection is
free because the name was new anyway. True for that binding's own diff line;
false for everything downstream. Module-binding names feed the split's name votes
AND the emission-order alignment, so each deflection perturbed placement and
ordering.

**Any change inside the name allocator has a blast radius larger than the
population it directly touches.** Measure the cascade, not just the target.

## 6. Refusing a name does not remove a collision — it moves it

Same experiment, the structural reason it failed. Blocking a binding from taking
an incumbent import alias sent it to another name, which collided with a
different incumbent alias. On 118→119 it saved `kairosCron`, still lost
`statusIcon`, and CREATED two new displacements. Across four hops the
displacement count was unchanged at six.

Whack-a-mole is the default outcome of any "forbid this name" rule in a system
with a collision ladder. Show why yours is not, before building it.

## 7. Name-keyed metrics cannot see content identity

The leaderboard's `reloc` column (`sameNameMovedFile`) rose by exactly the number
of statements exp042 and exp043 deliberately placed correctly — +14 and +7 — for
the reason those experiments existed: a recycled name is not an identity, so
restoring content to its own file necessarily moves the NAME. `analyze.ts`'s own
docstring says to prefer the order-independent `relocatedStatements`, which was
0 on every hop for both.

Relatedly, `diff-composition`'s `alias` column counts only `require` HEADER
lines (200 across four hops) while every usage site of a re-aliased import is
charged to `naming`. A ranking that reads that column at face value puts the
lever last when it is first.

Both are now encoded as data in `experiments/034-eval-harness/kpis.ts`, where
each KPI carries its direction (`↓` drive to zero, `=` real change that must not
move, `~` a move means nothing on its own) and its caveat prints under the table.

## 8. A metric can only be wrong about what it looks at

Rules 1-7 are about a number that measures the wrong thing. This one is about a
number that measures the right thing over the wrong SCOPE, which no amount of
care inside the measurement can catch.

**exp033-045 scored `src/` only.** `run.sh` passed `("$OUT/src" "$PRIOR_SRC")`
to the scorer and nothing else. Five experiments drove `src/` noise to a measured
floor of ~3,700 reducible lines in a 154,668-line diff and wrote that up as the
end of the arc — while `vendor/`, in the same emitted tree and diffed by the same
reviewer, churned **36,201 lines, 2.4x all measured `src` noise**. exp046 found
it was 90.8% noise and removed 73.4% of it with two levers, neither of which
needed a new idea; they needed someone to look at the other directory.

Every KPI in the harness was correct. Every gate passed honestly. The floor was
real, and it was a floor of the wrong room.

**Before believing a floor, enumerate what the harness does not look at.** For
each surface, say whether a human reviews it — a large number is a SIZE, not a
finding (`.humanify/humanified.js` churns 1.49M lines and nobody diffs it). That
audit is cheap and it belongs at the START of an arc, not five experiments in.

A related instance of the same shape, from the same experiment: **a hash can only
be wrong about what it serializes.** `structuralHash` compares two functions
faithfully, but it serializes strings as their LENGTH and numbers as their
order-of-magnitude bucket, so it cannot see six of twelve semantic differences.
exp046's brief proposed gating vendor body reuse on it; that would have shipped
libraries carrying the prior release's endpoints. Ask what a comparison ERASES
before trusting a match it certifies, and probe with same-length inputs — a probe
using `'alpha'` vs `'beta'` passes for the wrong reason.

## 9. The wrong number does not disappear when it is corrected — it stays in the older document

Rules 1–8 are about producing, or failing to produce, a wrong number. This one is
about meeting one later, which is now the more likely way to be misled: **every
retraction above lives in a newer file than the claim it retracts.**

`experiments/044-naming-correspondence/README.md` still contains the sentence
that sized alias churn at 38%. The correction to 7.2% is in that directory's
RESULTS file. Someone opening the README to find the next lever reads the brief,
not the result — briefs are shorter and sound more like a plan.

The same shape applies to any document written before the work it describes:

- **A brief's caution is a hypothesis too.** exp042's fenced off a case as a coin
  flip; reading the pairs refuted it and exp043 shipped it for −1,807 lines. The
  parts of a brief telling you what NOT to try are exactly as unverified as the
  parts telling you what to try.
- **Titles expire.** "The top remaining lever" and "the best-shaped one left"
  were true on the day they were written. Three experiments later the axis they
  name is the smallest one.
- **Method ages, not just numbers.** Anything from before `034` was not gated on
  four real version pairs with a boot check and a self-hop invariant, and several
  were scored on toy fixtures the matcher cannot be defeated by. Read those for
  the mechanism, not the magnitude.

The mitigation is cheap and belongs to whoever finishes the work: **a STATUS
block at the top of the README**, stating the outcome and naming which of the
document's own claims did not survive. Every README in the active arc has one;
`experiments/README.md` explains the convention. A directory without one has not
been audited.

## 10. A determinism aid becomes a lie the moment it is left on for the verdict

`--llm-cache` exists for a good reason: identical runs drifted +/-2.7k lines
ACROSS sessions because the local vLLM server's serving state is the dominant
nondeterminism, and caching by request content makes a repeated prompt
reproducible. It was built for testing paths that do NOT depend on LLM output.

**exp047's first gate ran entirely through it.** All 24,079 cache entries
pre-dated the run and not one new entry was written, so **not a single prompt
reached the model** across eight pipeline runs. Every reported "LLM call" was a
22ms disk read. The gate passed, every KPI looked byte-identical to control, and
the `src/` half of the result meant nothing — two legs replaying the same answers
must agree.

Re-running the same experiment COLD, with a cold control, changed three
conclusions that had looked settled:

- **`vendorReal`, a `=` "must not move" guard, is not draw-stable.** It reads
  3,364 cached and 3,576 cold for IDENTICAL code, because it counts humanify's
  own vendor-filename rotation as dependency change. One git module named
  `simple-git` in one run and `git-reset` in the next charges a library added
  plus a library removed.
- **The self-hop invariant is unreachable cold, for the control too** (78 lines).
  "216 is a perfect fixed point, 0 diff lines" was cache luck.
- **The committed `src/` baselines are themselves cache-pinned artifacts**, so a
  cold run establishes a new reference rather than checking against them.

The deterministic half was fine throughout: manifest ordering never consults the
LLM, and 85->86 reached exactly 0 under both conditions.

**Use the cache for iteration; never for a verdict.** A gate has to reproduce
what a real user sees. And when a candidate goes cold, the CONTROL has to go cold
with it — a cold candidate against a cached control measures the cache, not the
change. `run.sh` now defaults to no cache and prints which mode it is in;
`EVAL_LLM_CACHE=<dir>` opts back in and announces itself as not gate-valid.

Corollary, and the reason this rule is not just about one flag: **anything that
suppresses variance also suppresses the evidence that the variance matters.**
Before trusting an invariant, ask what would have to vary for it to fail, and
check that the thing is actually free to vary in your setup.

## 11. A gate cannot resolve an effect smaller than its own noise floor — and it will not tell you so

Rule 10 says take the aid off. This is what you find underneath: once the
variance is free to move, **the harness has a resolution limit, and every KPI
still prints a confident number below it.** A gate run at that scale does not
return "inconclusive". It returns a verdict, and the verdict is noise.

**exp048** A/B'd the family-permute pass cold, four pairs, both legs. Read
straight off the leaderboard it looked decisive:

- 118→119, the CALM canary, "won" **−2,864 noise lines** — the largest single-hop
  improvement anywhere in the arc.
- 197→198 "regressed" **+232 noise / +3,610 real**.

**Both hops had shipped ZERO renames.** The pass was provably inert on the one
that regressed, and on the one that won it applied 14 renames whose names occupy
**247 lines** — while only **252 of the 37,592** lines separating the two legs
mentioned any name it touched. The `src/` per-hop draw band is **±2,800**, eight
times the ±350 exp047 had estimated; the pass's real effect is **−335 git lines**
on its best hop. It was two orders of magnitude below the floor, and the gate
happily assigned it a sign.

Two things made this visible, and neither is a KPI:

1. **The change logged what it actually did.** A pass that records every rename
   it ships turns "did the metric move?" into "did the code do anything HERE?".
   A hop with an empty trail cannot have had its KPIs moved by the change,
   however they read. Instrument the mechanism, not just the outcome.
2. **A ceiling computed from the mechanism, before the run.** Summing the lines
   the touched names occupy predicted ≤220 on 215→216; the exact answer was 335
   in total churn, 96 on the noise column. A cheap mechanism-bound is what tells
   you whether a delta is even the right ORDER OF MAGNITUDE to be yours.

And when the effect really is below the floor, **remove the variance instead of
averaging over it.** Rule 10 forbids the cache for a verdict about
LLM-dependent behaviour; it explicitly allows it for a deterministic surface. If
the thing under test is deterministic and sits DOWNSTREAM of every prompt, pin
the draws and the delta becomes exact — but prove the pinning worked (exp048's
second leg wrote **zero** cache entries, which is the only reason its −335 means
anything) and state what the pinned run cannot see (there, the multi-hop
feedback where output becomes the next release's prior).

**Before a gate decides anything, measure what it reads for two runs that should
agree.** That number is the smallest effect it can see. Anything under it needs a
different instrument, not a bigger sample.
