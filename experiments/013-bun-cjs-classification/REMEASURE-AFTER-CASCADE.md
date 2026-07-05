# Binding-match remeasure after the cascade + identity-stage fixes

Date: 2026-07-04, branch `fix/transfer-validation`.

`measure-binding-match.ts` runs the matching phase only (no LLM) on the
real v119/v120 `runtime.js` pair (post-unpack, minified on both sides —
match **counts** are the metric; the transferred names are minified and
meaningless).

```
node --max-old-space-size=16384 --import tsx/esm \
  experiments/013-bun-cjs-classification/measure-binding-match.ts
```

## Results

| Metric                                  | Run B baseline (old code)               | After fixes                     |
| --------------------------------------- | --------------------------------------- | ------------------------------- |
| Direct binding matches (no propagation) | 4,047 (23%)                             | **13,991 (80.7%)²**             |
| — unique-unique / cascade               | 2,056                                   | (cascade, incl. identity stage) |
| — fn-var-name transfers                 | 1,991                                   | (included)                      |
| Binding total (v120)                    | 17,976¹                                 | 17,342¹                         |
| Functions matched (exact + close)       | 99.4%                                   | 37,086 + 5,884 = 99.5%          |
| matchPriorVersion wall-clock            | (hours, dominated by findPriorFnByCode) | **~40-50s**                     |
| Peak RSS (matching phase)               | OOM at 8GB heap, needed 16GB            | **~4-6GB**                      |

¹ Binding totals differ slightly: the old count came from the full
humanify run's diagnostics; this measurement uses the raw unified graph.
² 13,938 from the cascade + identity stage; +53 more after adding
module-to-function edges for bare function references
(`var alias = someFn`). Identity rounds over binding-neighbor evidence
(alias chains) added no further matches on this pair.

## What this means

- Direct binding matching went from 23% to **80.4% before vote
  propagation even runs**. In Run B, propagation lifted 23% direct to
  74% total; starting from 80.4% direct, total binding coverage should
  approach the function rate (~99%), which is what the runtime.js diff
  noise reduction needs.
- The matching phase that previously consumed most of Run B's 15.5
  hours now completes in ~40 seconds (findPriorFnByCode removal +
  prior-side Bun factory classification + cached placeholder mappings).
- Peak memory during matching is well under the old 8GB heap that
  OOM'd, because the prior side no longer fingerprints and retains
  guaranteed-unmatchable factory internals.

Next: a full humanify run with `--prior-version` (Phase 2 of exp013) to
measure end-to-end binding cache rate and the actual diff size against
the 167,944-line / 30,745-hunk baseline.
