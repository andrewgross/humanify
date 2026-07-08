# Exp015 results — megafunction truncation coverage (2026-07-08)

_DRAFT — A/B in flight, numbers land when the run completes._

## What changed (branch `exp015-megafunction-coverage`)

1. **Declaration-anchored code windows** (`src/rename/code-window.ts`,
   wired in `buildRequest`): oversized function code (>500 generated
   lines) is no longer flat-truncated at line 500 — each batch
   identifier contributes a window around its declaration line (input
   locs map 1:1 onto generated lines; runtime-guarded with the old flat
   truncation as fallback). Header + closing line always included;
   windows merge with elision markers; padding shrinks to the budget.
   Retry snippets extract from the windowed text.
2. **Whole-identifier regex** (`src/utils/identifier-regex.ts`):
   `extractRetrySnippet` used `\b<id>\b`, which never matches `$`-names
   (`= $H +`) and false-matches inside `a$H` — retries for the Bun
   `$`-families pulled wrong/empty snippets.
3. **Context caps**: close-match prior code (was: full prior function,
   up to 3,512 lines) capped at the 500-line budget; module-binding
   declarator text capped at 10 lines. Round-3 logs showed 54 + 63
   batches 400-failing on the 32K model context because of these — the
   failures concentrated exactly in the megafunction noise families.
4. **Diag fix**: the shadowed-binding second pass no longer overwrites
   `fn.renameReport` (it merged), so per-function outcomes are complete.

## Offline verification (before burning the A/B)

- `simulate-windows.ts` (production selection code, real inputs):
  invisible bindings 574/576 → **0** on both legs; mean shown lines per
  batch 143–144 vs 500 flat; no real fallbacks (the "1" was the bundle's
  own `[truncated]` string literals).
- 932 unit tests + 33 fingerprint snapshots green; `npm run check` clean.

## A/B (fresh v119 + incremental v120, protocol of exp014 round 3)

| metric               | round-3 baseline | this run | delta |
| -------------------- | ---------------- | -------- | ----- |
| diff lines           | 51,148           | TBD      |       |
| noise hunks          | 6,206            | TBD      |       |
| noise share of hunks | 76.2%            | TBD      |       |
| genuine-change hunks | 1,936            | TBD      |       |
| rename occurrences   | 11,596           | TBD      |       |
| distinct bindings    | 3,479            | TBD      |       |

Buckets (occurrences):

| bucket          | round 3       | this run | delta |
| --------------- | ------------- | -------- | ----- |
| transfer-gap    | 7,283 (62.8%) | TBD      |       |
| asymmetric      | 1,414 (12.2%) | TBD      |       |
| minifier-reroll | 2,899 (25.0%) | TBD      |       |

Megafunction attribution (attribute-to-megafunctions.py):

| metric             | round 3       | this run |
| ------------------ | ------------- | -------- |
| noise occ. inside  | 2,494 (21.5%) | TBD      |
| reroll occ. inside | 1,210 (41.7%) | TBD      |

Run health: TBD (parse, structural invariant, internalErrors,
context-length failures, unrenamed.missing, tokens).

## What moved, what didn't, next steps

TBD
