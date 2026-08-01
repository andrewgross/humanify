# 058 — Binding-derived placement: put a statement where its bindings came from

> ## STATUS: **(B) CLOSED — refuted, not merely under threshold.** (A) survives Task 0/1 and is a build candidate. See [`RESULTS.md`](./RESULTS.md), not this brief.
>
> - **(B) is worse than (A), not better.** Git-capped over four gate hops it removes
>   21 more lines and **creates 61**; over four walk hops, 122 more and **creates
>   154**. The bar was "beat (A) by ~1,500".
> - **This brief's central premise does not survive.** The ~97% correspondence is
>   not lost at the boundary — `binding-cascade` _ships the prior name_, so
>   carrying it is the identity function and `identityTier` degenerates into the
>   `all-same` vote it already sits below. (B) has no information to promote, only
>   rank.
> - **The singleton-rejection guard's "0 of 37,966" is neither excellent precision
>   nor a broken guard.** Two of its three tests cannot fail inside a hash bucket,
>   and it does not reach the module-binding cascade at all (0% of 11,094 accepts).
> - **13 disagreements exist across eight hops; all were read. The hash tier is
>   right on 9 and wrong on 4, and every one of the 4 is a zero-initializer
>   declaration.**
> - **(A)** removes **1,025 gate / 1,477 walk** git-capped lines, **0 created on
>   every hop**, and is provably inert on the 63 of 67 statements where the
>   fingerprint was already right.
>
> **This is a BRIEF — a hypothesis, including its cautions.** Whoever finishes it
> stamps a STATUS block here naming which of its claims did not survive.
>
> **This experiment may correctly produce NO CODE.** 051, 052, 053 and 057 each
> did, and each was right. It may also correctly produce only the SMALL half of
> what it proposes — see "two candidate changes" below, and price both.
>
> **Do not write a line of pipeline code until Task 0 produces two numbers: the
> matcher's precision, and a git-capped ceiling.** In that order. Task 0a is a
> veto, not a sizing exercise.

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md).

**Read first, in order:**

1. [`CLAUDE.md`](../../CLAUDE.md) and [`docs/measurement-pitfalls.md`](../../docs/measurement-pitfalls.md) — eleven rules; 5, 6, 11 and 1 all bite here.
2. [`057/RESULTS.md`](../057-alias-stability/RESULTS.md) — where this came from, and the collision it diagnosed.
3. [`docs/matching-cascades.md`](../../docs/matching-cascades.md) **sections A and C** — the binding cascade that already exists, and the placement cascade this would change.
4. [`054/RESULTS.md`](../054-post-split-reconcile/RESULTS.md) — the precedent for carrying identity across the rename→split boundary, including why a NAME is not a usable key there.
5. [`053/README.md`](../053-shingle-audit/README.md) — the measured cascade census this brief's numbers come from.

---

## The claim

Placement decides **per statement**, ranking evidence in a fixed order with the
content fingerprint (`statementHash`) first and unconditional. But the matcher
has already resolved **which prior binding each fresh binding is**, for ~97% of
them, using exactly the evidence that would settle placement: call-graph
neighbours, callee hashes, member keys, enclosing statements, propagation.

That correspondence is then compressed at the rename→split boundary into a
`Map<newName, priorName>` **containing only bindings whose name CHANGED** — 4
entries on the 2.1.216 run, 10 on 215, out of ~24,500 module bindings. The two
placement tiers that consume it (`preempt`, `fill`) sit BELOW the fingerprint.

So the proposal: **place a statement by where its bindings' matched prior
counterparts lived**, and let that outrank a content fingerprint.

### The case that motivates it, measured in 057

One statement — `var globalObjectReference, …, commandLib, …;`, 32 declarators,
no initializers — was placed by the fingerprint into the wrong file. Masking the
names leaves a fingerprint of "a `var` with 32 empty slots", which matched an
unrelated 32-declarator statement sharing not one name. One occurrence per side,
so the equal-count guard passed.

Cost: 32 exports relocated, every consumer's import alias rewritten, **~962 git
lines on one hop**. All 32 names are live (32/32 exported, assigned, and read by
other files; 0 dead). Three other tiers — `name`, `allsame` — said the right file
and were outranked.

## Two candidate changes. Price BOTH; they are not the same bet.

**(A) The narrow rule.** A statement whose masked form carries no content beyond
its shape — a declaration with zero initializers — cannot be claimed by the hash
tier; it falls through to the name/identity evidence. Structural, no threshold to
tune. Ceiling on 215→216 is ~962 lines and it fires **once** on that hop.

**(B) The architecture.** Carry the real binding correspondence into the split
and let it outrank the fingerprint generally. Subsumes (A). Unknown ceiling,
much larger blast radius.

**If (A) captures most of (B)'s measured value, ship (A) and close (B).** Say so
explicitly in RESULTS either way. Shipping the architecture for a benefit the
patch already delivers is how rule 5 gets paid twice.

### One thing that is NOT the lever, established before you start

