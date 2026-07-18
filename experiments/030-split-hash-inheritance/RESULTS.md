# 030 — Hash-keyed split inheritance

## Problem

Walk measurement (2026-07-18, RUN 3 artifacts): on hops where upstream
reorders the bundle (2.1.85→86 preserved only 65% of statement order; ~1 in 9
hops), byte-identical statements — _with identical humanify names_ — land in
different files. Cause: `assignWithPrior` inherits by name votes, and every
abstaining statement (no declared names, unequal counts, or LLM-flipped
names) follows its **preceding neighbor** — position-based, so a reorder
scatters whole runs across folders (66% of moves crossed top-level dirs).

## Fix

`statementHash` (src/split/statement-hash.ts): rename-invariant structural
hash per wrapper-body statement — node types, shape, literals, operators,
declaration kinds; ALL identifier names masked (humanify renames bindings and
export-member names). Ledger carries `hashes[]` + `hashVersion`, and
`assignWithPrior` gets a tier ABOVE name votes: inherit the prior file when
the hash's occurrence count matches across releases AND every prior
occurrence lived in one file. Anything ambiguous abstains to name votes.
Old ledgers without hashes: tier off, behavior byte-identical.

## Validation (real RUN 3 artifacts, forward replay, only the tier toggled)

Shuffle hop 2.1.85→86 (the measured pathology):

|                          | OLD         | NEW (hash tier)          |
| ------------------------ | ----------- | ------------------------ |
| inherited                | 17,625      | 18,880 (10,955 via hash) |
| residueLocality          | 2,341       | 1,086 (−54%)             |
| conflictDisagree         | 1,438       | 213 (−85%)               |
| moved lines              | 4,232 (46%) | 2,483 (33%, −41%)        |
| files w/ structural diff | 675         | 619                      |

Quiet hop 2.1.89→90 (100% order preserved) — regression guard:
assignment differs on **4/20,422 statements**; moved 66→59 lines; rename/real
churn byte-identical. The tier is a near-no-op when nothing is wrong.

Cost: hashing 20k statements ≈ 750ms per hop.

Abstain census on 85→86 (probe-abstains.mts): hit 10,955; no-prior-hash 787
(real edits — name votes carry the name-stable ones); unequal-count unanimous
only 48 (33 ≥200B, max 640B) → loosening the count rule buys nothing;
non-unanimous 3,062 (short duplicated statements — name votes rightly decide).
Remaining movement is statements _edited and renamed in the same hop_ —
shrinks as rename noise shrinks; content identity cannot recover it.

## Scripts

- `validate-pair.mts <verA> <verB> <outRoot>` — replay a hop old-vs-new from
  RUN 3 artifacts (read-only), emit raw statement trees + assignment delta.
- `probe-abstains.mts` — abstain-bucket census for the 85→86 pair.
