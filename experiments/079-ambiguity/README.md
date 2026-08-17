# 079 — read the evidence, narrow the field, then assign

> **PLAN, for review. Nothing built.** Written 2026-08-17 out of the exp078
> work and the BinDiff research
> ([`../078-durable-names/bindiff-research.md`](../078-durable-names/bindiff-research.md)).
>
> Three ideas that look like one problem and are not:
>
> 1. some "ambiguity" is **evidence we are not reading** — fix by reading it;
> 2. some is **real ambiguity that a weaker rule could settle** — fix by only
>    ever running weak rules inside an already-narrowed set;
> 3. some is **provable interchangeability**, where no correct answer exists —
>    fix by choosing the assignment that churns least.
>
> They need different licences and different benchmarks. Conflating them is
> how a smaller diff gets bought with a wrong name.

## The benchmarks, up front

Everything below is scored on instruments that already exist. No task is
listed without one.

| instrument                                                                  | what it scores                                           | run                                                     |
| --------------------------------------------------------------------------- | -------------------------------------------------------- | ------------------------------------------------------- |
| **fixture validate** — real npm packages with GROUND TRUTH                  | function matching precision/recall per version pair      | `npx tsx test/e2e/harness/index.ts validate <fixture>`  |
| **the walk** — 4 consecutive releases, cold start, each inheriting the last | module identity, diff lines, files added/removed/renamed | `experiments/076-statement-placement/walk.sh <workdir>` |
| **offline attribution**                                                     | why each leftover failed, and what class it is in        | `experiments/078-durable-names/task0-attribute.ts`      |
| **self-hop**                                                                | a version against itself must produce ZERO               | inside `walk.sh` / `run.sh`                             |
| `npm run check`                                                             | 8 stages, every commit                                   | `npm run check`                                         |

**Current readings, so every claim below has a before:**

```
fixture zustand   v4.4.0 -> v4.5.0   71%   6 matched, 1 ambiguous, 2 unmatched
                  v4.5.0 -> v5.0.0   83%   6 matched, 1 ambiguous, 3 unmatched
                  resolved by: structuralHash 6/5, memberKey 0/1, all else 0
fixtures mitt / nanoid / preact      pass

walk, busy release (2.1.215 -> 2.1.216)
                  diff lines         27,646   (shipped layout: 23,323 = the bar)
                  files +/-/renamed  35 / 5 / 3
                  leftovers          41 modules
                    18 genuinely new — correct, leave alone
                     9 unique best on edges, mutual-best refused
                    13 tied on edges
                     1 no edges, name twin exists
                    19 of the 41 have NO matched neighbour at all
```

## Task 1 — read the name that is already there (zustand)

**The finding.** `getState` and `getInitialState` minify to
`A=()=>d` and `()=>D`. Once identifiers are masked — which they must be —
both are "a function of no arguments returning one variable". Nothing else
is in there: no calls, no literals, no branches.

But the distinguishing evidence IS present, in the source:

```js
B = { setState: z, getState: A, getInitialState: () => D };
```

`getInitialState` is written directly as the value of a property with that
name, and we match it — that is the `memberKey 1` in the second pair.
`getState` is assigned to a variable first (`A = () => d`) and only then used
as a property (`getState: A`). **One hop of indirection and we lose it.**

**Build.** When a function is assigned to a variable and that variable is
used as a property value, the property name is the function's member key.
One reference-follow.

**Benchmark.** `validate zustand` must go 71% → 100% on the first pair and
83% → 100% on the second, with `memberKey` rising and `unmatched` falling.
mitt / nanoid / preact must not move. This is ground truth, not a proxy —
the fixture knows the right answer.

**Refutation.** If precision drops on any fixture, the reference-follow is
pairing things that merely share a property name. Stop.

**This is NOT an ambiguity fix and must not be counted as one.** Nothing was
ambiguous; we were not reading the evidence.

