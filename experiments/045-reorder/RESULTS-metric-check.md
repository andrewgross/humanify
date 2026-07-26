# 045 — is the reorder metric sound? Yes, with one outlier

Brief: [README.md](README.md). Exact ceiling:
[RESULTS-task-a-exact.md](RESULTS-task-a-exact.md).

**Status: measurement complete. The reorder KPI is accurate — it matches git to
the line on three of four hops. One file over-charges by 83x and is worth 332 of
the axis's 6,078 lines. An earlier inference that the KPI was systematically
inflated was WRONG and is retracted here.**

## The suspicion, and why it was reasonable

`diff-composition` scores reorder with a statement-level LCS and charges a moved
statement `lines * 2`. Git diffs LINES and minimises the line edit script. When
one 166-line block and two 1-line statements swap relative order, the statement
LCS can declare the big block moved while git prints the two small ones.

That is exactly what `table/skill-docs/files-api.js` does across 2.1.85→86:

    metric charge   332 git lines   (the 166-line `filesApiDocs` doc blob "moved" 2 slots)
    real diff         4 git lines   (two 1-line `noop` statements swapped)

An 83x over-charge, and reorder was the file's ONLY change, so nothing else
explains the gap.

## Generalising from it was wrong

The first scale-up compared the metric's charge against the real diff over the
worst reordered files and appeared to show wild disagreement in both directions
(197→198: charged 1,442, real 3,986). That comparison is meaningless — those
files also carry naming and genuine code change, so most of their real diff is
not reorder at all.

The clean population is files whose ONLY difference is order: identical
statement multisets, nothing added, removed or edited, so the entire real diff
IS the reordering.

| hop     | pure-reorder files | metric charges | git prints |    ratio |
| ------- | -----------------: | -------------: | ---------: | -------: |
| 85→86   |                 10 |            442 |        112 |     3.9x |
| 118→119 |                  2 |              4 |          4 | **1.0x** |
| 197→198 |                  5 |             24 |         24 | **1.0x** |
| 215→216 |                 10 |            270 |        270 | **1.0x** |

**Exact agreement on three hops of four**, and the 85→86 ratio is produced
entirely by `files-api.js` — the other nine files on that hop agree to the line
(72/72, 12/14, 10/10, …).

So the metric is sound. `files-api.js` is a pathological case, not a systemic
artifact: 332 lines, 5.5% of the axis.

## Correction, on the record

Three tool calls before this one, this experiment inferred from a single file
that "a substantial part of the reorder KPI is a measurement artifact". That
inference was drawn from n=1, the attempt to confirm it at scale was confounded,
and the clean measurement refutes it. It is retracted.

That is the **third** time in this line of work that a plausible inference was
confirmed by one reading of the data and refuted by a better one — after exp044's
alias sizing (38% → 7.2%) and its correspondence probe ("7 of 8 followed another"
→ permutation). The pattern is consistent enough to state as a rule: **an
explanation derived from the largest single example is a hypothesis about that
example, not about the population, until the population is measured.**

## Net effect on the ceiling

Reorder's recoverable share was measured at 1,176 git lines. Removing the
`files-api.js` over-charge leaves roughly **844 genuinely recoverable lines**
across four hops — the smallest of the three axes by a wide margin:

| axis       | charged | genuinely reducible |
| ---------- | ------: | ------------------: |
| naming     |   7,616 |              ~2,600 |
| reorder    |   6,078 |            **~844** |
| relocation |   1,390 |                ~294 |

Total remaining reducible noise is roughly **3,700 lines in a 154,668-line
reviewed diff — about 2.4%.**
