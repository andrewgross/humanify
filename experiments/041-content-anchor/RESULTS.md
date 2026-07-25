# 041 — Content-anchor file inheritance: results

Brief: [README.md](README.md). Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md).

**Status: Task A complete (ceiling measured, every hop). Two levers found, one
of them not in the brief; a third candidate was measured and REFUTED by the
118→119 canary. Implementation and gate in progress.**

## TOTAL — what the ceiling says

| candidate             | 85→86      | 118→119  | 197→198    | 215→216    | verdict                         |
| --------------------- | ---------- | -------- | ---------- | ---------- | ------------------------------- |
| **anchor + all-same** | **+2,879** | **+716** | **+3,393** | **+1,257** | **build both**                  |
| anchor (the brief)    | +2,721     | +692     | +2,987     | +939       | build                           |
| all-same vote         | +626       | +24      | +2,713     | +952       | build                           |
| outer-names-only vote | +782       | **−52**  | +2,918     | +1,343     | **REFUTED — canary regression** |

Net git lines of cross-file relocation removed, per hop, measured at the point
of decision. Positive is churn removed. Every hop judged on its own: the
outer-names candidate wins two hops handsomely and **loses on 118→119**, which
is exactly the failure the canary exists to catch.

For scale, exp040's census of relocation on these hops: 7,583 / 791 / 6,205 /
1,842 git lines. The combined lever recovers **38% / 90% / 55% / 68%** of it.

Residue populations, for reference: 922 / 1,361 / 1,315 / 1,201 statements
placed by locality per hop, of which the combined lever decides 21.7% / 2.7% /
8.7% / 11.7%. A small share of statements carrying a large share of lines.

## How this was measured, and why not with `relocation-churn.ts`

exp040 sized relocation from the OUTPUT TREES: find statements that ended up in
a different file, ask whether a rare literal could have identified them. Right
size, wrong question — it cannot say whether the splitter would have USED the
evidence, because it does not know which tier placed each statement.

`experiments/041-content-anchor/replay-lib.ts` reconstructs the decision
instead. It replays `assignWithPrior` offline from the two bundles plus their
split ledgers — hash tier → name vote → identity fill → locality — so every
statement carries the tier that placed it. Candidates are then run on exactly
the population the brief targets: the locality residue.

**The replay is faithful, checked rather than assumed.** Its tier counts
reproduce the pipeline's own log line exactly, on every hop:

| hop     | replayed                                   | pipeline logged                                    | ledger agreement |
| ------- | ------------------------------------------ | -------------------------------------------------- | ---------------- |
| 85→86   | hash 10,954 · ordinal 466 · locality 922   | `10954 via hashes, 466 via ordinals, 922 residue`  | 19,960/19,966    |
| 118→119 | hash 12,945 · ordinal 579 · locality 1,361 | `12945 via hashes, 579 via ordinals, 1361 residue` | —                |
| 197→198 | hash 18,412 · ordinal 779 · locality 1,315 | `18412 via hashes, 779 via ordinals, 1315 residue` | 31,838/31,839    |
| 215→216 | hash 21,223 · ordinal 918 · locality 1,201 | `21223 via hashes, 918 via ordinals, 1201 residue` | 35,901/35,903    |

The handful of disagreements with the ledger are the identity-preempt tier,
which the replay does not model (the match map holds 5 entries on 215→216).

### The two numbers reported, and why the first one lies

- **DECIDES** — residue statements the candidate places with evidence.
- **NET** — what it does to relocation across the WHOLE tree: for every fresh
  statement with a priced prior twin, is it in the twin's file (aligned, an
  in-place edit in the diff) or not (a delete in one file and an add in
  another)? `healed` minus `broken`, in git lines.

NET is the honest one. Moving a statement also shifts the locality fallback of
every statement after it — a cascade the deciding tier never sees. A candidate
that heals 200 statements and breaks 150 is not a win, and only NET shows it.

Line pricing is the project's existing rule: today git prints the prior copy
deleted (`prior.lines`) plus the fresh copy added (`fresh.lines`); co-located it
prints only the lines that differ. Recovered = the difference. NET is a **lower
bound** — statements with neither a stable unique name nor a unique rare literal
cannot be priced and are invisible to it, in both directions.

## Finding 1 — the brief's mechanism is real, and its ceiling is worth building

Of the residue, the content anchor uniquely identifies **8.1% / 0.4% / 4.6% /
4.1%** (85→86 / 118→119 / 197→198 / 215→216) — a small share of statements carrying a large share
of lines, because the class it catches is enormous blocks.

Eyeballed, per the brief's standing rule. The two largest on 85→86:

```
390 ln  var initializeApplication16 = lazyInitializer(() => {     [fresh]
        var initializeApp307        = lazyInitializer(() => {     [prior]
        src/uri-validator/lsp-search/search-results.js  ->  src/session/remote/tab-truncator.js
        4 fresh-only lines, 4 prior-only, 27 shared rare literals

271 ln  var bootstrapApp10 = lazyInitializer(() => {              [fresh]
        var initModule74   = lazyInitializer(() => {              [prior]
        src/resources/permission/shell-sanitizer.js  ->  src/uri-validator/diff-tool/file-patch-renderer.js
        2 fresh-only lines, 2 prior-only, 45 shared rare literals
```

Exactly the class exp040 predicted: a minted-name lazy-init block, edited by a
couple of lines, whose name re-mints — so the hash tier and the name tier
abstain together and locality throws 390 lines into an unrelated file.

