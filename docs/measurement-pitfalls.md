# Measurement pitfalls — rules that cost retractions

Not tied to any experiment number, because experiment dirs get archived and
these keep applying. Every rule here was learned by publishing a wrong number
first. Each one names the case, so you can check whether your situation rhymes.

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
