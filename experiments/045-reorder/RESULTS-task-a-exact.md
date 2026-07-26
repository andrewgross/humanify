# 045 Task A (exact) — reorder is ~80% irreducible, and my lower bound was wrong

Brief: [README.md](README.md). Estimate this replaces:
[RESULTS-task-a.md](RESULTS-task-a.md).

**Status: measurement complete. Reorder's recoverable share is 1,176 git lines
of 6,078 — 19.3%, not the "~2,800, a lower bound" the estimate claimed. The
correction goes the OPPOSITE way from the one predicted.**

## The correction, stated plainly

Task A approximated a load-order barrier as "any top-level
`ExpressionStatement`" and reasoned that this OVER-counts constraint, because
the real model computes `effects` per statement and an effect-free expression
statement is movable. It therefore published ~2,800 recoverable lines as a
**lower** bound.

Running the shipped model (`bundleLoadOrderFacts`, the same facts the emitter's
aligner consults) shows the proxy did the reverse: it UNDER-counted. A variable
declaration whose initializer calls something impure is a barrier too, and the
syntax proxy saw only bare expression statements.

| hop     | reorder | constrained (proxy) | **constrained (exact)** | **recoverable** |
| ------- | ------: | ------------------: | ----------------------: | --------------: |
| 85→86   |   1,918 |               32.7% |               **58.5%** |      **796 ln** |
| 118→119 |     258 |                   — |               **95.3%** |       **12 ln** |
| 197→198 |   1,950 |               53.9% |               **91.3%** |      **170 ln** |
| 215→216 |   1,952 |               75.8% |               **89.9%** |      **198 ln** |
| TOTAL   |   6,078 |                     |               **80.7%** |    **1,176 ln** |

Three of four hops are ~90%+ constrained. The recoverable remainder is 180
statements across all four.

**The lesson is the one this series keeps relearning in new costumes:** a
syntactic proxy for a semantic property does not merely lose precision, it can
be biased in a direction you did not predict. Task A reasoned carefully about
which way its approximation erred and got the sign wrong. The fix cost one
import of the real model.

## Where the whole project stands

With all three axes now measured to their floors:

| axis       | charged in the diff | genuinely reducible | status                        |
| ---------- | ------------------: | ------------------: | ----------------------------- |
| naming     |               7,616 |              ~2,600 | rest is exp036 rotation       |
| reorder    |               6,078 |          **~1,176** | rest is load-order constraint |
| relocation |               1,390 |                ~294 | solved (−91.1%)               |
| alias      |                 200 |                 566 | fix attempted, FAILED         |

**Roughly 4,600 reducible lines remain in a 154,668-line reviewed diff — about
3%.** The pipeline is far closer to its floor than the headline "21,656
reducible" figure from exp042's brief suggested; most of that figure has since
been shown to be either fixed (relocation) or structurally irreducible (rotation,
load-order).

## What that implies for the next lever

Nothing here is worth a large build:

- **reorder** — 1,176 lines over 180 statements, and the constrained share RISES
  with base size (58% → 90% → 95%), so it shrinks as the codebase grows. Read
  the 122 recoverable statements on 85→86 before anything else; they are 68% of
  the whole axis's remaining value.
- **naming** — ~2,600 lines, diffuse, no mechanism identified after exp044 ruled
  out both correspondence and aliases.
- The honest recommendation is that further noise work is now a
  diminishing-returns judgement rather than an engineering question. The
  measurements to make that judgement are in place; the judgement is the user's.
