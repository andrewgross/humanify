# 055 Task 0 — RESULTS (executed 2026-08-11)

> **STATUS: MEASURED — decision threshold EXCEEDED.** Task 0 ran on two
> independent cold runs (labels `noise-band-r1` / `noise-band-r2`, both at
> commit `76c012b`, cache +0 on every pair). The brief's prediction of
> "at least 4,576" did **not** survive — it double-counted: exp054 shipped
> before this measurement ran, so the 4,576 lines it removed are already
> gone from these trees. What remains is below the prediction but above the
> decision threshold. The brief's decision rule stands as written.

## The number

Name-only lines hiding inside the REAL column of the `src/` tree diff
(`real-ledger.ts`, statement pairs billed by `diff-composition`, lines
paired set-based exactly as the tally bills them):

| hop     |     run 1 |     run 2 | as % of REAL (r1) |
| ------- | --------: | --------: | ----------------: |
| 85→86   |     1,622 |     1,662 |              6.6% |
| 118→119 |       260 |       284 |              0.7% |
| 197→198 |     1,010 |       986 |              2.1% |
| 215→216 |       556 |       570 |              2.3% |
| **all** | **3,448** | **3,502** |                 — |

**3,448 > 2,000 → per the decision rule fixed in the brief, the hidden
name churn is a live target and the classified-noise buckets are no longer
the map.** 85→86 carries nearly half of it, consistent with that hop's
larger upstream change surface.

The two-run agreement (Δ54 total, ≤40 per hop) means the metric is nearly
draw-invariant: the LLM picks different words each run, but the _amount_
of name-only churn stays fixed. Any future lever that moves this number by
more than ~50 lines moved it for real.

## What the predicate tests (rule 3, one sentence)

"These two billed lines tokenize to the same token stream with every
non-identifier token byte-identical and at least one identifier differing"
— it does NOT test that the difference is a rename; a call rerouted to a
different helper reads identically.

## Rule 1 sample (Task 1, first 25 of 85→86 read by hand)

Overwhelmingly naming noise: mint-ordinal drift
(`initializeEnvironment12` → `initializeEnvironment9`), parameter renames
carried through their uses (`documentTitle` → `agentName` ×4 in
`agent-saver.js`), local-binding drift (`sshModule` → `fsUtils`).
Ambiguous minority: member-expression pairs where both the module alias
AND the property changed (`systemBlockCharacters.initializeShades` →
`commandRunner.setupPlatformUtilities`) — indistinguishable from a
rerouted call at line granularity; these are naming drift of the callee
module's export in the likely case but are not proven so here.

## Caveats that bound the claim

1. **3,448 is a LOWER bound (rule 8).** 56–86% of REAL is one-sided
   add/remove — statements the classifier could not pair with a prior
   statement at all. A statement renamed heavily enough falls out of
   pairing entirely and its name churn lands there, uncounted. Nothing in
   this instrument looks inside one-sided mass.
2. **The decomposition disagrees with GNU diff by −24%..+17% per hop**
   (85→86 −24.3%, 197→198 +17.0%), confirming exp055's earlier finding.
   Per-file attribution has p90 relative error of 20–67%. The aggregate
   NAME-ONLY number is bounded-good (two-run stable); any per-file reading
   of this ledger is not trustworthy.
3. Set-based line pairing is positional — a mis-pair moves a line between
   NAME-ONLY and EDITED but never changes their sum, so NAME-ONLY could
   shift either way at the margin.

## What this decides

The next noise experiment targets name churn inside hash-flipped
statements (and should first size how much MORE hides in the one-sided
mass, per caveat 1 — that sub-measurement is also pipeline-free). The
standing residual table built from classified-noise buckets understates
naming noise by at least 3,448/7,598 ≈ 45% of its own total.
