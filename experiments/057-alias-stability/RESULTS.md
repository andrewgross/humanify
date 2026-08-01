# 057 — RESULTS: alias stability, measured. NO CODE SHIPPED, and that is the result.

> ## STATUS: CLOSED at task 0/1. Both sub-causes measured git-capped, both at or under the pre-registered threshold, both exactly ZERO on every calm walk hop.
>
> The brief's headline — **~7,360 git lines of require-alias churn, ~37% of all
> name-only churn** — does not survive. The require-alias part is **1,924 git
> lines over four hops, 26% of that figure.** The other 74% is two things that
> are not alias churn: a local variable that happens to precede a dot (1,798 ln),
> and unrelated lines paired by matching masked shape (3,746 ln).
>
> Both prior measurements this brief was written to correct (051's 300 and ≤256)
> were indeed too small. Correcting them did not change the verdict.
>
> Read this file, not the brief. Claims of the brief's that did not survive are
> listed at the bottom (rule 9).

## TOTAL, first

**Git-capped ceilings, per sub-cause, on two independent four-hop measurements.**
Lines CREATED are reported separately and never netted against lines removed.

| sub-cause       | gate 85→86 | 118→119 | 197→198 | 215→216 | **gate 4 hops** |
| --------------- | ---------: | ------: | ------: | ------: | --------------: |
| MOVED-DECL      |         12 |       0 |      56 |     962 |       **1,030** |
| — lines created |         10 |       0 |       4 |       0 |              14 |
| WIDENING        |         46 |     770 |      66 |      12 |         **894** |
| — lines created |          0 |       0 |       0 |       0 |               0 |

| sub-cause  | walk 212→213 | 213→214 (calm) | 214→215 (calm) | 215→216 | **walk 4 hops** |
| ---------- | -----------: | -------------: | -------------: | ------: | --------------: |
| MOVED-DECL |          330 |          **0** |          **0** |     946 |       **1,276** |
| WIDENING   |          680 |          **0** |          **0** |       0 |         **680** |

The decision rule, fixed in the brief before the measurement: **a sub-cause under
~1,000 git-capped lines across four hops is not worth a pipeline change.**

- **WIDENING: 894 (gate) / 680 (walk). Under, on both. Declined.**
- **MOVED-DECL: 1,030 (gate) / 1,276 (walk). Nominally 3% / 28% over — and
  declined anyway,** for three reasons stated below, the first of which the brief
  predicted: the fix is not in aliasing.

### Why MOVED-DECL is declined despite clearing the threshold

1. **It is not an aliasing defect.** The alias followed its declaration
   correctly, every time. The churn exists because a group of module-level
   declarations changed split file. That is `stable-split.ts`'s placement
   cascade — an axis exp041/042/043 drove down 91.1% and exp045 measured at its
   floor — and rule 5 says a change there has a blast radius larger than the
   population it touches (exp044 paid +3,742 lines for exactly that).
2. **It is one event per large release, not a recurring mechanism.** 93% of the
   gate figure and 74% of the walk figure is a single group migration, and on the
   walk it is **0 on both calm hops** — 0 of 2,958 lines of churn.
3. **Its tail is the shape rule 1 exists for.** The single-member entries are the
   054 cross-module misfire: `getMcpInfo` "moved" from `value-serializer.js` to
   `motion-icon.js`, but the prior declaration takes `(toolUse, entrypointName)`
   and the fresh one takes `(modelIdRaw, capability)` and strips `[1m]` from it.
   That is upstream code change wearing a relocation's clothes. So 1,030 is an
   over-estimate at the tail; the trustworthy part is the ~928 lines that are
   group migrations.

## Task 0 — what the 7,358 NS-MEMBER lines actually are

`name-drivers.ts` classifies a substitution NS-MEMBER on one test: **the
identifier is followed by a dot.** That is a POSITION, not an alias. Resolving
each side against the file's own `require` header (rule 3 — state what the
predicate tests, check the sentence is the claim):

