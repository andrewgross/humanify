# 052 — how much naming noise is hidden because the LLM happened to agree

> ## STATUS: MEASURED. No code shipped; this is an instrument plus a number.
>
> **Headline: of the naming decisions the LLM makes on a hop, 64.4% coincide
> with the other cold run's choice for the same binding.** Only the remaining
> 35.6% ever reach the diff. The naming residual is therefore roughly a THIRD of
> the naming instability the pipeline is exposed to.
>
> **One of this directory's two instruments FAILED and is kept as a warning.** > `silent-agreement.ts` keys on a NAME SPELLING and is contaminated beyond use —
> it reported 99.6% agreement because spellings the LLM proposed somewhere
> (`error`, `options`, `length`, `cache`) also sit on thousands of mechanically
> pinned locals. Rule 3, again. Use `reroll-rate.ts`, which keys on the input
> bundle's own `(functionId, minified name)` and has no such hole.

## The question

The diff shows only the names the LLM got **wrong**. Every binding the pipeline
cannot pin mechanically is a draw, and when the draw lands on the name the
previous release used, no line is printed. Those wins are real, unearned, and
uncounted. How many are there?

## How much of a hop is the LLM deciding at all

From each run's `identifierLedger` (cold, four gate hops, `/work/exp050-cold`):

| hop       |    bindings | mechanically settled |        LLM | LLM share |
| --------- | ----------: | -------------------: | ---------: | --------: |
| 85→86     |     110,001 |              106,677 |      3,128 |     2.84% |
| 118→119   |     132,602 |              124,379 |      8,575 |     6.47% |
| 197→198   |     179,852 |              174,155 |      6,008 |     3.34% |
| 215→216   |     200,425 |              195,699 |      4,948 |     2.47% |
| **total** | **622,880** |          **600,910** | **22,659** | **3.64%** |

## The re-roll rate

`reroll-rate.ts` compares the two COLD legs of the exp050 A/B — same input
version, both legs cold — keyed by `(functionId, original minified name)`, which
is a property of the input bundle and so identifies the same binding in both.

| hop       | decided by LLM in both legs |          same name |         different |
| --------- | --------------------------: | -----------------: | ----------------: |
| 85→86     |                       3,122 |              2,090 |     1,032 (33.1%) |
| 118→119   |                       8,551 |              5,176 |     3,375 (39.5%) |
| 197→198   |                       5,970 |              4,027 |     1,943 (32.5%) |
| 215→216   |                       4,913 |              3,236 |     1,677 (34.1%) |
| **total** |                  **22,556** | **14,529 (64.4%)** | **8,027 (35.6%)** |

Of the disagreements, **33.4 points are a different word** and only 2.2 are a
decoration flip — these are not near-misses:

```
osTypeVal        / platformName          fileList          / files
contextInfo      / refusalContext        processedSchema   / cleanedSchema
lastCommandNode  / finalCommandOutcome   isInsert          / preserveTrailingNewline
```

Same shape as the LOCAL-DRIFT class exp051 measured in the real diff
(`retrieveAnthropicApiKey → getAnthropicApiKeyOrNull`, `statusElement →
component`), which is the corroboration that this is the same phenomenon.

## What that implies for the residual

exp051 attributed **2,920 git lines over four hops to LOCAL-DRIFT** — the
visible tail of LLM re-decisions. If that tail is the 35.6% that disagreed, the
same decisions carry roughly

```
2,920 x (64.4 / 35.6) = ~5,300 git lines of noise that did not print
```

so the naming residual the harness scores is about a third of the exposure:
**~8,200 lines' worth of decisions, of which ~2,900 happened to show.** Treat
this as an order of magnitude, not a measurement — it assumes an agreeing
decision would have cost the same lines as a disagreeing one, and the line cost
per binding varies with how often the name is used.

Consistency check: this is ~1,300 lines a hop, the same order as the **±2,800
git-line per-hop draw band** exp048 measured directly. They are the same
phenomenon seen from two sides, which is the reason
`docs/measurement-pitfalls.md` rules 10 and 11 exist.

## Caveats, stated

- The two legs' priors are not byte-identical (each rebased its own), so part of
  the disagreement is different prompt context rather than temperature. The rate
  is an **upper bound** on pure draw variance — though its stability across four
  hops of very different prior similarity (32.5%–39.5%) argues the draw dominates.
- `silent-agreement.ts` is retained ONLY as a documented failure. Its loose
  reading (99.6% agreement) and its strict reading (71.6%–99.1%) are both
  artifacts of keying on a spelling rather than an occurrence.
