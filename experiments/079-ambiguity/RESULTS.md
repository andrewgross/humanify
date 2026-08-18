# 079 RESULTS — the address rung, measured

> **STATUS 2026-08-18.** Task 1 SHIPPED (on main). Containment change BUILT,
> WALKED, and **NOT MERGED** — see the verdict below. Everything else here is
> measurement.

## Gate table

| gate                            | result                                    | what it proves                         |
| ------------------------------- | ----------------------------------------- | -------------------------------------- |
| `npm run check`                 | GREEN 8/8 at every commit                 | nothing broken                         |
| `matcher-preflight.sh`          | mitt / nanoid / preact / zustand ALL PASS | ground truth on real packages unmoved  |
| cold candidate walk (`296de41`) | 4/4 exit 0                                | the pipeline runs                      |
| cold PAIRED control (`f9dd4d1`) | 4/4 exit 0                                | the comparison is like-for-like        |
| `novel` / `realLines`           | byte-identical both hops, both walks      | **no real change lost** (band is ZERO) |

`/work/walk-main` is NOT a valid control for this: 6,000 memberKey vs 6,055,
so it sits on the far side of Task 1's behaviour change. It was used as one in
an earlier reading of these numbers, and that reading was wrong.

## What the rung actually is

Measured, not assumed. The plan said anonymous functions have no address; they
have one, and these are its properties.

| fact                                                   | number                      |
| ------------------------------------------------------ | --------------------------- |
| rung's rank among all resolvers                        | **#2**, ~13,000 matches/hop |
| functions reaching it (2.1.215->216)                   | 18,844                      |
| resolved                                               | 12,997                      |
| abstained                                              | 5,846                       |
| ...of which NOT a loss (function is its own statement) | 2,396                       |
| ...count mismatch                                      | 2,032                       |
| ...over the 50-line cap                                | 1,340                       |
| ...statement hash gone                                 | 78                          |

### Three readings that were wrong, and what corrected each

1. **"Count mismatch is near-dead; statement-hash-gone dominates."** From five
   micro-cases. The population says the reverse — 2,032 vs 78. Toy examples
   did not resemble the bundle at all.

2. **"The 50-line cap is a 2x bias against the population the rung serves."**
   True across the whole bundle (42% of crowded functions), false at the gate:
   only **7.1%** of arrivals are over the cap. Most crowded functions in big
   statements never reach the rung — they resolve earlier. Measuring the bundle
   told me about the bundle, not about the gate (pitfalls rule 8).

3. **"The spanning positional pairing is the +50,606 mechanism failing."** The
   mechanism identification was right; the alarm was not. Where checkable it
   agrees 3,504 to 93 — **97.4%**. Refusing spanning groups would have
   surrendered ~6,000 good matches to fix 93 bad ones.

### The structural finding

The rung groups by enclosing-statement HASH, which is rename-invariant, so
identical statements anywhere in the bundle pool together.

| holder groups                        | number             |
| ------------------------------------ | ------------------ |
| functions in multi-member groups     | 12,744             |
| in groups spanning >1 statement node | **8,158 (64.0%)**  |
| largest group                        | **657 statements** |

Consequences: a count mismatch is the count of PATTERN INSTANCES changing
bundle-wide, never a local sibling edit (0 of 2,032 were local); and the
equal-count path pairs across unrelated statements by global source position.

## The containment change — walked, NOT merged

Narrow a spanning pool to the matched enclosing function before pairing; veto
pairs whose parents matched to different things.

### What it did to matching

| counter                | before |  after |
| ---------------------- | -----: | -----: |
| enclosingStmt resolved | 12,997 | 13,033 |
| count mismatch         |  2,032 |  1,982 |
| parent crossings       |     93 |      0 |
| injectivity demoted    |     16 |     30 |

Crossings reached 0 with **zero vetoes fired** — narrowing prevents them by
construction. The veto's zero is therefore not a precision result, and is
documented as such in the type; crossings among the 2,475 pairs with an
UNMATCHED parent stay invisible either way.

### What it did to the diff

| hop  | metric            | control | candidate |  delta |
| ---- | ----------------- | ------: | --------: | -----: |
| calm | churnLines        |   1,605 |     1,587 |    -18 |
| calm | noise             |      60 |        56 |     -4 |
| calm | namingNoiseLines  |   1,660 |     2,922 | +1,262 |
| busy | churnLines        |  28,355 |    29,066 |   +711 |
| busy | real              |  22,847 |    23,542 |   +695 |
| busy | namingNoiseLines  |   3,312 |     3,656 |   +344 |
| busy | sameNameMovedFile |     375 |       370 |     -5 |
| both | novel / realLines |   exact |     exact |      0 |

**Is +711 real or noise?** Sized rather than guessed: ~130 functions changed
match (36 net + 93 crossings prevented). A name recurs several times and each
changed line costs a delete plus an add, so the mechanism reaches +711
comfortably. Treat it as REAL.

The direction is also explicable: preventing a wrong pairing stops that
function inheriting a name, so it draws a fresh one, which is churn. An honest
fresh name costs more diff lines than a stable wrong one.

### Verdict: DO NOT MERGE this version

Not mainly for the 711 — that is an arguable correctness-for-lines trade — but
for two defects it does not fix:

1. **The pairing frame is order-dependent.** Narrowing consults the matches
   map WHILE it fills, so two functions in one container can compute different
   bijections. The replaced code's own comment required the frame be
   bucket-level "so every old member computes the SAME bijection"; the +14
   injectivity demotions are that invariant breaking.
2. **It reaches 60% of the target.** Only the 3,647 pairs whose parent is
   already matched. The 2,475 with an unmatched parent — the population that
   motivated containment — are untouched.

Both dissolve if containment runs in **propagation**, which executes after the
cascade with a COMPLETE matches map and already iterates to a fixed point.
That is the next build, and its effect would be large enough for the walk to
resolve — which this one's, at 36 net matches, was never going to be.

## Still open

- containment in propagation (above)
- structural ADDRESS inside a matched container: property NAMES over indices,
  so inserting a sibling stops shifting everything after it
- the 50-line cap: only 1,340 arrivals, so a small and cheap experiment, but
  measure before touching (it looked 6x bigger from the wrong scope)
