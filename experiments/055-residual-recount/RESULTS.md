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

## Task 0b — the one-sided mass, sized (caveat 1 executed same day)

`one-sided-ledger.ts` re-pairs the one-sided REAL statements by
identifier-masked shape. **The first version of this probe was zero by
construction and the plant check caught it**: a pure same-file rename can
NEVER land one-sided, because the statement hash is already rename-blind
and step 2 books it as NAMING. What CAN land one-sided while still being
name churn is a statement that **moved files while renaming** (hash
matching is per-file). Matching masked shapes across files:

| hop     | one-sided REAL | masked-twin |  anchored |      bare |
| ------- | -------------: | ----------: | --------: | --------: |
| 85→86   |         13,404 |       3,006 |     1,792 |     1,214 |
| 118→119 |         23,542 |         288 |       122 |       166 |
| 197→198 |         22,317 |       1,028 |       536 |       492 |
| 215→216 |         11,198 |         508 |       240 |       268 |
| **all** |     **70,461** |   **4,830** | **2,690** | **2,140** |

Every match is cross-file (0 same-file, as the construction predicts).
Two-run stable (r2 total 4,696, Δ134). Composition matters:

- **anchored (2,690)** — the masked shape retains a string literal,
  overwhelmingly `const § = §("../../exact/path.js")`: the same require
  moved to a different file with a drifted local alias. Near-certain
  identity; this is relocation+rename churn charged to REAL.
- **bare (2,140)** — no literal anchor; multi-line lazy-initializer
  prologues (the exp051 permutation shape) plus junk like `var §, §;`
  that can pair spuriously. Upper bound only, precision unknown.

## What this decides

Defensible hidden name churn inside REAL: **≈ 6,138 lines** (3,448
paired name-only plus 2,690 anchored cross-file), with up to ~2,140 more
unproven — against 7,598 in the entire classified-noise table. The
buckets understate naming noise by ~80% of their own total (lower bound).
The next noise experiment targets name churn inside hash-flipped and
relocated statements; the residual table alone is not the map.

Remaining unsized: name churn inside one-sided statements that ALSO
changed shape (no predicate exists — needs a hand-read per rule 1), and
string-keyed names (export-key drift breaks the anchor match by design).
