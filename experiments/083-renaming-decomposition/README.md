# 083 — the renaming residual decomposed: one lever dead, one split in three

> **STATUS 2026-08-20: measurement only, no code.** Sizes the two levers the
> 081 handover left open for the 5,132 name-only lines on the busy hop
> (2.1.215→216). Every census below states what its filter wrongly includes
> before quoting the number (measurement-pitfalls discipline).

## Lever 1 — "make the model's answer reproducible": DEAD as a diff lever

Two independent cold runs of the SAME hop (same input, same prior tree —
`/work/exp080-letwalk/2.1.216` vs `/work/exp082-moves/2.1.216`) differ by
**7,424 lines across 227 files, 91% of it provably name-only (6,790)**. So
within one release the model is close to a fresh draw per run — consistent
with exp052's 33.4%.

But reproducibility cannot transfer across releases, because **the questions
do not recur**. Parsing every `suggestAllNames` prompt out of the two -vv walk
logs: 2.1.215 asked 94 distinct questions, 2.1.216 asked 3,719, and only
**16 are byte-identical across the two releases** (15 answered the same,
1 differently). What gets asked on a hop is precisely the code that changed,
and its question text did not exist last release. Temperature-0/seeded
sampling would make RERUNS agree (useful for measurement noise, nothing
else); it would not remove release-to-release churn.

(No contradiction with the 35-line walk noise band: the band measures
reproducibility of the churn TOTALS, not of tree bytes.)

## Lever 2 — "keep the prior name for changed code": three distinct buckets

Extracted every (old → new) identifier substitution from the busy hop's
provably name-only lines (masked-identical line pairs, positional token
alignment): **2,637 line-pairs ≈ 5,274 git lines, 1,187 distinct name pairs**
— agreeing with the scorecard's `nameOnlyLines` 5,132.

Filter caveats: lines that changed structurally AND renamed are missed
(charged to real, by design); multi-rename lines pair correctly per token.

| bucket                                        | line-pairs | share | example                                                             | owner                                          |
| --------------------------------------------- | ---------: | ----: | ------------------------------------------------------------------- | ---------------------------------------------- |
| alias-form flip (same stem, `src`/`-2` state) |        213 |    8% | `srcStripAnsi → stripAnsi2`                                         | module identity (part fixed by exp082 veto)    |
| derived alias churn, not an LLM answer        |        877 |   33% | `srcSanitizeHtmlVal → parseCommandFlagsAnalyzeFeatures`             | module placement: upstream regrouping + chains |
| LLM answered; prior name WAS in the prompt    |        703 |   27% | `hookEvent → heartbeatCleanup`, `errorMessage → errorMessageString` | naming: hint obedience / contention            |
| LLM answered; prior name NOT in the prompt    |        844 |   32% | `tokenData → tokenRecord`, `proposalParseResult → parsedProposal`   | naming: correspondence never reached the ask   |

How the buckets were attributed: the run's own `-vv` log carries every prompt
and parsed answer; a new name is looked up among the answers, and the old
name is searched in the producing prompt's text.

**Stated false positives:** "prior name WAS in the prompt" counts ANY
occurrence of the old name in the prompt — including as a DIFFERENT binding's
displayed name (prompts render already-transferred names inside the code
body) and as neighbor-line context. Sampled: the `hookEvent →
heartbeatCleanup` case is exactly that — the prompt displays `hookEvent` as
another binding's name, i.e. a name-CONTENTION case (exp061's cap), not a
disobeyed suggestion. **The 27% is an upper bound on true deviations.**
"NOT in the prompt" has the opposite bias: it is the clean bucket.

## What this means for building anything

1. **41% of the "renaming" headline is not a naming problem at all** (buckets
   1+2). It is import aliases following module identity events: the exp082
   chains (fixed), upstream bundler regrouping (the exp070 relayout lever),
   and `src`-prefix/`-2` suffix state flips.
2. The genuinely-naming half splits into **hint-obedience/contention**
   (≤1,400 git lines, upper bound) and **missing correspondence**
   (~1,700 git lines) — different machinery, and the second is the cleaner
   target: the pipeline never surfaced a prior name it demonstrably had
   (the old name sits on a masked-identical line in the prior tree).
3. The risk posture question from the handover ("stale name on repurposed
   code") applies only to those naming buckets; the derived 41% carries no
   such risk — it is placement work.

## Where the data lives

- rename pairs: `experiments/083-renaming-decomposition/` scripts were run
  from the session scratchpad; the census commands are reproducible:
  two-run diff (`diff -r`), `080/name-churn-where.py` between the two 216
  runs, prompt census over `<tree>.log` (parse `suggestAllNames - SUCCESS`
  blocks, key by exact user-prompt text).
- The `--diagnostics` file of the 2.1.216 run (`/work/exp082-moves/`) holds
  `strategyTrails` (201,720 entries), `nameContention`, `transferStats` —
  populated and useful. Its `placementTrails` is EMPTY (see exp082 README:
  the trail is not wired for fossil trees).