## Finding 2 — the brief's premise is wrong about WHY, and that opened a second lever

The brief states the residue has "no identity evidence at all". For most of it
that holds (`novote`: 722 / 1,316 / 1,194 / 1,048 of the residue per hop). But a
distinct sub-population — `conflict`: 200 / 45 / 121 / 153 — has strong evidence that
is **thrown away by the voting rule**:

```
fresh#22009  function generateContextUsageMarkdown(inputData, unusedOptions)
  generateContextUsageMarkdown  [all-same] -> src/query-input/interface/context-usage.js
  unusedOptions                 [all-same] -> src/query-input/interface/context-usage.js
  inputData                     [ordinal, 39th of 53 prior homes] -> src/hostname/logging/socket-logger.js
  => 2 votes, disagreement, drop to locality -> src/lsp/plugin-management/skill-hook-registry.js
```

The statement's own function name votes correctly. `inputData` — a **function
parameter** — votes for wherever the 39th `inputData` in the prior release
happened to live. `declaredNames` is `t.getBindingIdentifiers`, which returns
parameters and other nested bindings, so parameters vote and are written into
`nameToFiles` (`inputData` is recorded in 53 files). One junk positional vote
vetoes a 5-to-1 majority; `handleProxyRequest` loses 5 votes to 1 the same way.

Two fixes were measured:

- **all-same first** — when the voters disagree, a UNANIMOUS subset of all-same
  votes (names with exactly one prior home) decides; ordinal votes, which are
  positional guesses across dozens of files, no longer veto it. Strictly
  additive: it only fires where the shipped rule already gave up. Re-places
  **6 / 0 / 1 / 2** statements that had evidence — the risk is measurable and tiny.
- **outer names only** — drop parameters from the vote entirely. Bigger on two
  hops, but it re-places **25 / 104 / 89 / 71** statements that the shipped rule
  placed WITH evidence, and it **regresses 118→119 by 52 lines**. Refuted.

## Finding 3 — minted names on new code (Task D)

Noted while reading pairs; the minted-name census is a separate lever. The
relocating blocks are named `initializeApp307`, `bootstrapApp10`,
`initializeApplication16`, `initModule74` — all wordless mint shapes on
`lazyInitializer` blocks. 2.1.86 carries 3,273 such blocks, 1,868 minted-named.
The anchor tier does not fix the names; it stops them from moving.

## What ships

`src/split/content-anchor.ts` — pure, order-independent, unit-tested. Gates, all
abstaining rather than guessing: rare on both sides → unique candidate → ≥50%
token overlap → unique claim (two fresh statements resolving to one prior
statement both abstain). The similarity gate is load-bearing: without it a
single shared string paired a 5,073-line statement with a 7-line one and
inflated a hop's relocation reading from 1,842 to 8,956 lines.

The ceiling script measures the SHIPPED function rather than a copy of its
rules, so the number above is a measurement of what actually runs.

Kill switches: `HUMANIFY_NO_CONTENT_ANCHOR=1`, `HUMANIFY_NO_ALLSAME_VOTE=1`.

## Instrumentation: placement is now recorded, not reconstructed

Task A cost a 300-line offline replay of `assignWithPrior`
(`replay-lib.ts`) purely to recover something the splitter knew and discarded:
**which tier placed each statement, and what evidence the others had.** The run
log reports totals (`922 residue by locality`) but never which statements, so
"why did this 390-line block move?" was unanswerable from the output.

Naming does not have this problem. `src/rename/prior-transfer.ts` exports
`TRANSFER_PIPELINE: TransferStep[]` — seven named steps, each with a
description and its own stats — and `strategy-trail.ts` records, per
identifier, every strategy that considered it. Placement had neither.

`src/split/placement-trail.ts` closes the recording half: a module singleton in
the same shape as `strategyTrail`, enabled by `--diagnostics`, drained into the
diagnostics JSON as `placementTrails`. Every tier is counted; the tiers that
LOST (locality) and the newest ones are described statement by statement, with
the votes that were cast and discarded. Detail is deliberately restricted —
the diagnostics file is already ~100 MB, and the hash and name tiers place ~90%
of statements uneventfully.

The value is not hypothetical: Finding 2 above is one line of this trail.

    placedBy "novote", votes ["…/context-usage.js", "…/socket-logger.js"]

### Still to do: make placement a registry, like naming

`assignWithPrior` is a hand-rolled ladder — a `PriorTiers` struct, an if-chain
in `decideStatementFile`, and a `switch` in `recordTier`. Adding ONE tier means
editing eight sites that nothing keeps in sync: the `PriorTiers` interface, the
tiers object, the `TierKind` union, the ladder, the `recordTier` switch,
`StableSplitStats`, the stats mapping in `stableSplitFromCode`, and the log line
in `unified.ts`. This experiment made those eight edits twice.

A `PLACEMENT_TIERS: PlacementTier[]` registry — `{name, description,
decide(ctx, i) → file | undefined}`, in evidence-strength order — collapses that
to one entry per tier, derives the counters from the registry instead of
bespoke fields, renders the log line generically, and makes the trail a
two-line addition inside the single loop that walks it.

Deliberately NOT done in this commit: it is a refactor landing beside a measured
behavior change, and if a hop regressed there would be no way to tell which
caused it. It follows as its own commit, certified byte-identical against this
one's output.