## Task 2 — narrow before applying a weak rule (drill-down)

**The idea, from BinDiff** (`match/call_graph.h:172-190`): when a key ties
with n>1 on both sides, do not abstain — recurse into the **next** rule
restricted to that tied bucket.

The reframe that makes it valuable: _trustworthiness is a property of the
SITUATION, not of the signal._ Statement count is worthless across 4,850
modules and perfectly good across the two candidates a strong signal already
isolated. Several signals we have dismissed as too weak may be fine, provided
they only ever run last and inside a narrowed set.

**Build.** Where a tier currently abstains on a tie, hand the tied bucket to
the next tier instead. Applies to both matchers; the module matcher is where
the 13 stuck leftovers are.

**Benchmark.** Leftover census (`task0-attribute.ts`): the 13 tied must fall.
Then the walk: diff lines toward 23,323. Fixtures must not regress.

**Refutation.** exp078's precision probe: of the pairs a new tier makes,
what share agree with the independently-derived filename? A tier whose
agreement is at chance is inventing matches.

**Known limit, stated now:** this will NOT fix zustand. Every rule after the
structural one also comes back empty there — there is nothing to narrow
_with_. If a drill-down implementation appears to fix zustand, it is matching
on something it should not be.

## Task 3 — when nothing can distinguish them, choose the quiet diff

Andrew, 2026-08-17: _"in the cases of the ambiguity, we should be able to use
the information about what would minimize the diff to break the ambiguity."_

The function matcher already states the principle:

> _"Members that tie under every computable signal are provably
> interchangeable — any consistent assignment is as correct as any other, so
> the only quality axis left is cross-version stability."_

Three gaps between that and this:

**(a) It does not exist for modules at all.** The fossil matcher abstains on
ties. That is the 13.

**(b) It optimises a PROXY.** Today: maximise agreement with already-matched
neighbours (callers/callees weight 2, bundle-order neighbours weight 1).
That stands in for "which assignment churns less". Use the real objective —
count the lines each assignment produces.

**(c) It decides one pool at a time.** Greedy per-pool does not minimise the
total. At tens of candidates, min-cost bipartite assignment is trivial, and
greediness is exactly the criticism the BinDiff research surfaced.

### The licence, and its boundary

Legitimate **only for certified-identical candidates**, where there is no
fact of the matter about which is which — then minimising churn is not a
guess, it is the only decision left.

**Illegitimate for candidates that are merely indistinguishable TO US.**
Then the smaller diff buys a wrong name on wrong code and hides that it did.
An honest churn beats a quiet lie.

zustand sits exactly on that boundary and is the worked counter-example:
`()=>d` returns current state, `()=>D` returns initial state. Identical in
shape, different in meaning. Breaking that tie by diff size would have a fair
chance of labelling the current-state function `getInitialState` — smaller
diff, wrong code, nothing flagging it. **Which is why Task 1 exists and why
Task 3 must never be allowed to reach that case.**

**Build.** Extend certification to modules (equal counts both sides, every
candidate unmatched, one shared evidence key, no other signal separating
them), then assign the whole certified set at once by min-cost matching where
cost = diff lines the assignment would produce.

**Benchmark.**

- walk: diff lines toward 23,323, files renamed toward 0.
- **self-hop must stay ZERO.** A version against itself must produce the
  identity assignment. This is free and it already caught one earlier attempt
  at this, which paired by source order and failed.
- fixtures must not move.

**The guard that makes it safe.** The objective is _minimise NOISE lines_,
never _minimise lines_. Minimising total diff could hide real change.
`novel`/`realLn` have a MEASURED band of exactly zero, so any movement is
detectable — and any movement means stop.

**Refutation.** Self-hop non-zero, or any movement in `novel`/`realLn`.

## Task 1b — "connect the graph": measured, and SMALLER than it looks

