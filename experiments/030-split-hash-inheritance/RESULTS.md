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

## Late-era validation (archived pre-capture-fix run, using-era bundles)

`using` syntax parses and hashes fine (~29-34k statements, ≈850ms). LIS order
census: the late era is order-stable (183→185 100%, 185→186 97.5%, 186→187
99.1%, 187→190 100%, 190→191 99.9%) — EXCEPT 2.1.207→208 at **73.1%**, the
history's biggest churn hop (73.4% of files). Shuffle hops are rare but
catastrophic; the rest of the ≥50%-churn hops are real-change-heavy.

Shuffle hop 2.1.207→208 (strongest result — heavier rename noise in this
pre-capture-fix archive means name votes miss more, so content identity
recovers more):

|                  | OLD          | NEW (hash tier)          |
| ---------------- | ------------ | ------------------------ |
| moved lines      | 10,768 (37%) | 4,480 (19.5%, **−58%**)  |
| residueLocality  | 4,358        | 2,668 (−39%)             |
| conflictDisagree | 2,155        | 627 (−71%)               |
| inherited        | 29,665       | 31,355 (19,720 via hash) |
| re-homed         |              | 539/34,023 statements    |

Quiet hop 2.1.187→190 (the 188/189 npm gap): 16/29,399 statements differ;
conflicts 115→39. Its apparent 53% movement is ONE ~1,450-line edited
statement (`initializeMetricsVal`) relocating to a sibling file — the
edited-AND-renamed residual class, correctly out of the hash tier's reach.
Heavy real-change hop 2.1.185→186 (97.5% order): 140/29,286 re-homed,
conflicts 591→330, residue −18%.

## Scripts

- `validate-pair.mts <verA> <verB> <outRoot>` — replay a hop old-vs-new from
  walk artifacts (read-only; `VERSIONS_ROOT` env overrides the versions
  root, e.g. the archived run), emit raw trees + assignment delta.
- `probe-abstains.mts` — abstain-bucket census for the 85→86 pair.
