# exp208 — matching counts as a stability check

STATUS: DONE, NOT MERGED (2026-11-10), branch `exp208-matching-stability`,
gate green (worktree + clean checkout) on `41caeca7c`.
Harness only (`crates/humanify-ag-dev`); the pipeline and its output are
unchanged. noise-bands.json NOT regenerated here (the main agent adopts
the 40921b9f bands; a `matchingBands` block appears in the slot the next
time `eval bands` writes it).

Andrew's idea: the KPIs move between same-commit cold repeats because the
LLM draws different names, but how many functions / bindings / identifiers
the pipeline MATCHES versus SENDS TO THE LLM should be stable or exactly
equal. Make those counts a first-class check, so a harness or determinism
problem shows up directly instead of being inferred from noisy KPIs.

## What was built

- **`eval::matching`** — a `MATCHING` spec read from each scored run's
  `<to>.stats.json` (`coverage`, `resolutionStats`) through the exp206
  diagnostics engine, which is now shared (`eval::llm_diag::Spec`; the
  call-ledger digest is `LLM_CALLS`, unchanged output). Columns:

  | column        | stats source                                     | kind   |
  | ------------- | ------------------------------------------------ | ------ |
  | `fnTotal`     | `coverage.functions.total`                       | EXACT  |
  | `fnCarried`   | `coverage.functions.cached`                      | banded |
  | `fnClose`     | `coverage.functions.closeMatch`                  | EXACT  |
  | `fnAsked`     | `coverage.functions.llm + .failed` (asked the LLM, renamed or not) | EXACT |
  | `fnLLMRenamed`  | `coverage.functions.llm` (asked AND renamed)    | banded |
  | `fnUnmatched` | `resolutionStats.unmatched`                      | EXACT  |
  | `fnAmbiguous` | `resolutionStats.stillAmbiguous`                 | EXACT  |
  | `mbCarried`   | `coverage.moduleBindings.cached`                 | banded |
  | `mbLLMRenamed`  | `coverage.moduleBindings.llm`                    | banded |
  | `idAsked`     | `coverage.identifiers.total` (asked the LLM)     | banded |
  | `idLLMRenamed`  | `coverage.identifiers.llm`                       | banded |

- **`eval summarize`** prints a `MATCHING (per pair)` block after the LLM
  call diagnostics and writes `summary.json` `matchingDiagnostics`.
- **`eval leaderboard`** prints `MATCHING (totals across the compared
  pairs)` side by side with deltas. When every label is at the SAME commit,
  it prints `!! MATCHING NOT EQUAL at the same commit: …` for any EXACT
  column that moved, and `!! MATCHING outside its band …` for a banded
  column that moved past its `matchingBands` entry. At different commits:
  one line, "not judged".
