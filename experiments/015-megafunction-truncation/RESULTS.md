# Exp015 results — megafunction truncation coverage (2026-07-08)

Branch `exp015-megafunction-coverage`. Two A/B rounds: run 1 validated
the truncation/context fixes and **falsified the brief's causal model
for the headline noise families**; the autopsy found the real mechanism
(same-named sibling-block bindings) and run 2 tests its fix.

## Changes under test

Run 1 (commits through `979941d`):

1. **Declaration-anchored code windows** (`src/rename/code-window.ts`):
   oversized function code (>500 generated lines) is no longer
   flat-truncated — each batch identifier gets a window around its
   declaration line (input locs map 1:1 onto generated lines, verified
   for all 137 oversized functions; runtime-guarded fallback to flat
   truncation). Header + closing line always shown; windows merge with
   elision markers; padding shrinks to the 500-line budget. Retry
   snippets extract from the windowed text.
2. **Whole-identifier regex** (`identifierRegex`): `extractRetrySnippet`
   used `\b<id>\b`, which cannot match `$`-names and false-matches
   inside longer ones — `$`-family retries pulled wrong/empty snippets.
3. **Context caps**: close-match prior code capped at 500 lines (was:
   whole prior function, up to 3,512 lines); module-binding declarator
   text capped at 10 lines. Round 3 had 54+63 batches 400-failing at
   the model's 32K context because of these.
4. **Diag fix**: shadowed-pass reports merge instead of overwriting —
   round-3 diags silently dropped main-pass outcomes for any function
   with a shadowed second pass (the brief's "477 in unrenamed.missing"
   was this artifact).

Run 2 adds (commits `bb2efdc`, `e390d56`):

5. **Module used-names cap** (200): the module prompt joined EVERY
   non-eligible module-scope name; late-run batches carried thousands
   of applied names and still 400-failed (one batch at 45K tokens in
   run 1's fresh leg).
6. **Uniquify same-named sibling bindings**: the actual fix for the
   headline families — see "The real mechanism" below.

## Run 1 — windows + caps

| metric               | round-3 baseline | run 1  | delta     |
| -------------------- | ---------------- | ------ | --------- |
| noise hunks          | 6,206            | 6,298  | +92       |
| noise share of hunks | 76.2%            | 76.6%  | +0.4pp    |
| genuine-change hunks | 1,936            | 1,922  | ~stable ✓ |
| rename occurrences   | 11,596           | 11,891 | +295      |
| distinct bindings    | 3,479            | 3,553  | +74       |

Buckets: transfer-gap 7,283→7,632, asymmetric 1,414→1,343,
reroll 2,899→2,916. Inside oversized functions: 2,494→2,479
occurrences — **the truncation families did not move**
(`v6→X6` 114, `$_→w_` 111, `vH→T_` 104, `J_→W_` 96 — identical counts).

Run health (all tripwires green):

- Fresh leg 13m31s / **46.5M tokens (round 3: 49.4M, −6%)** while
  LLM-naming 124,756 identifiers vs 103,767 (+20%; the identifier
  total jump 106,866→131,860 is the diag fix making counts honest).
- Incremental leg **4.7M tokens (round 3: 5.9M, −20%)**, and
  **0 context-length failures (round 3: 63)**. Fresh leg: 1 module
  batch still failed (the used-names bomb, fixed for run 2).
- Both outputs parse; structural invariant clean; genuine ~1,922.

So the windows/caps delivered coverage + cost wins but **not the
families** — proof the brief's "truncation → families" attribution was
wrong.

## The real mechanism: same-named sibling-block bindings

The families' bindings were RENAMED per the diag (`$_ →
messageSessionIdentifierVal` in fn `516027:5`) yet survived in the
output. The diff hunks resolve it: `let $_ = …` appears at lines
516097, 516140, 516167 and `catch ($_)` at 516159 — **multiple distinct
bindings sharing one minified name across sibling block scopes**. Bun
reuses tiny names aggressively inside big functions.

The batch protocol keys identifiers by NAME, so collection dedups:
`collectOwnedBindingInfos` takes the first `$_`, the shadowed second
pass recovers at most one more (its per-name `Map` collapses the rest).
N−2 of every group stay minified in BOTH legs → Bun rerolls the token
between builds → the reroll families. They concentrate in
megafunctions (more sibling blocks), which made truncation look
causal.

Sizing (`count-duplicate-names.ts`, v119): **7,017 duplicate bindings
across 2,082 functions; 3,018 beyond the reach of both passes.** The
worst offenders are exactly the families: `521806:2122` `v6`×36 (the
114-occ family), `516027:5` `$_`×34 (111 occ), `264855:2` `s`×12,
`294647:12` `m`×15.

Fix (run 2): the shadowed pass mechanically uniquifies duplicates
first — the k-th binding of a name group becomes `<name>_<k>` via
`attemptValidatedRename` (AST order → version-stable suffixes,
recorded as decisions) — then every binding is individually
LLM-nameable with its own declaration-anchored window. If the LLM
misses one, both legs still agree on the mechanical name.

## Run 2 — + uniquify + module used-names cap

| metric               | round-3 baseline | run 1  | run 2 |
| -------------------- | ---------------- | ------ | ----- |
| noise hunks          | 6,206            | 6,298  | TBD   |
| noise share of hunks | 76.2%            | 76.6%  | TBD   |
| genuine-change hunks | 1,936            | 1,922  | TBD   |
| rename occurrences   | 11,596           | 11,891 | TBD   |

Buckets and megafunction attribution: TBD.

## What moved, what didn't, next steps

TBD after run 2.
