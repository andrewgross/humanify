# 044 — Naming drift: which correspondence wins

> **STATUS (2026-07-26): TWO NEGATIVE RESULTS — nothing here shipped, and the axis
> is CLOSED.** See [RESULTS-correspondence.md](./RESULTS-correspondence.md).
>
> 1. The alias reservation FAILED the gate: naming +80, diff **+3,742 git lines**,
>    displacements unchanged at 6→6. It is unmerged on branch
>    `exp044-alias-reservation`. Refusing a name does not remove a collision, it
>    MOVES it.
> 2. exp039's correspondence hypothesis is REFUTED — the biggest naming slice is
>    **87.4% permutation** (exp036 rotation), which is irreducible.
>
> **Two numbers in this document were published and then RETRACTED.** Alias churn
> "38% of naming" is actually **7.2%**, and "93% of the 6+ substitution bucket is
> alias churn" is the reverse — **98% is not**. Both predicates tested something
> other than what their names implied. The corrected figures are in the RESULTS
> file; the transferable lessons are rules 3, 5 and 6 of
> [docs/measurement-pitfalls.md](../../docs/measurement-pitfalls.md).

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
_Idea → Evidence (table) → Conclusion_; **ceilings measured before builds**;
totals-first; every hop judged **on its own**.

Read [exp039's brief](../039-naming-drift/README.md) first — this is its
step 1 and step 2, carried out. Then [exp043 RESULTS.md](../043-name-family/RESULTS.md),
because placement work moved naming as a side effect twice and nobody predicted
either direction.

## Why now

exp041–043 took cross-file relocation from 15,699 to 1,390 git lines (−91.1%)
and measured the remainder as evidence-starved rather than mis-adjudicated.
Naming is now the largest reducible source by a factor of 5.5:

| source     |     85→86 | 118→119 | 197→198 | 215→216 | TOTAL |
| ---------- | --------: | ------: | ------: | ------: | ----: |
| **naming** | **5,760** |     452 |     674 |     730 | 7,616 |
| reorder    |     1,918 |     258 |   1,950 |   1,952 | 6,078 |
| relocation |       349 |      16 |     343 |     682 | 1,390 |
| alias      |        28 |      54 |      44 |      74 |   200 |

**85→86 carries 76% of it.** That is the small, quiet control pair — and the one
with the heaviest bundle reshuffle. A fix that only works on the calm hops is not
a fix.

## The census — measured, in GIT LINES

`name-drift-census.ts` takes exactly the population `diff-composition.ts` charges
as `naming` (same rename-blind statement hash, different text) and charges each
pair the same git lines, so these totals reconcile with the 034 gate rather than
being a parallel measurement in their own units. Because the hash is
rename-invariant — and property names and free identifiers are hash CONTENT —
two statements sharing a hash differ ONLY in binding identifiers, so the
substitutions read off by walking both token streams in lockstep.

### By how many names differ in one statement

| hop     | 1 sub         | 2 subs | 3–5 subs | **6+ subs** | total |
| ------- | ------------- | -----: | -------: | ----------: | ----: |
| 85→86   | 658           |    512 |      744 |   **3,874** | 5,788 |
| 118→119 | **332 (66%)** |    146 |       12 |          14 |   506 |
| 197→198 | **424 (59%)** |    148 |       68 |          76 |   718 |
| 215→216 | **646 (80%)** |     90 |       34 |          34 |   804 |

**Two different problems wearing one label.** The three calm hops are dominated
by SINGLE-name drift — one binding changed, the statement re-printed. 85→86 is
dominated by statements where SIX OR MORE names changed at once (67% of its
lines), which is not "a name drifted"; it is the whole statement being described
by a different set of names.

### By what kind of rename

| kind          | 85→86 | 118→119 | 197→198 | 215→216 | TOTAL | example                               |
| ------------- | ----: | ------: | ------: | ------: | ----: | ------------------------------------- |
| **other**     | 5,023 |     169 |     587 |     458 | 6,237 | genuinely different names             |
| **qualified** |   143 | **215** |      72 | **306** |   736 | `kairosCron → logTaskEventKairosCron` |
| **synonym**   |   309 |     119 |      54 |      38 |   520 | `idx → i`, `idx → index`              |
| **counter**   |   313 |       1 |       3 |       3 |   320 | `React123 → React93`                  |

Note what this corrects. The largest SINGLE-substitution drifts on 85→86 are all
vendor-alias counters —

    50 ln  React123 -> React93        26 ln  React114 -> React72
    30 ln  React117 -> React74        14 ln  React101 -> React21

— and they are the naming-axis twin of exactly what exp042/043 fixed for
placement: a minted counter is a slot number, not an identity. It is tempting to
build that next. **Don't, on this evidence: the whole counter class is 320 git
lines, 4% of naming.** The top-N list is not the mass. Sizing it first is the
only reason that is known.

`qualified` is more interesting than its total suggests: `toolResults →
taskOrchestratorToolResults`, `kairosCron → logTaskEventKairosCron` — the prior
name with a FILE-DERIVED qualifier bolted on. That is **placement churn becoming
naming churn**, and it is 38% and 43% of the two hops where relocation is now
lowest. The two axes are coupled; this is the seam.

## The hypothesis this experiment exists to test

exp039 measured 97.2% of 85→86's bindings named DETERMINISTICALLY, so "the LLM
re-rolled it" cannot be the story. The 6+ substitution slice sharpens the
alternative:

> When six names in one statement all change together, the renamer did not drift
> six times. It paired that statement's bindings with a DIFFERENT prior
> statement's bindings and transferred that statement's names, coherently. Two
> matchers disagree about which prior entity this one corresponds to — the
> function-fingerprint matcher and the statement-hash twin — and the noise metric
> (and the reader) use the second.

If that holds, the lever is about which correspondence wins, not about naming
stability, and it is the same shape as the fix that worked for placement:
**when two witnesses disagree, prefer the one corroborated by content.**

## The instrument's known gap — fix this FIRST

Tier attribution joins each fresh name to `strategyTrails.settledBy` from
`--diagnostics`. It currently leaves **50.6% / 85.9% / 71.4% / 86.0%** of lines
`unattributed`, because the index maps only the last applied `newName` per trail
entry and misses bindings settled outside that pipeline. The attributable part
says `binding-cascade` carries 33% of 85→86 and `exact-match` leads the calm
hops — but with that much unattributed, **no conclusion about tiers is safe
yet**. Any brief that ranks tiers on today's numbers is quoting an artifact.

## The work, in order

### A. Close the attribution gap, then READ pairs — do NOT build before this

1. Fix `settledByIndex` so every drifted fresh name resolves to a tier (join on
   binding identity, not on the name string; names are not unique).
2. Take the ten largest 6+ substitution statements on 85→86 and, for each, dump
   the fresh statement beside BOTH candidates: the prior statement the naming
   used, and the prior statement the hash twin says it is. Confirm or kill the
   hypothesis above by reading. Six hypotheses have now been refuted across
   exp040–043 by exactly this step, one of which fit the arithmetic perfectly and
   two of which came from a brief's own stated premise.
3. Read three `qualified` pairs and confirm the qualifier is file-derived.

### B. Build the winner — scoped, and the scoping is not optional

The lever: an incumbent import alias must not be displaced by a newly-minted
module-binding name. Ceiling 551 git lines; damage measured in
[RESULTS.md](RESULTS.md).

**The obvious implementation is the catastrophic one.** Seeding the renamer's
`usedNames` with the ~1,500 prior aliases blocks every binding from taking any
alias name — including the ~85 per hop that already HELD those names last
release. `logger` alone has 517–549 reference sites. That is the DESTABILISED
column, and it is why the damage was measured before building.

So the reservation must never reach a name that is being CARRIED OVER. Concretely,
in `src/rename/`:

- the TRANSFER path (`prior-transfer.ts`, `TRANSFER_PIPELINE` — exact-match,
  statement-twin, binding-cascade, module-pin, module-vote) is UNTOUCHED. Every
  name it settles came from the prior release, so by construction it cannot be
  the newcomer that displaces an alias.
- only names the LLM proposes (cold, and close-match) are checked against the
  reserved set, in `processModuleBindingBatch`'s callbacks
  (`buildModuleBindingBatchCallbacks` → `getProximateUsedNames` for what the
  model is told is taken, plus the proposal-acceptance check so a model that
  ignores the hint is still refused).

Scoped that way, destabilisation is zero BY CONSTRUCTION rather than by luck,
and the population is the six bindings measured: `stringDecoder`,
`memoryExtractor`, `dreamPrompt`, `kairosCron` (118→119) and `memoryExtractor`,
`apiRetry` (215→216).

Plumbing: the prior alias set is in the prior split ledger's `aliases` map.
`unified.ts` currently loads that ledger at line ~550, INSIDE the split, which
runs after rename — so it must also be loaded (or the aliases extracted) beside
`loadPriorVersionCode` and passed through the rename options, next to
`priorVersionCode`.

Behind `HUMANIFY_NO_ALIAS_RESERVATION=1`. TDD red-first. Watch for one specific
failure: a deflected name that lands on the collision ladder becomes a DECORATED
name (`kairosCronVal`), which exp042/043 established is itself a slot marker and
a fresh source of cross-version churn. If the ladder is what catches these, the
lever trades 551 lines of alias churn for six decorated names — measure that in
the gate rather than assuming it away.

### C. Gate

`experiments/041-content-anchor/gate-verdict.sh exp043-nearident <label>`.
Control is `exp043-nearident` (committed). The tier fires on SIX bindings across
four hops, so confirm it fires on those six BY NAME — a KPI that moves while
nothing fires where predicted is the failure mode this series keeps catching.
Non-negotiables unchanged: naming down on EVERY hop, `novel`/`realLn` unmoved, four boots, self-hop byte-identical
in bundle AND ledger, and the trail must show the new tier firing where the
ceiling said. **Watch `relocation` specifically** — it is at 1,390 and this lever
touches the matcher that placement tiers consume, so it can regress the work of
three experiments.

## Do NOT

- **Interchangeable-bucket assignment.** exp036 proved that population
  irreducible (isomorphic members, no cross-version identity); exp039's 391
  statement / 1,547 noiseLn rotation slice is the same set.
- **Rank tiers before fixing the attribution gap** (see above).
- **Build the counter-stabiliser first** because the specimens are vivid. It is
  320 git lines, 4%.
