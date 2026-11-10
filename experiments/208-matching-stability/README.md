# exp208 — matching counts as a stability check

STATUS: DONE, NOT MERGED (2026-11-10), branch `exp208-matching-stability`.
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
  | `fnToLLM`     | `coverage.functions.llm + .failed` (sent to the LLM, named or not) | EXACT |
  | `fnLLMNamed`  | `coverage.functions.llm`                         | banded |
  | `fnUnmatched` | `resolutionStats.unmatched`                      | EXACT  |
  | `fnAmbiguous` | `resolutionStats.stillAmbiguous`                 | EXACT  |
  | `mbCarried`   | `coverage.moduleBindings.cached`                 | banded |
  | `mbLLMNamed`  | `coverage.moduleBindings.llm`                    | banded |
  | `idTotal`     | `coverage.identifiers.total` (in LLM prompts)    | banded |
  | `idLLMNamed`  | `coverage.identifiers.llm`                       | banded |

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
- `fnToLLM` is `llm + failed`, not `llm`: `functions.llm` moved by 1 on
  2.1.181 and 2.1.198, exactly offset by `functions.failed` (a function sent
  to the LLM that came back with no rename). The sum never moved.
- `fnCarried` is BANDED although equal across all four 40921b9f repeats:
  at 111401fe it read 46239 / 46237 / 46239 on 2.1.181 (offset exactly by
  `alreadyNamed`). On scratch bases the prior is itself a cold LLM run, so
  whether a matched function counts as "carried" or "already named" can
  follow the base's draws. Making it exact would have refused a valid
  reference.
- Module bindings and identifiers move by a few to ~25 on every commit
  (why: exp209). Banded.

## What the block prints (40921b9f, totals over five pairs)

```
=== MATCHING (totals across the compared pairs) ===
-- FUNCTIONS
                      fnTotal fnCarried fnClose fnToLLM fnLLMNamed fnUnmatched fnAmbiguous
ref-names-40921b9f-r1  254797    220762    3595    5928       5923        4749        4756
ref-names-40921b9f-r2  254797    220762    3595    5928  5922 (-1)        4749        4756
ref-names-40921b9f-r3  254797    220762    3595    5928  5922 (-1)        4749        4756
serial-40921b9f-s1     254797    220762    3595    5928       5923        4749        4756
-- MODULE BINDINGS
                       mbCarried mbLLMNamed
ref-names-40921b9f-r1      95920       1717
ref-names-40921b9f-r2 95923 (+3)  1714 (-3)
ref-names-40921b9f-r3 95922 (+2)  1715 (-2)
serial-40921b9f-s1    95919 (-1)  1715 (-2)
-- IDENTIFIERS
                          idTotal  idLLMNamed
ref-names-40921b9f-r1       26090       25929
ref-names-40921b9f-r2 26105 (+15)  25928 (-1)
ref-names-40921b9f-r3  26095 (+5) 25918 (-11)
serial-40921b9f-s1     26083 (-7) 25941 (+12)
```

Per pair (r1 summarize):

```
                 fnTotal fnCarried fnClose fnToLLM fnLLMNamed fnUnmatched fnAmbiguous
2.1.85->2.1.86     35872     30700     670     631        630         917         931
2.1.118->2.1.119   42981     35645     404    1895       1894         473         890
2.1.179->2.1.181   53295     46826     890     918        917        1146         920
2.1.197->2.1.198   58156     50459     942    1387       1386        1397        1150
2.1.215->2.1.216   64493     57132     689    1097       1096         816         865
```

`eval bands` dry run (scratch copies; the worktree's bands files restored):

| repeats              | exact counts | fnCarried | fnLLMNamed | mbCarried | mbLLMNamed | idTotal | idLLMNamed |
| -------------------- | ------------ | --------- | ---------- | --------- | ---------- | ------- | ---------- |
| 40921b9f r1-r3 + s1  | all equal    | ±0        | ±1         | ±4        | ±3         | ±22     | ±23        |
| 111401fe r1-r3       | all equal    | ±2        | ±1         | ±5        | ±6         | ±26     | ±26        |

Planted fault (one copy of s1 with `unmatched` +1 on 2.1.198):
`eval bands` → exit 2, `!! MATCHING NOT EQUAL: fnUnmatched on
2.1.197->2.1.198: …r1=1397 …r2=1397 perturbed-s1=1398`, nothing written;
`eval leaderboard r1 perturbed` → `!! MATCHING NOT EQUAL at the same commit:
perturbed-s1 fnUnmatched 4750 vs ref-names-40921b9f-r1 4749`.

## Gate

See the final report / the commit; filled in after `cargo dev check`.

## What this could NOT prove

- Seven repeats at two commits is the whole evidence for "exact". A future
  change that makes matching legitimately depend on the base's names would
  trip the refusal; the fix then is to move that column out of `EXACT`
  with evidence, not to loosen the check.
- `fnToLLM` is derived (`llm + failed`), not a recorded "asked" count; it
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
