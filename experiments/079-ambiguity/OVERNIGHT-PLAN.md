# 079 overnight — plan of record

> Written 2026-08-18 before starting, so the plan can be judged against what
> actually happened rather than rewritten to match it.

## Standing decisions (Andrew, before leaving)

1. **Correctness beats diff size, and the cost gets recorded.** A change that
   provably prevents a wrong name transfer ships even if lines go up. Every
   such trade is stated in the results table, never absorbed silently.
2. **Merge and push what validates.** Gate green + the right instrument
   passing = merge to main and push.
3. **Measure the noise floor first.** Two same-commit cold walks before any
   experiment, because two deltas were misread today for want of one.

## Why the band comes first

Today's readings went wrong twice in the same way:

- `+513` was called "36x the band" using `treeLn=14`, a band belonging to the
  eval harness, not the walk.
- `/work/walk-main` was used as a control when it sits on the far side of Task
  1's behaviour change (6,000 memberKey vs 6,055).

Neither error was in the arithmetic; both were in believing a comparison that
had not been established. A measured band at the CURRENT main commit fixes
both at once and makes every later verdict readable — and the two band runs
double as the control for the experiments, so the cost is paid once.

**A walk is ~36 minutes and the box has 251 GB / 64 cores, so walks run two at
a time.**

## Instrument per change type

| change type                  | instrument             | pass condition                             |
| ---------------------------- | ---------------------- | ------------------------------------------ |
| counters, refactors, types   | `neutrality.sh` WARM   | 0 files, 0 lines, **baseline +0 writes**   |
| anything touching matching   | cold walk vs the band  | delta outside the band, in the right sign  |
| any change at all            | `npm run check`        | 8/8                                        |
| any change touching matching | `matcher-preflight.sh` | outcome set unchanged (4 fixtures)         |
| every change                 | `novel` / `realLines`  | **exact** — band is zero, any move refutes |

`novel`/`realLines` moving is a hard stop regardless of every other number: it
means real source change was lost, which no line-count win can buy back.

## The experiments, in order

### Phase 1 — noise band at main (`76c9e05`)

Two cold walks, same commit, same protocol. Yields the per-metric spread for
`churnLines`, `noise`, `namingNoiseLines`, `sameNameMovedFile` on both the calm
and busy hop. **Also serves as the control set for Phases 2-4**, so no
experiment needs its own paired control run.

### Phase 2 — containment in PROPAGATION (the main event)

Today's containment fix narrowed a spanning holder pool to the matched
enclosing function. It worked, and was reverted for two reasons that both come
from running inside the cascade:

- the matches map fills WHILE the cascade runs, so two functions in one
  container can compute different bijections — breaking the invariant the
  original code's comment demanded;
- it reached only the 3,647 pairs whose parent happened to be matched already,
  leaving the 2,475 that motivated it.

Propagation runs AFTER the cascade with a complete map and already iterates to
a fixed point. Both problems dissolve there. Expected reach: all 6,122 spanning
pairs, and 2,032 count mismatches that currently abstain.

**Refuted by:** `novel`/`realLines` moving; `falseMatchFound` rising in the
fingerprint snapshots; any fixture regressing.

### Phase 3 — structural ADDRESS inside a matched container

Position inside a statement is currently source ordinal, so inserting one
sibling shifts every function after it (the delta problem, SSTIC 2005 4.5.6).
Replace with a path from statement to function that uses object property NAMES
where they exist and indices only where they do not.

Only legitimate INSIDE an already-matched container, which is what Phase 2
builds. Hence the ordering.

**Refuted by:** same as Phase 2, plus — if it does not beat plain ordinal
inside a container, the delta problem is not what is costing us and the idea is
dead.

### Phase 4 — the 50-line cap, if time

1,340 arrivals fall out to it. Cheap to test (a constant), but it looked SIX
TIMES bigger from the wrong scope (42% of the bundle's crowded functions vs
7.1% of arrivals), so it is last and gets measured, not assumed.

## What gets written down

Whatever happens, `RESULTS.md` gets a gate table and a results table per
experiment, including the ones that fail. A negative result with its
refutation recorded is the cheapest thing in this repo; an unrecorded one gets
rediscovered.
