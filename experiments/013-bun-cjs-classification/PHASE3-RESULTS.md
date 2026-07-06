# Phase 3 results — v119→v120 after rename-invariant hashing + correctness fixes

Date: 2026-07-06. Runs A″/B″ on main @ 0067b92 (binding-keyed placeholder
hashing, duplicate-declaration rename fix, free-name capture invariant,
semantic output gate), pinned worktree, same inputs and LLM
(gpt-oss-20b) as Phase 2. Artifacts: /tmp/exp013-phase3/.

## Headline

|                              | original (Run A/B) | Phase 2 (A′/B′) | **Phase 3 (A″/B″)**  |
| ---------------------------- | ------------------ | --------------- | -------------------- |
| diff lines                   | 167,944            | 159,713         | **131,437 (−17.7%)** |
| hunks                        | 30,745             | 28,833          | **22,983 (−20.3%)**  |
| pure 1↔1 rename hunks¹      | —                  | 26,372          | **20,181 (−23.5%)**  |
| pure-rename lines¹           | —                  | 82,678          | **65,170 (−21.2%)**  |
| wall clock (incremental run) | —                  | 3h58m           | 2h59m                |

¹ Same classifier on both diffs (equal-count change hunks whose lines are
identical after identifier normalization) — the Phase 2 doc's "63.7%"
used a narrower 1-line-hunk definition and is not comparable.

Correctness: both outputs parse; **zero parse failures and zero semantic
gate failures** on the real 20MB bundle — the new free-name-set and
binding-count invariants held through a full production run.

## Coverage: the hashing fix delivered what the A/B predicted

| Run B″ (v120 with prior)             | Phase 2 | Phase 3                   |
| ------------------------------------ | ------- | ------------------------- |
| functions exact-transferred (cached) | 24,463  | **33,912**                |
| functions close-matched              | 17,445  | **7,576**                 |
| functions pure-fresh LLM             | 6,807   | **1,148**                 |
| module bindings cached               | 82.3%   | **89.7%** (15,617/17,413) |
| LLM-touched functions                | ~24,252 | **~8,724 (−64%)**         |

Run A″ (fresh) also sped up: 1h55m vs 3h04m (hashing rewrite removed
per-function deep clones; matching phase faster).

## Where the remaining 20,181 rename-noise hunks live

Attribution by population (approximate):

1. **Close-matched function bodies** — 7,576 functions transfer only
   name+params; body locals are re-LLM'd and drift. The next
   highest-value fix: transfer body-local names where statements align,
   and make the LLM actually reuse names from the prior code it is
   shown.
2. **Fresh-LLM'd module bindings** — 1,793 bindings × ~4–5 hunks each
   (declaration + every use site) ≈ 7–9K hunks. Includes init-less
   forward declarations (`var x;`) that are unmatchable by construction
   until first-assignment-RHS matching is wired through.
3. **Genuinely new/changed code** named nondeterministically — the
   floor until naming is deterministic (temperature, reasoning effort).

"Other" hunks (structural, not 1↔1): 2,461 → 2,802. This bucket
contains the true v119→v120 changeset plus line-rewrap artifacts (a
longer name re-wraps a line, breaking 1↔1 shape). The changeset view
work should diff at the function level to make this bucket the ONLY
thing a reader sees.

## Timing finding: the retry tail now dominates incremental runs

B″'s mainline LLM phase (everything the prior didn't cover) ran at
~70 in-flight and finished in well under an hour. The remaining ~2
hours were the **collision-retry tail**: ~950 functions resolving name
conflicts through small serial retries (2–3 identifiers each) that
re-send large used-names context — ~4M input tokens spent on that tail
alone, at 2–4 requests in flight. With transfers eliminating most
mainline work, retry-storm reduction (batch the retries, trim re-sent
context, cap per-function retries harder) is now the #1 wall-clock
lever for the incremental workflow, alongside the reasoning-effort
change in docs/llm-server-tuning-brief.md (>90% of completion tokens
are the gpt-oss reasoning channel).

## Next levers, in expected-value order

1. Changeset renderer + hermetic diff-noise regression test (the
   product goal; matcher already computes everything needed).
2. Close-match body-local transfer + prior-name prompt adherence
   (attacks the largest remaining noise population).
3. Retry-storm reduction + reasoning effort low (wall clock 3h → likely
   well under 1h for incremental runs).
4. Matcher precision hardening: injectivity, no fallback-widening,
   singleton-bucket corroboration (196 cascade-residual functions), and
   binding-identity votes for propagation.
5. Forward-declaration binding matching via first-assignment RHS.

## Reproduction

```bash
PHASE2_OUT=/tmp/exp013-phase3 bash experiments/013-bun-cjs-classification/run-phase2.sh
# noise classifier: see PHASE3-RESULTS analysis snippet in git history
# (equal-count c-hunks, lines equal after \b[A-Za-z_$][A-Za-z0-9_$]*\b → '#')
```
