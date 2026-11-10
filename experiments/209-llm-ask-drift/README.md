# exp209 — why the number of things sent to the LLM drifts between same-commit runs

STATUS: DONE (investigation), 2026-11-10, branch `exp209-llm-ask-drift`.
No code change. No determinism bug found. Every drifting count traces to one
of two LEGITIMATE causes: (a) the scratch-rebuilt prior names differently in
each run, or (b) the model's answer in THIS leg (a name kept unchanged). Retries
and timing change nothing that is asked. Four reporting quirks in the coverage
block make the counts look like "sent to the LLM" when they are not; they
are listed under "Reporting quirks" (proposals, not fixes).

Andrew, 2026-11-10: across four same-commit cold runs of `40921b9f`
(`ref-names-40921b9f-r1/r2/r3`, parallel lanes; `serial-40921b9f-s1`) the
matched-function count is identical but the count of things sent to the LLM
moves slightly. He assumed those counts are deterministic.

## Answer in one table

The asked SET depends only on the input and the prior (proven below). In scratch
base mode the prior (`v-1`) is itself a cold LLM rebuild in every run, so
its names differ. A different prior name changes which bindings the transfer
tiers can carry, and so which are left to ask.

| drifting count (scored leg) | what it really counts | cause of the drift | verdict |
| --- | --- | --- | --- |
| `coverage.functions.llm` (2.1.181 917/917/916/917; 2.1.198 1386/1385/1386/1386) | functions where the LLM renamed at least one binding (NOT functions asked: asked = `llm + failed` = 918 / 1387 in every run) | one single-binding function where the model returned the minified name unchanged (2.1.181 r3: the function at input line 635766, its one binding kept `A`; 2.1.198 r2: line 288029, kept `r`). `functions.failed` is the residual "asked, nothing renamed", not a transport failure | legitimate (model answer) |
| `coverage.moduleBindings.llm` (e.g. 2.1.181 177/179/174/175) | module bindings asked AND renamed | the asked count is `total - cached`; `cached` (carried from the prior by binding-cascade, module-vote, module-pin) moves with the prior's names. Examples below | legitimate (prior names) |
| `coverage.moduleBindings.total` (2.1.86 s1 14430 vs 14431) | `cached + asked`, NOT every module binding | in s1 one module binding was named by the statement-twin tier, which counts as neither cached nor asked, so it drops out of the "total" | legitimate drift; reporting quirk 2 |
| `coverage.identifiers.total` (e.g. 2.1.86 3001/3012/3014/3010) | bindings put in an LLM request (function bindings + module bindings + shadowed block bindings) | inside the SAME asked functions, a different number of bindings is already carried from the prior (close-match pairs, closure-capture, retry). `skippedBySkipList` moves the other way. Same-run cold rebases (no prior) have a CONSTANT asked total in all four runs | legitimate (prior names) |
| `coverage.identifiers.llm` / `.notRenamed` | asked and renamed / asked and kept | the total above, plus how many names the model kept unchanged | legitimate (prior + model) |
| `transferStats.exactMatch.attempted` (±1-3) | transfer pairs with prior name != fresh minified name | a pair whose prior name EQUALS the fresh minified name is not attempted; the prior's leftover letters differ per run | legitimate (prior names) |
| `transferStats.exactMatch.applied` / `closeMatch.applied` (±1-3) | pairs applied | a prior name that collides in scope (`shadows-child`, `target-in-scope`) is rejected and goes to retry, or a different name lands. Examples below | legitimate (prior names) |
| `coverage.llm.totalCalls`, lane counts | requests | follows from the above, plus truncated answers re-asked | legitimate |

## The proof: the asked set is a pure function of (input, prior)

Six runs of the frozen `40921b9f` release binary on the 2.1.86
bundle against the ONE stub LLM (no model), with the eval's exact config and
each real run's own scratch-rebuilt prior (`<run>/2.1.85-rebased`).
Columns: functions asked, module-binding total, module bindings asked,
module bindings cached, identifiers asked, `skippedBySkipList`.

| run | prior from | stub | real scored leg | match |
| --- | --- | --- | --- | --- |
| 1 | r1 | 631 / 14431 / 238 / 14193 / 3001 / 7782 | 631 / 14431 / 238 / 14193 / 3001 / 7782 | exact |
| 2 | r1 (repeat) | same as run 1 | same | exact |
| 3 | s1 | 631 / 14430 / 238 / 14192 / 3010 / 7776 | 631 / 14430 / 238 / 14192 / 3010 / 7776 | exact |
| 4 | s1 (repeat) | same as run 3 | same | exact |
| 5 | r2 | 631 / 14431 / 238 / 14193 / 3012 / 7772 | 631 / 14431 / 238 / 14193 / 3012 / 7772 | exact |
| 6 | r3 | 631 / 14431 / 238 / 14193 / 3014 / 7770 | 631 / 14431 / 238 / 14193 / 3014 / 7770 | exact |

Also: `transferStats` of stub run 1 equals the real r1 leg's exactly (and run
3's equals s1's); the per-binding transfer settles are identical between the
two repeats, and the two r1-prior stub trees are byte-identical. The stub
answers EVERY name differently from the real model and its server timing is
nothing like the real one, yet the asked counts land on the real run's
numbers. So neither the model's answers in that leg, nor thread or
completion order, nor retries reach what is asked. Each stub run: 115-120 s
wall time, 9.4-9.6 GB peak RSS.

Same check from the other side, in the data the runs already wrote: in the
five cold REBASES (no prior) the asked totals (`identifiers.llm +
notRenamed`, `moduleBindings.llm + notRenamed`, `functions.llm + failed`) are
equal in all four runs. Only how many asks got renamed moves.