| class          |    4 hops | share | what it actually is                                          |
| -------------- | --------: | ----: | ------------------------------------------------------------ |
| MOVED-REN      | **3,746** | 50.9% | **not the same declaration** — a pairing artifact, see below |
| NOT-AN-ALIAS   | **1,798** | 24.4% | a plain local variable that happens to precede a dot         |
| **MOVED-KEEP** |   **974** | 13.2% | **genuine: the declaration changed split file**              |
| **WIDENING**   |   **650** |  8.8% | **genuine: the alias spelling widened**                      |
| MOVED-UNVER    |       190 |  2.6% | different path, export check fails on a side                 |
| **total**      | **7,358** |       | reconciles to `name-drivers.ts` exactly                      |

Per hop, 85→86 apart from the calm three:

| class        | 85→86 | 118→119 | 197→198 | 215→216 |
| ------------ | ----: | ------: | ------: | ------: |
| MOVED-REN    | 2,706 |     206 |     650 |     184 |
| NOT-AN-ALIAS |   814 |      88 |     512 |     384 |
| MOVED-KEEP   |    16 |       0 |      52 |     906 |
| WIDENING     |    32 |     562 |      48 |       8 |
| MOVED-UNVER  |     2 |       4 |     178 |       6 |

### The pairing artifact — half the class

Both `name-drivers.ts` and this decomposition pair a removed line with an added
line when their **masked shapes match**, FIFO within the file. Two unrelated
one-line calls have the same masked shape, so the pairing happens whether or not
they are the same statement:

```
- (0, systemBlockCharacters.initializeShades)();
+ (0, commandRunner.setupPlatformUtilities)();
```

Read as a substitution this is "alias renamed AND member renamed". It is neither.
The two lines are different entries in a lazy-init dependency prologue whose
contents changed. The usage site cannot settle it — the pair matched _because_
every non-identifier token agrees, so any counterfactual rewrite makes them
identical. Identity has to come from the DECLARATION
(`moved-ren-identity.ts`):

| hop     | decl shape IDENTICAL | DIFFERENT | not found | member is a minted `initializeAppNN` |
| ------- | -------------------: | --------: | --------: | -----------------------------------: |
| 85→86   |             117 (9%) |     1,068 |        66 |                            696 (56%) |
| 118→119 |               9 (9%) |        91 |         0 |                             47 (47%) |
| 197→198 |              10 (3%) |       269 |        18 |                            125 (42%) |
| 215→216 |               4 (4%) |        85 |         2 |                             33 (36%) |

**81–97% are not the same declaration**, and half of them read a
splitter-minted lazy-init initializer, which is not a carried identity at all.
Even the "identical shape" residue is mostly degenerate — `emptyFunctionItem`,
`noOperationItem`, `doNothingVar`, `placeholderFunctionVar`: every no-op stub in
the tree masks to the same shape, so shape identity is necessary and nowhere near
sufficient.

**Consequence beyond this experiment: 055's name-driver totals are an upper bound
on name churn, not a measure of it.** Any figure derived from shape-FIFO pairing
inherits this.

### The ceiling is a counterfactual, not an attribution

`ceiling.ts` does not attribute lines. It builds the tree in which the sub-cause
did not happen and re-runs the same `diff` that produces the reviewer-facing
number, so both sides are real texts and the figure cannot over-charge (054's
construction). Every rewrite is checked to touch identifier tokens only and to
leave the line count unchanged; a file failing either is dropped, not counted.

Verified against ground truth by hand on `floor/cli-interaction/task-serializer.js`:
the ceiling says 8 lines, and `diff | grep -c` for either alias spelling says 8,
in a file with 248 diff lines total.

**The brief predicted git-capping would REDUCE these figures, as 051's ledger
over-charged by 29%. It went the other way** — WIDENING 650 attributed → 894
capped, MOVED 974 → 1,030 — because `name-drivers.ts` charges a changed line pair
to its FIRST differing substitution only, so a line whose alias change is not
first is charged to some other bucket entirely.

## Task 1 — read them, per sub-cause