**Widening `matchMap` to cover unchanged names adds nothing.** It skips them by
design (`buildPriorMatchMap`: `if (finalName === priorName) continue`) and for an
unchanged name the "identity" and the name vote carry the _same_ fact — the prior
home is `nameToFiles[name]` either way. The lever is **rank**, plus the
empty-fingerprint case. Do not spend a day widening a map that adds no
information.

## Task 0a — matcher precision. A VETO, not a sizing exercise. No pipeline code.

Promoting the matcher above the fingerprint converts **every wrong match into a
file move**, which is the expensive failure mode (two files churn plus every
importer). Today the fingerprint is an independent check on the matcher. Removing
that check without knowing the matcher's precision is the shape of rule 5.

What is known and is not reassuring: on 215→216 the **singleton-rejection guard
fired 0 times out of 37,966 singleton accepts**, and there were 16 injectivity
demotions. Zero is either excellent precision or a guard that is not testing what
its name implies (rule 3). **Establish which.**

Produce:

1. The population where binding-correspondence placement would DISAGREE with
   today's placement, per hop, off the trees and ledgers on disk.
2. **Read twenty of them by hand** (rule 1). For each: is the matcher right, or
   would promoting it have moved correct code?
3. A live check that singleton rejection can fire at all — construct an input
   where it must, and watch it.

**Veto rule, fixed now: if more than 2 of 20 read disagreements show the matcher
placing correct code WRONGLY, close (B) and consider only (A).** A tier that is
right 90% of the time is not a promotion candidate when the 10% relocates code.

## Task 0b — the ceiling, git-capped, for (A) and (B) separately

Same construction as 054 and 057, and for the same reason: a decomposition
attributes lines, it does not bound them. Build the counterfactual tree, re-run
the `diff` that produces the reviewer-facing number, report lines removed and
lines CREATED separately and never netted.

Reuse [`057/ceiling.ts`](../057-alias-stability/ceiling.ts) — it already does
rewrite → guard → re-diff, checks that only the intended tokens moved, and drops
any file whose line count changes.

Report per hop, **85→86 separately from the calm three, always**, and add the
four walk hops (`/work/exp056-clean`), because the gate pairs are not the
production shape.

**Decision rule, fixed before the measurement:**

- **(A)** under **~800 git-capped lines across four hops** ⇒ close it. It is
  already known to be ~962 on one hop, so this is a low bar it should clear;
  if it does NOT, the 057 figure was hop-specific and that is the finding.
- **(B)** must beat (A) by more than **~1,500 git-capped lines across four
  hops** to justify its blast radius. Under that, ship (A) and close (B).
- Either way, both are inside the **±2,800 lines/hop** draw band, so the gate
  must be draw-pinned (see below). A cold A/B cannot resolve this and will
  print a confident sign anyway (rule 11).

## Task 1 — read twenty, per class

Rule 1 has refuted a hypothesis in every experiment of this arc, including three
inside 054 and two inside 057 — and in 057 it refuted the brief's own headline.
Classify the disagreements before you price them; "the fingerprint was wrong"
and "the code genuinely moved upstream" look identical from the placement site.

## Task 2 — build, only if something survives

State the ceiling in git lines **before writing code**. Behind a kill switch
(`HUMANIFY_NO_<X>=1`, the house pattern), TDD red-first — the failing test must
reproduce the real misplacement, not a synthetic one.

### Design notes, so you do not re-learn these

- **The tier already exists.** `identityTier` / `viaIdentity` / `viaIdentityPreempt`
  in `stable-split.ts` do exactly "this statement declares bindings whose matched
  priors lived in ONE file". Extend and re-rank it; do not add a parallel tier.
- **A name is not a key across this boundary.** 054 measured **27% of renames
  share a `fromName` with another binding in the same file**, and one trail held
  `retryAttemptCount` mapping to two different new names. 054 solved it by
  carrying `emitIndexes` — the bundle statement index behind each emitted slot.
  Any richer carry here inherits that problem; use a real key.
- **The unanimity rule is load-bearing.** Every tier that inherits a file
  requires a single unanimous answer and abstains otherwise. A statement whose
  bindings came from two files has no answer — do not invent a majority rule
  without pricing it separately.
- **`PLACEMENT_TIERS` is a registry.** A tier's counters, the run log and the
  diagnostics trail all derive from that one list; adding or reordering is one
  edit there, not eight across four files.

### Two hazards this change specifically has

- **Rule 5.** exp044's alias reservation measured its own damage correctly,
  destabilised exactly what it said it would — and cost **+3,742 git lines**,
  because module-binding names feed the split's name votes AND the emission-order
  alignment. Reordering the placement cascade has a blast radius larger than the
  statements it re-places. Measure the cascade, not the target.
- **Rule 6.** Refusing the fingerprint on one statement class does not remove a
  mis-placement, it moves it. Show why yours is not whack-a-mole before building.

### Explicitly out of scope — measured, do not retry

**Raw AST position as a placement or disambiguation signal.** 057 measured how
far honest fingerprint matches drift between releases:

