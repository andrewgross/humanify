# 044 — Task A: import-alias churn, sized correctly the second time

Brief: [README.md](README.md). Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md).

**Status: Task A COMPLETE. No `src/` change yet — the build premise shrank 5×
under verification and the decision is recorded below rather than acted on.**

## CORRECTION — an earlier draft of this file claimed 38%; it is 7.2%

The first pass classified a substitution as import-alias churn when the prior
name was an alias in the prior file and the fresh name was an alias in the fresh
file. That test never checked the two aliases named the **same module**, so it
counted every statement that legitimately calls a different module, plus every
mis-paired hash twin. It reported 2,966 git lines (38% of naming).

Adding the check that the prior alias's target is actually GONE from the fresh
tree — a renamed module rather than a different one — collapses that class from
**2,415 lines to 15**:

| class                                | first pass | verified |
| ------------------------------------ | ---------: | -------: |
| same import, re-aliased              |        551 |  **551** |
| import path changed (module renamed) |      2,415 |   **15** |
| **total alias churn**                |  **2,966** |  **566** |
| share of naming churn                |      38.0% | **7.2%** |

The same flawed test also drove the claim that 93.4% of the "6+ substitutions"
bucket was alias churn in disguise. Re-measured with the verified predicate it
is the reverse: of 3,874 lines on 85→86, **3,794 (98%) contain no alias
substitution at all.** exp039's correspondence hypothesis is therefore NOT
displaced — it remains the open question for that hop.

Two instrument caveats now measured rather than assumed: statements whose prior
twin was chosen from several same-hash candidates in the file (where the
substitutions read off are unreliable) are only **2.7% / 2.2%** of lines, so
mis-pairing is not what inflated the number — the unsound predicate was.

## What IS verified, and it is small, mechanical, and specific

Across all four hops, the number of import aliases that changed while their
target path stayed identical is **six**:

| hop     | alias change                                 | old name held in fresh by |
| ------- | -------------------------------------------- | ------------------------- |
| 85→86   | `skillFiles → apiResponseSkillFiles`         | NOBODY                    |
| 118→119 | `statusIcon → planReviewStatusIcon`          | NOBODY                    |
| 118→119 | `kairosCron → logTaskEventKairosCron`        | NOBODY                    |
| 215→216 | `proxyServer → runProxyServer`               | NOBODY                    |
| 215→216 | `toolResults → taskOrchestratorToolResults`  | NOBODY                    |
| 215→216 | `memoryExtractor → userInputMemoryExtractor` | NOBODY                    |
| 197→198 | (none — 0 of 1,505 shared paths)             | —                         |

1,527 of 1,528 shared paths keep their alias on 85→86. The stability tier in
`claimPriorAliases` (`src/split/cjs-emit.ts`) works. **Six decisions cost 551 git
lines** — 42% of 118→119's naming churn and 31% of 215→216's — because an alias
is tree-wide and every usage site in every importer re-prints.

## Root cause: a priority inversion, read to the line

`kairosCron`, 2.1.119. The alias was free — nobody else holds it. It was refused
because `nsNameIsFree` consults `importScope.isShadowed`, and one importing file
now declares a binding of that name:

    2.1.118  table/config-sync/client-configs.js exports
             kairosCronFactory, kairosCronProvider, kairosCronService
    2.1.119  the same file exports  kairosCron

The RENAMER minted a module-scope binding called `kairosCron`. That is a real
collision and the emitter is right to refuse to shadow it. But the priority is
backwards: **a brand-new binding name displaced a long-standing, tree-wide import
alias, and the alias moved in all 22 importing files.** Everywhere else in this
pipeline the incumbent wins and the newcomer moves.

## The decision, and why it is not taken here

The fix belongs in the renamer: treat the prior release's import aliases as
taken when minting module-scope binding names, so the newcomer picks
`kairosCronJob` and the incumbent alias survives. The prior ledger already
carries `aliases` and `--prior-version` already loads it.

Not built, for a reason worth stating: the ceiling is **551 git lines, 7.2% of
naming**, and the change reserves ~1,500 names inside a 3,500-line name
allocator whose output IS the naming metric. A lever that small with a blast
radius that large on the same axis needs its own ceiling measured for the
DAMAGE, not just the benefit — how many other bindings change name because
1,500 names became unavailable. That measurement is the honest prerequisite, and
it is the natural next step.

Alternatives considered:

- **Relax one-alias-per-module to per-file.** Twenty-one importers keep
  `kairosCron`; only the colliding file qualifies. Cheap and contained, but it
  breaks a deliberate, documented readability choice ("a reader should see the
  same import name for the same path in every file"), so it is the user's call,
  not a silent trade.
- **Rename the colliding local binding at emit time.** More invasive than the
  problem.

## Where naming noise actually is, after this

| slice                                  |     lines | share |
| -------------------------------------- | --------: | ----: |
| 85→86, 6+ substitutions, no alias      | **3,794** | 48.5% |
| everything else on 85→86               |     1,994 | 25.5% |
| verified import-alias churn (all hops) |       566 |  7.2% |
| remaining on the three calm hops       |     1,462 | 18.7% |
| **total**                              | **7,816** |       |

The dominant slice is unchanged from exp039's framing: statements on 85→86 where
six or more names differ and none of them is an import alias. That is the
correspondence question, and it is now the only large one left.
