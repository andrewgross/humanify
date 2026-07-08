# Exp015 baseline — sizing the truncation population (2026-07-08)

Measured on main @ exp014 round-3 (`f7d82d0`-era pipeline), inputs prepared
exactly as the pipeline prepares them (bun unpack + babel beautify, forced
`--bundler bun --minifier bun` — plain detection returns passthrough for
the decompiled entrypoints and silently skips factory extraction).

## Population (truncation-coverage.ts)

| leg  | functions | oversized (>500 gen lines) | ≥1 invisible | eligible bindings | INVISIBLE past cap |
| ---- | --------- | -------------------------- | ------------ | ----------------- | ------------------ |
| v119 | 42,980    | 68                         | 37           | 3,304             | 574 (17.4%)        |
| v120 | 43,197    | 69                         | 37           | 3,315             | 576 (17.4%)        |

(An earlier draft measured 653/648 across 44 functions — inflated by a
`\b`-boundary regex that cannot match `$`-names; `identifierRegex` in
`src/utils/identifier-regex.ts` fixes the measurement AND a production
bug in `extractRetrySnippet`, which used the same pattern.)

The wrapper IIFE is excluded (module-binding path, no truncation). The
"known five" undercounted ~7x: 37 functions carry invisible bindings,
led by `input.js:492821:2` (3,512 lines, 190 invisible of 401),
`524905:2122` (CLI action, 1,822 lines, 81 invisible), `471148:2`
(1,941 lines, 80 invisible).

Shape probe: 18 of the 44 contain a single top-level statement >400
lines (up to 1,860) — segmentation by top-level statements alone cannot
reach bindings declared inside those; the chosen design (declaration-
anchored line windows) is nesting-agnostic. `generate(fn.node)` line
count == input loc span for ALL 137 oversized functions across both
legs, so binding locs map exactly onto generated-code lines.

## Where the invisible bindings ended up (round-3 diag join)

- v119 fresh leg: 172 of 653 were blind-renamed by the LLM
  (nondeterministic → asymmetric/transfer-gap noise); 481 have NO diag
  outcome at all — the brief's "477 in unrenamed.missing" was actually
  this: `processFunction` runs the shadowed-binding second pass through
  `processFunctionBatched`, which OVERWRITES `fn.renameReport`, so the
  main pass's outcomes vanish from diagnostics for any function with
  shadowed bindings. Diag under-reports; fix on this branch.
- v120 incremental leg: 26 of 27 `unrenamed.missing` are in oversized
  functions; 12 are in the invisible sets.

## Noise attribution (attribute-to-megafunctions.py on round-3 diff)

| metric                   | total  | inside oversized fns |
| ------------------------ | ------ | -------------------- |
| noise hunks              | 6,206  | 1,197 (19.3%)        |
| rename occurrences       | 11,596 | 2,494 (21.5%)        |
| transfer-gap occurrences | 7,283  | 895 (12.3%)          |
| asymmetric occurrences   | 1,414  | 389 (27.5%)          |
| minifier-reroll occ.     | 2,899  | 1,210 (41.7%)        |

Top truncation families confirmed inside oversized ranges:
`v6→X6` (114, CLI action), `$_→w_` (111) + `J_→W_` (96) in
`516027:5`(v119 id), `vH→T_` (104) in `513052:4`(v119 id), `s→o` (57)
in `264855:2`(v119 id).

So the megafunction-addressable ceiling is ~2,494 occurrences / ~1,197
hunks — above the brief's 700–1,000 estimate. Not all of it is
truncation-caused (some close-match naming instability lives inside
these ranges too), but the reroll share (1,210) plus the blind-naming
share is directly addressable by coverage.

## Reproduce

```bash
NODE_OPTIONS=--max-old-space-size=16384 npx tsx \
  experiments/015-megafunction-truncation/truncation-coverage.ts \
  <bundle-or-prepared.js> [--prepared] [--save-prepared p] [--diag d] [--json j]
python3 experiments/015-megafunction-truncation/attribute-to-megafunctions.py \
  <runtime-diff.txt> <truncation.json> [top_n]
```