## Concrete examples (2.1.179 -> 2.1.181, r1 vs r2)

Per-binding settle tiers come from the diagnostics file's `strategyTrails`
(keyed by input location + minified name, which is stable across runs).
86 settle differences across the six run pairs on this hop. In 85 of them the
prior name proposed for that binding differed. The remaining one also turns
on a prior-side name (see closure-capture below). Typical r1-vs-r2
differences:

- **module-pin vs ask.** Five no-op module bindings (input lines 78-88k). A
  single exact vote pins the binding only if the voted prior name still has a
  prior role (`prior_binding_roles`, keyed by NAME, drops a name already
  consumed by a cascade rename). r1's prior offered unique names, so the
  binding was pinned. r2's prior offered generic ones (`onClose`, `listener`,
  `onData`) that another prior binding had already consumed, so the result was
  `no-prior-role` and the binding was asked. And vice versa for others.
- **binding-cascade rejected -> module-vote.** Three module bindings (lines 876,
  97956, 133937): r2's prior name was a short generic word already taken in
  scope (`target-in-scope`), so the cascade was rejected and module-vote
  applied it. r1's prior name was longer and landed directly.
- **exact-match rejected (`shadows-child`) -> retry.** Bindings at lines
  214887, 291036, 342848, 433072, 137899-900: where one run's prior named the
  binding with a single letter (`i`, `x`, `y`) that a child scope also uses,
  the exact transfer was rejected and the retry tier applied it. Where the other
  run's prior had a descriptive name, exact-match applied it.
- **closure-capture vs ask.** Line 641904 and similar: the captured local gets the
  top vote only when it is unique and the owner function does not already hold
  that name from an earlier tier. Both depend on the names that this run's
  prior gave to the close-matched function and its statement twins (twin votes
  carry no trail, so the vote lists in the trail can look identical while the
  tally differs).
- **shadowed-binding suffixes.** Bindings like `$r` at lines 641-642k appear as
  `$r_3` in one run and `$r_4` in another. The gate release counts shadowed
  block bindings by their CURRENT name, and how many still carry the
  minified name depends on how many the prior already renamed. This changes
  only the label, not whether the binding is asked.
- **Statement-twin (2.1.86, s1).** Module binding at line 59249: r1's prior name
  for the twin was already taken in scope (rejected, cascade rejected, asked).
  s1's prior gave a different name and the twin applied it, so s1 asked one
  binding fewer here and pinned one more elsewhere.

Example of a model-only difference (2.1.86, r1 vs r3): the function at input
line 421816 was asked with the SAME 14 bindings in both runs. In r1 the model
returned seven of them (`f`, `D`, `j`, `M`, `J`, `P`, `z`) unchanged, and they
remain minified in r1's output (they are in r1's `mintedCensus`). In r3 the
model named them. Asked set: equal. Outcome: different.

## What was ruled out

- **Retries / timing.** Every scored leg: 0 failed transport calls. Retries
  happened in some legs (2.1.181 r2: 27 connection retries; r3: 9; 2.1.119 r2:
  6; 2.1.216 r3: 9), and every one succeeded on resend. A resend is the same
  request, so it cannot change what is asked. The stub runs, with no retries
  at all, reproduce the counts.
- **Thread / completion order, hash iteration.** Ruled out for the asked set
  by the stub repeats (identical trails and byte-identical trees) and by the
  stub-equals-real match under completely different response timing.
- **Model answers in the same leg changing later asks.** The cold rebases
  answer differently in each run but ask the same totals. The stub (different
  answers again) asks the real leg's totals.

## Reporting quirks (proposals, not fixed here)

These are why "sent to the LLM" looked non-deterministic. All four are naming
or accounting in `crates/humanify-ag-core/src/naming/report/coverage.rs`:

1. `coverage.*.llm` counts things the LLM RENAMED, not things SENT. Asked is
   `llm + notRenamed` (identifiers, module bindings) or `llm + failed`
   (functions). A reader comparing runs should compare the asked sum.
   Proposal: add an explicit `asked` field per row.
2. `functions.failed` is a residual ("asked, nothing renamed", including a
   model that kept every name), not a transport failure. Proposal: rename it
   (`askedNoneRenamed`) or split out true failures.
3. `moduleBindings.total` is `cached + asked`. A module binding settled by
   statement-twin, retry or module-naming is in neither, so the "total"
   moves with the prior. Proposal: report the real module-binding universe.
4. `identifiers.skippedBySkipList` mostly counts bindings whose current name is
   in the function's transferred set (already carried), not the reserved-name
   skip list. Proposal: split the two. Note that the transferred-set check is
   by NAME (`select_llm_bindings`): an uncarried binding whose minified name
   happens to equal a carried name would also be skipped. This is NOT measured
   here and is not a drift cause I could find, but worth a test if someone
   touches that code.

## What this could NOT prove

- The stub proof covers one hop (2.1.85 -> 2.1.86), with all four of its priors.
  For the other four hops, the per-binding trail diffs and the constant rebase
  totals are evidence, not a replay.
- It says nothing about whether a scratch base SHOULD differ between runs. It
  does, because the rebase is a cold LLM run. A deterministic asked count
  across runs would need a pinned base (seeded mode, or a shared rebased tree).

## Data

- Runs: `/work/ref-names-40921b9f-frozen/dev/eval/results/{ref-names-40921b9f-r1,r2,r3,serial-40921b9f-s1}/`;
  diagnostics files `/work/eval-work/<run>/<V>.diag.json`.
- Stub runs: `/work/exp209-stub/<n>/` (`run.sh`, `compare.py`, `measure.txt`).
