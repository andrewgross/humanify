# 088 — BRIEF: recover names the diff itself proves, whatever the hunk shape

> Origin: Andrew, 2026-08-20 — "for those cases we should have been able to
> identify that we didn't need the LLM to pick names. Look at the actual
> diffs, trace what we did for those statements, understand why it didn't
> work."

## Two traced cases (post-exp086 trees, /work/exp086-walk 215→216)

**Case A — `userChatMessage → currentTimestamp` (10 lines,
`is-message-relevant/is-message-relevant.js`).** Upstream wrapped a yield in
braces inside a 20-line `for (let userChatMessage of processChatMessages(...))`
statement. Chain of failure:

1. FORWARD: close-match transfer aligns whole STATEMENTS on rename-invariant
   content. The braces flip the 20-line statement's hash, alignment misses
   wholesale, and every local declared inside is re-asked with no pinned
   hint. The model names a chat-message loop variable `currentTimestamp`.
2. BACKWARD: the same braces make the diff hunk unbalanced (20 vs 22 lines),
   hunk classification calls it GENUINE, and its **18 lines that differ only
   in the identifier** — eighteen witnesses all voting `userChatMessage` —
   are discarded before candidate discovery. Replayed with today's code:
   the binding is never a candidate (not any skip reason — invisible).

**Case B — `tokenRecord → tokenInfo` (9 lines,
`handle-abort-or-timeout/handle-abort-or-timeout.js`).** The function was
re-added within the same file (pure add + delete hunks). No paired lines
exist in the normal diff at all, so the restore pass sees nothing — but the
masked-identical lines across the add/delete pair still testify.

## The general failure, named

Both directions gate on the wrong granularity:

- transfer: STATEMENT-atomic (one internal edit poisons a 20-line statement);
- reconcile candidates: HUNK-atomic (two brace lines poison 18 witnesses;
  add/delete shape yields zero pairs; >10-line hunks excluded wholesale;
  contiguous witnesses count as ONE consumer-tier witness).

The witness evidence itself — lines identical after masking identifiers —
survives every one of those shapes.

## Sizing (measured on the residual, filters stated)

Non-alias, non-path-derived rename pairs on the busy hop: 860 pairs,
1,405 line-pairs (~2,810 git lines). By witness count:

| witnesses (masked-identical line pairs) | line-pairs | share |
| --------------------------------------- | ---------: | ----: |
| 3+                                      |        444 |   32% |
| 2                                       |        380 |   27% |
| 1                                       |        581 |   41% |

**Ceiling for a ≥2-witness recovery: ~824 line-pairs (~1,650 git lines).**
Contamination to expect (and let the existing gates kill): minified↔minified
re-rolls (`A → w`), minified→descriptive improvements that must NOT revert
(`j → isSidechainFlag`) — reconcile's name-quality ladder already refuses
both classes.

## Ideas to test, in order

1. **Skeleton-vote candidate discovery (backward; build first).** Pair
   changed lines by unique identifier-blanked skeleton across the WHOLE file
   diff — including inside genuine/oversized hunks and across add/delete
   pairs — and admit a binding as a candidate when ≥2 paired lines agree on
   one old→new mapping. EVERY existing gate stays: name quality, target
   liveness, import-decl skip, eval taint, same-name-siblings, validated
   rename, pure-rename invariant. Offline-provable with the replay harness
   before any walk (deterministic given the trees).
2. **Sub-statement hint mining (forward).** When a close-matched function's
   statement fails to align, mine masked-identical lines inside the changed
   statement for name votes and decorate the prompt with explicit
   per-binding "previously named X" suggestions (today the prior name is at
   best ambient context). Fixes the ask, not just the answer; needs a cold
   walk to validate (prompt changes shift draws).
3. (Subsumed) within-file function-move pairing — idea 1's skeleton pairing
   across add/delete hunks covers case B.

## Validation

- Idea 1: offline replay first (exact); then unit red/green; `npm run
check`; cold walk with `novel`/`realLines` exact and nameOnlyLines down by
  the surviving-candidate count; hand review 20 restores.
- Pre-registered expectation to fill in after the offline replay, before the
  walk.

## Shipped increment (2026-08-20) + pre-registered walk predictions

Three repairs (all TDD-pinned): whitespace-blind pairing, property-aware
skeletons, and the skeleton-vote pool (unique OR equal-count-unanimous
shapes, any hunk geometry) with testimony counted in distinct skeletons and
a >=3-witness exemption from the from-not-novel gate.

Offline census (deterministic, post-exp086 residual): **144 line-pairs
(~290 git lines) restore**, incl. both traced cases. The raw >=2-witness
ceiling (824) shrank 6x at the gates — the census's multiset pairing counts
witnesses the skeleton rules rightly reject as ambiguous; recorded so the
next reader doesn't re-derive the gap.

Walk predictions (baseline = exp087's walk, since exp088 stacks on it):

- busy nameOnlyLines down ~250-350 (draw band ±90); `novel`/`realLines`
  EXACT; calm hop inside the 32-line spread; boot gates pass.
- hand-review 20 applied restores from the walk tree before merge.

Headroom left on the table (sized, deliberate):

- consumer-to-name-live 112 lp — restoring while the prior name is live
  elsewhere; riskier, revisit with evidence.
- occurrence-outside-diff 164 lp — mechanism not yet understood; census
  first before touching.
- not-a-candidate 660 lp — largely census-artifact witnesses (multiset
  pairing counts what skeleton uniqueness rightly refuses).

## WALK VERDICT (2026-08-21, /work/exp088-walk, shared with the exp087 fix)

| gate                                                                        | result                                                   |
| --------------------------------------------------------------------------- | -------------------------------------------------------- |
| 4 hops complete, cards written (walk aborts on boot failure → boots passed) | PASS                                                     |
| `novel`/`realLines` exact                                                   | **PASS — 986/122,066 busy, 146/33,135 calm**             |
| busy nameOnlyLines                                                          | 4,458 → **4,314** (−144; predicted −250..−350, band ±90) |
| busy churnExBuild                                                           | 24,653 → 24,534 (−119)                                   |
| calm hop                                                                    | 247 exBuild / 20 nameOnly — best readings on record      |

Honest note: the census point estimate (−288 git lines) delivered ~half.
Candidate explanations, unproven: the bundle-level pass runs before the
per-file one and shifts what the census's isolated replay sees; restored
lines that still carry OTHER renames stay in nameOnly; draw variance. The
traced cases are gone from the residual and the mechanisms are unit-pinned.
Magnitude sits at the prediction band's edge — recorded, not rounded up.
