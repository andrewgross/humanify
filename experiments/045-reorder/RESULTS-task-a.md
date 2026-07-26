# 045 Task A — half the reorder axis is the constraint working

Brief: [README.md](README.md).

**Status: Task A complete, no build. The brief's headline ceiling (~1,950 git
lines per hop) is wrong: 33–76% of it is statements that CANNOT legally move
back. Recoverable is roughly 2,800 of 6,078 lines, not 6,078.**

## The recurring file, read

`floor/server-status/otel-exports.js` is the worst file on two separate hops
(274 ln on 85→86, 502 ln on 197→198). On 197→198 a ~120-line block genuinely
moves within the file — `initOTLPTraceExporter` sits at prior line 95 and fresh
line 586, a ~490-line shift, with the text byte-identical.

It could not have stayed. The block contains **five load-order barriers**:

    (0, resourceLifecycle.defineModuleExports)(traceExporterModule, {
        OTLPTraceExporter: () => OTLPTracesExporter
      });

Those are top-level expression statements — things that can observably act while
the module loads. exp038's model forbids moving anything across one (the
boot-crash rule that exp037 learned the hard way), and there is another barrier
between the block's old and new positions. The aligner behaved correctly; the
churn is upstream's reordering that our constraint is not allowed to undo.

**The single biggest item in the axis is not a defect.**

## Ceiling, corrected

| hop     | reorder | barrier statements | blocked by a barrier | **constrained** | recoverable |
| ------- | ------: | -----------------: | -------------------: | --------------: | ----------: |
| 85→86   |   1,918 |         8 / 122 ln |          20 / 506 ln |       **32.7%** |      ~1,291 |
| 197→198 |   1,950 |          7 / 66 ln |          36 / 986 ln |       **53.9%** |        ~899 |
| 215→216 |   1,952 |        18 / 342 ln |        73 / 1,138 ln |       **75.8%** |        ~472 |

Roughly **2,800 recoverable lines across the four hops**, against 6,078 charged.
That puts reorder level with naming's tractable residue (~2,600) rather than
twice it — though reorder still has the better shape, since two thirds of its
churn sits in ten files per hop.

Note the trend: the constrained share RISES with base size (33% → 54% → 76%).
Bigger releases have more load-time effects per file, so the axis gets harder as
the codebase grows, not easier.

## Caveat on this number, stated rather than buried

The barrier test here is coarse: every top-level `ExpressionStatement` is
treated as a barrier. exp038's real model (`src/split/load-order.ts`) is finer —
it computes `effects` per statement, and an expression statement with no
observable effect is movable. So this over-counts constraint, which means
**2,800 is a LOWER bound on what is recoverable** and the true figure is
somewhere above it.

Getting the exact number means running `bundleLoadOrderFacts` over the emitted
trees rather than approximating from syntax, which is the first thing a Task B
should do — and it is a measurement, not a build.

## What this does to the ranking

Nothing dramatic, and that is the point:

| axis       | charged |      genuinely tractable | shape                          |
| ---------- | ------: | -----------------------: | ------------------------------ |
| naming     |   7,616 |                   ~2,600 | diffuse                        |
| reorder    |   6,078 | **~2,800** (lower bound) | **concentrated: 10 files/hop** |
| relocation |   1,390 |                       ~0 | solved                         |

Reorder remains the better-shaped of the two remaining axes, but it is not the
2× advantage the brief claimed before this measurement. Both are now in the same
range, and both are far below what relocation was worth when exp041 began.