|                                 | median |   p90 |   p99 | worst |
| ------------------------------- | -----: | ----: | ----: | ----: |
| 215→216                         |   0.1% |  7.0% | 17.7% | 66.3% |
| 85→86 (upstream reordered ~35%) |   1.2% | 15.6% | 40.1% | 52.7% |

The collision sits at 33.8%. Any cutoff that rejects it also rejects hundreds of
honest matches on 85→86 (383 at a 25% cutoff). This is the same axis that cost
**+50,606 noise lines** as a positional tie-break. `057/position-signal.ts`
re-runs the measurement if you want to see it yourself; do not spend more than
that on it.

## Measurement rules this experiment must follow

Each cost something in 054, 055 or 057.

1. **Ground truth is `diff -r`, not the decomposition.** `composeDiff`'s total is
   off −24.0% on 85→86 and +17.2% on 197→198.
2. **`noiseLn` scores the BUNDLE** and is structurally blind to a change that
   only moves split files. It read 0 delta on every hop of the 054 arc. Never
   conclude "no effect" from it.
3. **`layout.noise` understates a tree-level change up to 30×.**
4. **Never compare across two cold runs.** That manufactured a phantom +254
   regression in 055. Compare ON to OFF within one run.
5. **Per-file attribution is unreliable** even when totals are fine (p90 relative
   error 28.6%). Quote aggregates.
6. **`reloc` is NAME-keyed** and rises when you place things correctly (rule 7).
   Use `relocatedStatements`.

## Gate — only if something ships

[`041/gate-verdict.sh`](../041-content-anchor/gate-verdict.sh), every hop judged
on its own, plus:

1. **Direct `diff -r` churn down on every hop.** Binding criterion.
2. `novel` / `realLn` unmoved — necessary, NOT sufficient.
3. **Boot gate green, BOTH halves** — `--version` AND a live
   `-p "say exactly: boot-ok"`. `bun` is at `~/.bun/bin`; confirm it resolves in
   the harness's environment, because without it the check is skipped SILENTLY.
4. Self-hop judged **draw-pinned**, never cold ([`054/pin-selfhop.sh`](../054-post-split-reconcile/pin-selfhop.sh),
   which compares the TREE, not just the bundle).
5. **Draw-pinned A/B** ([`054/pinned-ab.sh`](../054-post-split-reconcile/pinned-ab.sh)).
   Prove the pinning — both legs must write ~0 cache entries — and state what the
   pinned run cannot see.
6. **`056/walk.sh` as the final check.** Placement churn compounds; a single-hop
   gate cannot show whether a fix holds or decays. Baseline in
   [`056/RESULTS.md`](../056-multi-hop-walk/RESULTS.md): calm hops 1,391 and 1,567.

## Instruments to reuse rather than rebuild

| file                                     | what it gives you                                                                                                                                                                      |
| ---------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `057/trail-check.ts`                     | runs the placement trail on a real pair in ~1 min, no LLM. `TRAIL_DUMP=<path>` dumps it for querying. **Pass the PRIOR BUNDLE too** or the content-anchor tier is off and moves read 0 |
| `057/ceiling.ts`                         | git-capped counterfactual ceiling: rewrite, guard, re-diff, report created lines separately                                                                                            |
| `057/hash-collision-probe.ts`            | prints both statement texts behind a fingerprint match, so a claim is read not deduced                                                                                                 |
| `057/decl-usage.ts`                      | per-name: exported / assigned / read here / consumers — is a declaration live?                                                                                                         |
| `057/position-signal.ts`                 | the drift distribution, if you want to re-refute the position idea                                                                                                                     |
| `055/full-ledger.ts`                     | per-hop files/lines/statements/noise, reconciled to ground truth                                                                                                                       |
| `054/pinned-ab.sh`, `054/pin-selfhop.sh` | the draw-pinned gate and tree-level self-hop                                                                                                                                           |
| `056/walk.sh`                            | cold build + N-hop walk, per-hop churn and boot                                                                                                                                        |
| `placementTrail` (`--diagnostics`)       | **now describes 100% of statements** (was 3.3%), with `priorFile` / `priorFileFrom` / `hashMiss` / `alternatives`. This is what diagnosed the 057 collision                            |

## State of the tree

Branch `exp058-binding-placement`, off `main` at `a7111d9`, carrying two commits
that are prerequisites and are NOT yet on main:

- the widened placement trail + LLM naming provenance (diagnostics only — the
  recorders are off unless `--diagnostics`, and no emitted byte should change);
- 057's RESULTS and instruments.

`npm run check` is green (1,688 pass / 0 fail / 1 skipped). The skip is the
`bun#11100` canary, version-gated because bun 1.3.14 fixed the upstream bug.

The trail's **"changes no emitted byte" claim is now MEASURED, not reasoned**
(`trail-inert.ts`): the real split run twice on 2.1.215→216, trail off and on,
gives 1,497 files with an identical tree digest, an identical ledger digest and
identical placement tier counts — with 0 trail entries off and 35,903 on, which
is the control proving the second leg was enabled rather than the two agreeing
because neither did anything. On that basis the diagnostics commit was moved to
`main`; this branch carries only 057's results, this brief, the bun canary gate
and that proof.
