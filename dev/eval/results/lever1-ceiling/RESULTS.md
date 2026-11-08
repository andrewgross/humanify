# Lever 1 ceiling — unique statement-hash twins (measured 2026-07-21)

**Question.** Of the `noise` statements (rename-invariant `statementHash` in both
versions, text differs — structurally unchanged code the pipeline re-named), how
many have a **unique** prior hash-twin (hash count 1 on both sides), so names
could transfer positionally with zero ambiguity and no LLM?

**Method.** `ceiling-lever1.ts`, run directly on the archive outputs
(`~/Development/unpacked-claude-code/versions/*/.humanify/humanified.js`) for the
4 eval pairs — same classification as `analyze.ts` (totals reproduce the
committed `archive-shipped` noise exactly: 5,004 stmts / 110,447 lines).

## Result

| pair    | noise st/ln    | unique st/ln  | uniq-multiline | uniq-sameHead | eqCount n:n | unequal      | uniq% st/ln |
| ------- | -------------- | ------------- | -------------- | ------------- | ----------- | ------------ | ----------- |
| 85→86   | 2,079 / 47,526 | 602 / 39,357  | 596 / 39,351   | 584 / 38,580  | 388 / 3,267 | 1,089/4,902  | 29 / 82.8   |
| 118→119 | 763 / 15,810   | 101 / 12,849  | 99 / 12,847    | 96 / 12,838   | 88 / 1,443  | 574 / 1,518  | 13.2 / 81.3 |
| 197→198 | 1,428 / 27,095 | 256 / 22,530  | 254 / 22,528   | 246 / 22,295  | 172 / 1,224 | 1,000/3,341  | 17.9 / 83.2 |
| 215→216 | 734 / 20,016   | 154 / 16,616  | 153 / 16,615   | 148 / 16,458  | 139 / 1,769 | 441 / 1,631  | 21 / 83     |
| TOTAL   | 5,004 /110,447 | 1,113 /91,352 | 1,102 / 91,341 | 1,074 /90,171 | 787 / 7,703 | 3,104/11,392 | 22.2 / 82.7 |

## Reading

1. **The ceiling is 82.7% of noise LINES** (91,352 of 110,447) — carried by only
   22.2% of noise statements. Noise line-mass concentrates in big multi-line
   statements, and those are almost always uniquely twinned (rich structure ⇒
   unique hash).
2. **99% of the unique-twin line mass is multi-line** (91,341 lines) — a
   multi-line structure colliding by hash chance is implausible, so false-twin
   risk on the recoverable mass is concentrated in the 11 single-line twins
   (~11 lines). The `bindingRolesAgree` gate still applies to all.
3. **96.5% of unique twins already have a byte-identical head line**
   (1,074/1,113; 90,171 lines): the top-level declared name is already inherited
   — the churn is **internal locals** re-rolled by the LLM (the close-match
   path / ~20k-line floor). ⇒ The build must transfer bindings **positionally
   through the whole statement subtree** (lockstep walk of the two same-hash
   ASTs), not just the statement's own declared names. Only ~39 twins have a
   flipped head.
4. The other buckets are small in line terms: equal-count n:n (7,703 ln) would
   need a positional-unanimity rule — not worth the risk now; unequal-count
   (11,392 ln) is ambiguous — abstain by design.

## Go/no-go: **GO**

A deterministic, no-LLM tier that (a) finds unique statement-hash twins,
(b) corroborates with `bindingRolesAgree`, (c) pairs bindings positionally via
the lockstep structural walk, and (d) pins those names before the LLM pass,
has a measured upper bound of **~91k noise lines across the 4 pairs (~83% of
all naming noise)**. Even at half-capture after the precision gate it dwarfs
every previous lever.

Caveats: measured archive-vs-archive (shipped noise, 110k ln); `baseline-main`
fresh noise is 93,970 ln — same structure, slightly smaller pool. The ceiling
counts pre-gate; `bindingRolesAgree` corroboration will prune some transfers
(precision over recall — intended).
