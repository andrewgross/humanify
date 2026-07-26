# Refactor backlog: places where one small change costs 4+ edits

Every entry here was **measured while making a real change**, not spotted by
reading. The symptom is edit amplification: adding one tier, one field, or one
KPI forces edits at N sites that nothing keeps in sync, so the compiler catches
some mistakes and silence catches the rest.

Ranked by cost × how often it is paid. Each entry carries its status; a fixed
entry stays here with its measurement intact, because the measurement is the
argument for fixing the next one.

| #   | entry                                          | status                  |
| --- | ---------------------------------------------- | ----------------------- |
| 1   | placement tiers, a hand-rolled ladder          | **DONE** — `8d28d4f`    |
| 2   | matcher→split payload re-declared per layer    | **DONE** — `PriorCarry` |
| 3   | `TransferStats` cloned as `TransferStatsEntry` | **DONE** — type-only    |
| 4   | eval KPIs named independently in three tools   | **DONE** — `kpis.ts`    |

## 1. Placement tiers are a hand-rolled ladder — 8 sites per tier

**DONE** (`8d28d4f`), certified byte-identical against the gate's own 215→216
output. `PLACEMENT_TIERS` is now a registry of `{name, label, description,
decide(ctx)}` in evidence order; the counters, the run log and the diagnostics
trail all derive from it, so a new tier is ONE entry. The original measurement
follows, because it is the evidence for the entries still open.

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

**DONE**, certified byte-identical against `exp043-nearident`'s 215→216 output.
`src/split/prior-carry.ts` names the payload in two stages — `MatcherCarry` for
what is collectible while the prior AST is alive, `PriorCarry` for that plus the
`matchMap`, which cannot exist until every rename pass has settled the final
names. Both take REQUIRED fields, so the "forgot the empty-result branch"
failure is now a compile error rather than a silent `undefined`. The four
forwarding layers name the carry once each and have no opinion on its contents.
Adding a field costs one edit at the producer and one at the consuming tier.

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

**DONE.** The clone is deleted, and the container is named once as
`TransferStatsByTier` in `prior-transfer.ts`; `TransferContext.stats` is its
`Partial`, which is what an incrementally-filled version actually is. The change
is type-only — the diff contains nothing but type imports and declarations, so
it erases at compile time and no byte-identical run was needed.

Reading it turned up what a clone costs. Three of the four declarations of this
shape omitted `retry`, while `retry` is present in every diagnostics JSON on
disk (`{attempted: 23, applied: 10, skipped: 13}` on 215→216) — the object was
forwarded whole and the narrower types simply lied about it. Nothing was broken,
but for four sites the declared shape and the produced shape had drifted apart
with no way to notice.

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

**DONE** — `experiments/034-eval-harness/kpis.ts`. Every numeric cell the two
tables print is byte-identical to before; what is new is that direction is data.
`analyze.ts` now annotates its output as the shared `Scorecard`, so the producer
and both consumers are checked against one shape, and `summarize.ts`'s
thirteen-positional-argument `row` is gone — its columns render from the
registry.

Direction turned out to matter more than the edit count. Marking `novel` and
`realLn` as `hold` put them on the leaderboard for the first time: the gate has
always required real change to stay still, and the table people actually paste
into write-ups could not show it. Marking `reloc` as `context` states in the
output itself that it rose on all three experiments that cut relocation 91%.

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
one a registry entry. Every case above went from "one hard-coded thing" to
"hard-coded ladder" without anyone deciding to; the ladder was never designed,
it accreted.

All four are now fixed, and each cost far less than the measurement that
justified it — #1 was eight sites, #2 was eleven, #3 was five declarations, #4
was three tools, and not one of them changed a single output byte. The expensive
part was noticing. That is the argument for writing the next one down the moment
you pay it twice, rather than after the third.

The backlog is empty. Keep the entries: the next one gets added the same way,
by measuring it while paying it.