Andrew, 2026-08-17, after task 1 landed: _"I wonder what other things like
following the var name calls for functions assigned to them are out there, I
bet there's more places we can 'connect the graph' so to speak."_

The instinct found a real disconnection. The sizing then says it is not the
lever, and it corrects a number this file previously stated.

**The disconnection is real and large.** The matcher's import graph comes
from the bundle's LEADING INIT CALLS — the lazily-forced dependencies. The
files' actual `require` edges are a strict superset:

|                                         |              edges |
| --------------------------------------- | -----------------: |
| what the matcher sees (init calls)      |             25,627 |
| what the files actually have (requires) |             36,059 |
| **invisible to the matcher**            | **10,432 — 28.9%** |

And starkest: **793 modules have no outgoing edge in the matcher's graph
when only ONE genuinely imports nothing.** To the matcher, 792 modules look
like isolated leaves and are not.

**But it barely reaches the modules we fail to match:**

| of the 41 unmatched                              |       |
| ------------------------------------------------ | ----: |
| have a matched neighbour in the graph we use     |    39 |
| have one in the REAL require graph               |    41 |
| **gained a neighbour they appeared not to have** | **2** |

**CORRECTION to this file's own earlier claim.** It said "19 of the 41
leftovers have NO matched neighbour". That was wrong as worded: 19 have no
edge _AGREEMENT_ — no edge whose counterpart on the other side also exists —
which is a different and much stronger condition than having a matched
neighbour. 39 of 41 have a neighbour. The ceiling argument built on the
wrong number stands only for agreement, not for connectivity.

**So the honest verdict:** the graph is genuinely missing 29% of its edges,
and that is worth fixing for its own sake — an enclosure with real
dependencies should not look like a leaf. But it is NOT the lever for the
41, whose connectivity is already 39/41. Its plausible value is in edge
AGREEMENT strength and tie-breaking (more edges, more chances to corroborate
or separate), which is **unmeasured** and must be measured before building.

**Ordering caveat.** Requires are emitted AFTER assignment, so they cannot
simply be read at match time. The underlying fact — which module declares
each binding another module references — is available then, and the split
already computes it to emit the requires. So this is a real piece of work,
not a lookup.

### What else might be disconnected — census, not guesswork

Task 1 found its hop by reading one failing case. That does not scale. Before
guessing at more, enumerate them:

- functions reaching a name via export aliases, destructuring, or
  `obj.x = f` through a variable;
- call edges through a variable holding a method (`const g = o.m; g()`);
- call edges through destructured imports;
- bindings aliased to a named binding (`const a = b`).

Build a detector that reports, per pattern, how many sites exist and how many
currently resolve — the same shape as the switch and clone censuses. A
pattern with 3 sites is not worth code however elegant the hop.

## Order, and why

1. **Task 1** — smallest, ground-truth benchmark, no ambiguity semantics at
   all. Do it first because it removes a case from the pile rather than
   deciding it.
2. **Task 2** — additive, and its refutation probe already exists.
3. **Task 3** — last, because it is the only one that can trade truth for
   appearance, and it should face the smallest possible residue.

## What would refute the whole plan

- The walk does not move toward 23,323 after all three. The remaining cost is
  then not identity at all, and the model of where the lines go is wrong.
- 19 of the 41 leftovers have **no matched neighbour** (measured). Anything
  that assumes a neighbour exists has a ceiling of ~22, not 41. If Tasks 2
  and 3 recover far fewer than that, the neighbourhood assumption is the
  problem, not the rules.
- Any of it moves `novel`/`realLn`. Band zero; any movement is real change
  lost, which outranks every line-count win.

## Sizing caution carried forward

QBinDiff measured anchoring at +9.8–19.8% f1 — from _no_ anchors to _some_.
We are at 97.6% matched. Total headroom is **2.4%**, and the published gains
do not transfer. Judge these tasks on the walk and the fixtures, not on any
number borrowed from that literature.