- **`eval bands`** checks the repeats per pair: an EXACT count that differs
  on any pair REFUSES the whole write (exit 2, `!! MATCHING NOT EQUAL: <col>
  on <pair>: r1=… r2=…`, nothing written — a determinism problem means the
  KPI spread is not a floor either). Otherwise it writes `matchingBands`
  (spread of each column's totals over the pairs) into the same
  (scorer, mode) slot as the KPI bands and prints one line naming each.
  Repeats whose summaries lack `matchingDiagnostics` still get KPI bands,
  with a `matching counts NOT measured` line.
- Loader: `NoiseBands::matching_bands(scorer, mode)` (optional key; old
  slots read as none). Old labels/stats without the fields print `-`.

## Which counts are exact, and why

Evidence: seven same-commit cold repeats at two commits, five pairs each
(35 pair-runs): `ref-names-40921b9f-r1/r2/r3` + `serial-40921b9f-s1`, and
`ref-names-111401fe-r1/r2/r3`.

- EXACT (equal on every pair of every repeat at both commits): functions
  total, close-matched, sent to the LLM, unmatched, still ambiguous. These
  are structural matching decisions taken before any model answer.
- The stats' key names mislead (exp209, branch `exp209-llm-ask-drift`):
  `coverage.*.llm` counts ASKED AND RENAMED, and `coverage.functions.failed`
  counts ASKED, NOTHING RENAMED (not a transport failure). So functions
  asked = `llm + failed`, and it is EXACT: equal in r1/r2/r3/s1 on all five
  pairs (631 / 1895 / 918 / 1387 / 1097), and in 111401fe r1-r3. `functions.llm`
  moved by 1 on 2.1.181 and 2.1.198 because the model kept the minified
  name (exp209); the asked sum never moved. Every column is labelled with
  what it counts: `fnAsked`, `fnLLMRenamed`, `mbLLMRenamed`, `idAsked`,
  `idLLMRenamed`.
- `moduleBindings.total` is carried + asked only; bindings named by the
  statement twin are in neither, so it can move by 1 (exp209). Not shown.
- `identifiers.skippedBySkipList` mostly counts bindings already carried
  (exp209). Not shown.
- `fnCarried` is BANDED although equal across all four 40921b9f repeats:
  at 111401fe it read 46239 / 46237 / 46239 on 2.1.181 (offset exactly by
  `alreadyNamed`). On scratch bases the prior is itself a cold LLM run, so
  whether a matched function counts as "carried" or "already named" can
  follow the base's draws. Making it exact would have refused a valid
  reference.
- Module bindings carried / renamed, identifiers asked / renamed, and the
  exact- and close-match transfer counts follow the names of the cold-built
  scratch base (exp209): legitimately banded, not bugs.

## What the block prints (40921b9f, totals over five pairs)

```
=== MATCHING (totals across the compared pairs) ===
-- FUNCTIONS (total; carried from the prior; close-matched; asked the LLM = stats llm+failed; asked AND renamed by the LLM = stats llm; unmatched; still ambiguous)
                      fnTotal fnCarried fnClose fnAsked fnLLMRenamed fnUnmatched fnAmbiguous
ref-names-40921b9f-r1  254797    220762    3595    5928         5923        4749        4756
ref-names-40921b9f-r2  254797    220762    3595    5928    5922 (-1)        4749        4756
ref-names-40921b9f-r3  254797    220762    3595    5928    5922 (-1)        4749        4756
serial-40921b9f-s1     254797    220762    3595    5928         5923        4749        4756
-- MODULE BINDINGS (carried from the prior; asked AND renamed by the LLM; statement-twin names are in neither)
                       mbCarried mbLLMRenamed
ref-names-40921b9f-r1      95920         1717
ref-names-40921b9f-r2 95923 (+3)    1714 (-3)
ref-names-40921b9f-r3 95922 (+2)    1715 (-2)
serial-40921b9f-s1    95919 (-1)    1715 (-2)
-- IDENTIFIERS (asked the LLM; asked AND renamed by the LLM)
                          idAsked idLLMRenamed
ref-names-40921b9f-r1       26090        25929
ref-names-40921b9f-r2 26105 (+15)   25928 (-1)
ref-names-40921b9f-r3  26095 (+5)  25918 (-11)
serial-40921b9f-s1     26083 (-7)  25941 (+12)
```

Per pair (r1 summarize):

```
                 fnTotal fnCarried fnClose fnAsked fnLLMRenamed fnUnmatched fnAmbiguous
2.1.85->2.1.86     35872     30700     670     631          630         917         931
2.1.118->2.1.119   42981     35645     404    1895       1894         473         890
2.1.179->2.1.181   53295     46826     890     918        917        1146         920
2.1.197->2.1.198   58156     50459     942    1387       1386        1397        1150
2.1.215->2.1.216   64493     57132     689    1097       1096         816         865
```

`eval bands` dry run (scratch copies; the worktree's bands files restored):

| repeats              | exact counts | fnCarried | fnLLMRenamed | mbCarried | mbLLMRenamed | idAsked | idLLMRenamed |
| -------------------- | ------------ | --------- | ---------- | --------- | ---------- | ------- | ---------- |
| 40921b9f r1-r3 + s1  | all equal    | ±0        | ±1         | ±4        | ±3         | ±22     | ±23        |
| 111401fe r1-r3       | all equal    | ±2        | ±1         | ±5        | ±6         | ±26     | ±26        |

Planted fault (one copy of s1 with `unmatched` +1 on 2.1.198):
`eval bands` → exit 2, `!! MATCHING NOT EQUAL: fnUnmatched on
2.1.197->2.1.198: …r1=1397 …r2=1397 perturbed-s1=1398`, nothing written;
`eval leaderboard r1 perturbed` → `!! MATCHING NOT EQUAL at the same commit:
perturbed-s1 fnUnmatched 4750 vs ref-names-40921b9f-r1 4749`.

## Gate

On `41caeca7c` (rebased onto main `61063385e`, the 40921b9f names bands):

| check                         | worktree | clean checkout |
| ----------------------------- | -------- | -------------- |
| shims                         | PASS     | PASS           |
| no-private-addresses          | PASS     | PASS           |
| rust:fmt                      | PASS     | PASS           |
| rust:clippy                   | PASS     | PASS           |
| census:clones (advisory)      | REVIEW   | REVIEW         |
| census:concurrency            | PASS     | PASS           |
| census:slop (advisory)        | PASS     | PASS           |
| rust:unit                     | PASS     | PASS           |
| rust:build                    | PASS     | PASS           |
| format-golden                 | PASS     | PASS           |
| e2e                           | PASS     | PASS           |

census:clones: the first run flagged this lane's test helpers (two
`add_matching` copies, `add_llm_diagnostics`, two bands-test setups);
unified into `eval/test_support.rs` and a `bands_over` helper. What remains
predates this branch. The first worktree run failed rust:unit only because
the fresh worktree had no `node_modules` (webcrack shim); after `npm ci
--ignore-scripts` it passed.

## What this could NOT prove

- Seven repeats at two commits is the whole evidence for "exact". A future
  change that makes matching legitimately depend on the base's names would
  trip the refusal; the fix then is to move that column out of `EXACT`
  with evidence, not to loosen the check.
- `fnAsked` is derived (`llm + failed`), not a recorded "asked" count; it
  equals total − carried − already-named − nothing-to-rename only while
  `libraryPrefix` and `fallback` are 0 (they are in every run read).

## Proposed CLAUDE.md text

Add after the "When a KPI moves, read the LLM call diagnostics" paragraph:

> **Read the MATCHING block before the KPIs (exp208, 2026-11-10).** `eval
> summarize` and `eval leaderboard` print how many functions the pipeline
> carried, close-matched, left unmatched or ambiguous, and sent to the LLM,
> and how many module bindings and identifiers the LLM named (read from each
> scored run's stats). Five of them — functions total, close-matched, sent
> to the LLM, unmatched, still ambiguous — are decided before any model
> answer and must be EQUAL between same-commit repeats: `eval bands`
> refuses to write when one differs, and a same-commit leaderboard prints
> `!! MATCHING NOT EQUAL`. That is a harness or determinism bug, not the
> LLM — find it before reading any KPI. The other counts move with the
> draws; `eval bands` records their spread as `matchingBands` beside the
> KPI bands, and a same-commit move past it is flagged.
