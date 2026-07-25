# Refactor backlog: places where one small change costs 4+ edits

Every entry here was **measured while making a real change**, not spotted by
reading. The symptom is edit amplification: adding one tier, one field, or one
KPI forces edits at N sites that nothing keeps in sync, so the compiler catches
some mistakes and silence catches the rest.

Ranked by cost × how often it is paid.

## 1. Placement tiers are a hand-rolled ladder — 8 sites per tier

Paid twice in exp041 (content-anchor tier, all-same vote tier).

Adding ONE file-placement tier to `src/split/stable-split.ts` requires:

1. the `PriorTiers` interface
2. the tiers object in `assignWithPrior`
3. the `TierKind` union
4. the if-ladder in `decideStatementFile`
5. the `switch` in `recordTier`
6. `StableSplitStats`
7. `zeroTransferStats()`
8. the summary line in `src/commands/unified.ts`

**Naming already solved this.** `src/rename/prior-transfer.ts` exports
`TRANSFER_PIPELINE: TransferStep[]` — seven named steps, each `{name,
description, run(ctx)}`, executed in evidence order, each writing its own stats.
Adding a naming strategy is ONE entry, and `docs/naming-pipeline.md` is that
registry rendered as a table.

**Fix:** `PLACEMENT_TIERS: PlacementTier[]` with `{name, description,
decide(ctx, i) → file | undefined}` in evidence order. Counters derive from
tier names instead of bespoke fields; the log line renders generically; the
placement trail becomes two lines inside the single loop that walks it.

**Second-order win:** placement provenance stops being unrecoverable. Before
`src/split/placement-trail.ts`, answering "which tier put this statement here,
and what did the others have?" required a 300-line offline replay of the ladder
against both bundles (`experiments/041-content-anchor/replay-lib.ts`).

## 2. The matcher→split payload is re-declared at every layer — ~11 sites per field

Paid by `priorMatchMap` (Lever B) and again by `priorStatementTexts` (exp041).

One optional field travelling from the prior-version matcher to the split
touches five files and is named 16 times:

| file                                 | mentions | what it re-declares                           |
| ------------------------------------ | -------: | --------------------------------------------- |
| `src/prior-version/prior-version.ts` |        4 | result interface, empty result, build, return |
| `src/rename/prior-transfer.ts`       |        3 | return type, early return, final return       |
| `src/rename/plugin.ts`               |        4 | destructure, result interface, return object  |
| `src/commands/unified.ts`            |        1 | pass-through                                  |
| `src/split/stable-split.ts`          |        4 | option, guard, tier, call site                |

Every layer restates the payload it is only forwarding. Miss the empty-result
branch and the field is silently `undefined` on a path that still typechecks.

**Fix:** build one `PriorCarry` object in `prior-version.ts` and thread it
whole. Adding a field becomes one edit at the producer plus one at the consumer,
and the forwarding layers stop having an opinion.

## 3. `TransferStats` is cloned as `TransferStatsEntry` — 5 declarations

`src/rename/diagnostics.ts:40` defines `TransferStatsEntry` with the same four
fields, in the same order, carrying the **same comment** as
`TransferStats` (`src/rename/prior-transfer.ts:51`). The only difference is
`Record<string, number>` vs `Record<RenameRejectionReason, number>` — the clone
is strictly weaker typing.

The `{exactMatch, closeMatch, statementTwin?}` shape around them is then written
out longhand four times (`prior-transfer.ts` ×2, `plugin.ts` ×1,
`diagnostics.ts` ×2), so adding one strategy's stats is five declarations.

**Fix:** delete `TransferStatsEntry`, import `TransferStats`, and name the
container shape once (`type TransferStatsByTier = Record<string,
TransferStats>`), which also lets a new strategy appear without a type edit.

## 4. Eval KPIs are named independently in three tools

`analyze.ts`, `leaderboard.ts` and `summarize.ts` each hard-code the same KPI
set (19 / 11 / 35 mentions of `noise|reloc|mints`). Adding a KPI means editing
three tools plus the JSON shape, and nothing enforces that a KPI means the same
thing — or points the same direction — in all three.

**Fix:** one descriptor table — `{key, path in the stats JSON, lowerIsBetter,
format}` — that all three consume. Direction-of-good becomes data, so a
leaderboard cannot silently render a regression as an improvement.

## Not worth changing

**33 `.option()` calls in `unified.ts` alongside `CommandOptions`.** Adding a
flag costs 3 edits (option, interface field, pass-through), but that is
commander's normal shape and the compiler catches the interface half. Low
amplification, high churn risk. Leave it.

## Rule of thumb this suggests

When a subsystem grows a second instance of something (a second tier, a second
transfer strategy, a second KPI consumer), that is the moment to make the FIRST
one a registry entry. Both cases above went from "one hard-coded thing" to
"hard-coded ladder" without anyone deciding to; the ladder was never designed,
it accreted.