### WIDENING: one shadowing local, tree-wide, and it is exactly the stated mechanism

118→119 carries 770 of the 894. Three substitutions, 86 files:

| alias widened to                  | importers | shadowing binding that caused it                                            |
| --------------------------------- | --------: | --------------------------------------------------------------------------- |
| `statusIndicatorsStatusIndicator` |        25 | `agent/store/agent-editor.js:1433` — `let statusIndicator;`                 |
| `interfaceCommandEntry`           |        49 | `completion/plugin-cache/plugin-loader.js:2334` — `let commandEntry = …`    |
| `logTaskEventKairosCron`          |        22 | `table/config-sync/client-configs.js:856` — top-level lazy var `kairosCron` |

`isShadowed(declFile, name)` returns true if **any** importer of the module
declares or references that identifier anywhere, and `claimed` is global, so one
nested `let` in one file rewrites every usage site in all twenty-five importers.
The mechanism is confirmed exactly as the brief describes it.

Note the stability tier already exists: `claimPriorAliases` carries the prior
release's alias for a file ahead of the ladder. It fired everywhere it legally
could — which is why WIDENING is **0 on both calm walk hops**. What it cannot do
is keep an alias that has become illegal in one importer.

### MOVED-DECL: a cluster shedding a group, and the alias correctly following

215→216 carries 962 of the 1,030. **26 module-level declarations** — including
`commandLib`, the React namespace — moved from
`floor/cli-interaction/task-serializer.js` to
`storage/error-messages/auth-manager.js`. 866 of the 962 lines are in one
consumer, `user-input/id-inserter/slack-autocomplete.js`, at 399 usage sites.

This is a genuine partial migration, not a file rename: the source file survives
in 216 with **115 of its 147 exports**, and the consumer still imports it as
`taskSerializer` for the members that stayed. The walk shows the same shape twice
(`httpClientMergedConfig` shedding 31 members on 212→213 and 25 on 215→216).

**Why did they move? ANSWERED — a statement-hash collision.** This was left open
here ("the moved group appears in 0 placement-trail entries"), then the trail was
widened to describe every statement and it named the cause immediately.

The 26 declarations are one statement: a bare 32-declarator
`var globalObjectReference, fileSystemPromises34, …, commandLib, …;`. The hash
tier placed it, and the hash tier is the one tier that cannot move a statement —
so its prior home should have been `task-serializer.js`. It was not:

| what                                            | value                                      |
| ----------------------------------------------- | ------------------------------------------ |
| fresh statement hash                            | `8a7597db519cfa8d`                         |
| statements carrying it — fresh / prior          | 1 / 1, so the equal-count guard passed     |
| the prior statement it matched                  | `var cryptoModule48, StreamModule, …` (32) |
| does that prior statement declare `commandLib`? | **no** — it shares not one name            |
| its file, which the group inherited             | `storage/error-messages/auth-manager.js`   |
| `nameToFiles["commandLib"]` in the prior        | `floor/cli-interaction/task-serializer.js` |

`statementHash` is rename-invariant, so it MASKS identifier names. A bare
`var a, b, c, …;` has no content beyond its declarator count, and two unrelated
32-declarator statements hash identically. The equal-count guard that exists to
stop a new statement teleporting into an old cluster is what let this through:
there happened to be exactly one such statement in each release, so the match
looked unambiguous. Verified by hand, both texts read
(`hash-collision-probe.ts`), not inferred.

**It is rare and expensive, not a class.** On the whole 215→216 hop there is
exactly **one** hash-placed statement the name vote contradicts — this one — and
it is worth ~962 git lines. `alternatives` records the dissent:
`{name: task-serializer.js, allsame: task-serializer.js}` against a hash tier
that said otherwise, three tiers to one.

This is rule 8's corollary again, from exp046: **a hash can only be wrong about
what it serializes.** Whether to act on it is a separate question — the obvious
guard (distrust a hash match on a statement with no content but its shape, when
the name vote unanimously disagrees) is a change to the placement cascade, and
rule 5 applies to those.

