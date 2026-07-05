# Phase 2 results — end-to-end v119→v120 after the fixes

Fresh humanify of v119 (Run A′, 3h04m) → humanify v120 with
`--prior-version` (Run B′, 3h58m). Both on the real 13.7MB decompiled
bundles, local gpt-oss-20b.

## Correctness: PASS

- **Both outputs parse cleanly, zero post-hoc patches.** The original
  Run B needed manual edits just to parse (`delete` keyword, duplicate
  `let NH`); Run B′ produced valid runnable output directly.
- Validation caught real hazards in production: exactMatch transfers
  rejected 27 (`target-visible` capture ×26, `target-in-scope` ×1),
  closeMatch rejected 38 (`target-visible`) — renames that would have
  changed behavior, now skipped and left for the LLM.
- v120 module-binding cache: **82.3%** (14,358/17,442), up from 74%.
- CPU: Run A′ used 27m user CPU over 3h04m wall; Run B′ 15.5m over
  3h58m — genuinely LLM-bound, not the pegged-CPU behavior.

## Diff-noise reduction: MISS (and the inherited analysis was wrong)

|                        | Baseline (old Run A/B) | Run A′/B′ (new code)     |
| ---------------------- | ---------------------- | ------------------------ |
| diff lines             | 167,944                | 159,713                  |
| hunks                  | 30,745                 | 28,833                   |
| pure 1↔1 rename noise | 66.7%                  | **63.7%** (18,379 hunks) |

~5% smaller, not the predicted ~10x. **The module-binding cache lift
worked exactly as measured (74%→82%, ~1,600 fewer drifting bindings)
but barely moved the diff, because module-binding rename noise was never
the dominant term.** MODULE-BINDING-CACHE-MISS-ANALYSIS.md's "66.7% is
binding noise → cascade gives 10x" conflated two things: it counted all
1-line identifier-rename hunks as "binding noise", but most are not
matchable bindings.

### Where the 18,379 noise hunks actually come from

Classified by the drifting identifier:

1. **Function body-local drift — the biggest term.** 17,445 functions
   (40.4%) matched as _close_ (not exact), so only name+params transfer
   and the **body is re-named by the LLM fresh**; 6,807 more got pure
   fresh LLM names. Across ~24K functions the LLM names body locals
   differently between versions: `cache→renderCache` (578 hunks),
   `error→caughtError`, `serializeWithDisposable→computeDiff` (352).
   Exact-matched functions (24,463) transfer ALL names via placeholder
   mapping and do NOT drift — the problem is specifically the
   close-match + pure-LLM population.

2. **Fresh-LLM'd module bindings, amplified by use sites.** The 3,084
   uncached bindings (17.7%) each drift across their declaration **plus
   every assignment and reference** — 13,959 of the noise hunks are
   assignment/use lines, ~3,800 are declarations. One drifting binding
   ≈ 4-5 hunks. Includes init-less forward decls (`var mapGet;`→
   `var get;`) that have no structural hash and are unmatchable by the
   cascade by construction.

3. **Irreducible minifier churn** on genuinely-unmatched functions
   (`G→R`, `R→G`, `QH→dH`) — real cross-version change, not noise.

## The real levers (evidenced, not predicted)

The next win is **not** more module-binding work. In priority order:

1. **The 40% close-match rate is the headline problem and is
   suspiciously high** (standalone matching on raw v119↔v120 produced
   ~14% close; the full run against _humanified_ v119 produced 40%).
   Two independent angles, both in `docs/code-review-2026-07-04.md`:
   - Tighten **exact** matching so exact-eligible functions stop
     falling into close-match (review C4 singleton-bucket blind match,
     C5 cascade fallback widening, C7 non-injective matches). Every
     function moved close→exact transfers its whole body and stops
     drifting.
   - For functions that are genuinely close, transfer body-local names
     where structure still aligns, or hard-constrain the LLM to reuse
     the prior name (today `priorVersionContext` is passed but the model
     doesn't reliably honor it) — review C6.
2. **Investigate the close-match count jump** (5,884 standalone vs
   17,445 full-run). Same v120, different prior (raw-minified vs
   humanified v119). If humanifying the prior degrades exact matching,
   that is a self-inflicted cache miss worth a targeted fix.
3. Init-less binding forward-decls: match via first-assignment RHS
   identity (the fingerprint already has a `firstAssignmentRHS` path;
   verify it's wired through the cascade).

## Bottom line

The branch delivered its **correctness** goal — the pipeline now
produces parseable, runnable, validated output on a real bundle, which
it could not before. The **diff-size** goal is unmet, but the run
converted a wrong inherited hypothesis into an evidenced one: the noise
floor is function-body-local LLM drift on close-matched functions, not
module-binding rename churn. That is where the next phase should aim.