## What this leaves for the next experiment

The alias axis is closed. Ranked by what the measurement actually surfaced:

0. **The masked-hash collision above** — one statement, ~962 git lines on the
   worst hop, now diagnosed and reproducible. The cheapest candidate guard is to
   refuse a hash match when the statement's masked form carries no content beyond
   its shape AND the name vote is unanimous against it. Ceiling it first: on this
   hop it fires once.
1. **Placement stability for group migrations** — 1,030/1,276 git lines. The
   instrument this needed has since been built: the placement trail now describes
   all 35,903 statements instead of 1,192, records where each one lived and why
   the hash tier missed, and it is what diagnosed item 0.
2. **`NOT-AN-ALIAS`, 1,798 lines** — local-variable drift misfiled by 055's
   spelling predicate. This is the LOCAL-DRIFT population 054 already attacks and
   051/052 sized as the LLM floor. No new lever here.
3. **MOVED-REN, 3,746 lines** — mostly not name churn at all. Roughly half is
   lazy-init prologue contents changing, which is the `initializeAppNN` /
   registrar surface exp049 worked. Whether that residue is reducible is a
   question about prologue stability, not about names.

## The product question, re-put with the real number

The brief asks that the WIDENING trade be re-put to the user with the measured
figure rather than 051's ≤256. It is:

> Letting an unshadowed importer keep the short alias is worth **at most 894 git
> lines over four gate hops / 680 over a four-hop walk, and 0 on every calm hop.**
> The cost is giving up "one alias per module tree-wide": a reader would see
> `kairosCron.x()` in twenty-one files and `logTaskEventKairosCron.x()` in the
> twenty-second.

At ≤256 lines 051 declined it. At 894 it is still under this experiment's own
pre-registered threshold, and the readability property it trades away is a
deliberate design choice documented at `cjs-emit.ts:297`. **Recommendation:
decline again.** It remains available if the readability trade is judged
acceptable — that call belongs to the user, not to the measurement.

## Claims in this directory's own brief that did not survive

- **"NS-MEMBER (require alias) ~7,360 lines, ~37% of all name-only churn."** The
  require-alias part is **1,924 git-capped, 26% of that.** The predicate behind
  the 7,360 tests whether an identifier is followed by a dot.
- **"The NS-MEMBER class is the only one with concentration."** True of the two
  real sub-causes; the bulk of the class (MOVED-REN) is a 1,513-pair long tail of
  unrelated lines paired by shape.
- **"Expect git-capping to reduce these, as 051's ledger over-charged by 29%."**
  Reversed: the attribution UNDER-charged both sub-causes, by 38% and 6%.
- **"exp051 measured MOVED-DECL at 300 lines over four hops and closed it as too
  small."** The recount confirms 051 undercounted (1,030), and confirms 051's
  conclusion anyway.
- **The brief's framing that both sub-causes are alias problems.** MOVED-DECL is
  a placement problem with an alias symptom; the brief itself hedged this
  ("the fix is probably NOT in aliasing") and the hedge was right.
- **"a single substitution is 494 occurrences, ~988 git lines, 23% of that hop's
  entire name-only churn."** Survives — the exact instrument reads 453
  occurrences and 962 git lines for the same event.

## Instruments added

| file                      | what it does                                                                      |
| ------------------------- | --------------------------------------------------------------------------------- |
| `ns-classify.ts`          | splits the NS-MEMBER population by resolving both sides against `require` headers |
| `ceiling.ts`              | git-capped counterfactual ceiling per sub-cause; rewrite, re-diff, guard, report  |
| `moved-ren-identity.ts`   | declaration-shape identity test — is a "substitution" the same declaration?       |
| `show-pairs.ts`           | prints the raw line pairs behind one (file, substitution), for rule 1             |
| `trail-check.ts`          | runs the placement trail on a real pair in ~1 min, no LLM; `TRAIL_DUMP=` to query |
| `hash-collision-probe.ts` | prints both statement texts behind a hash match, so it is read not deduced        |
