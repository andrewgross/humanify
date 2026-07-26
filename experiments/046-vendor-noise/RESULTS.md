# 046 — Vendor noise: results

Read `README.md` (the brief) first for what was believed going in, and this
file for what the numbers said. Where they disagree, **this file is right** —
that is rule 8, and the brief now carries a STATUS block saying which of its own
claims did not survive.

## Totals first

Vendor churn across the four gate hops, decomposed. The eval had never scored
this tree at all; `run.sh` passed `("$OUT/src" "$PRIOR_SRC")` and nothing else.

| bucket                             |      lines | share | reducible?                      |
| ---------------------------------- | ---------: | ----: | ------------------------------- |
| manifest `factoryVar`              |     12,665 | 34.7% | **yes — shipped, Task B**       |
| bodies, local-name reroll only     |     13,980 | 38.3% | **yes — Task C**                |
| manifest, other fields             |      6,411 | 17.6% | mostly entry blocks moving      |
| files added / removed              |      3,282 |  9.0% | **no — real dependency change** |
| bodies, free minified token reroll |         44 |  0.1% | yes                             |
| bodies, genuinely edited           |         82 |  0.2% | **no — real dependency change** |
| **TOTAL**                          | **36,482** |       |                                 |

Measured 36,482 against the brief's published 36,201 (0.8% apart; GNU `diff -rN`
and per-file `diff` pick different edit scripts for added/removed files).

**Real dependency change is ~3,364 lines of 36,201 — 9.2%. The other 90.8% is
noise.** On three of the four hops, genuine change to a library body is
**zero**.

Per hop:

| hop     | vendor | manifest | of it `factoryVar` | bodies name-only | bodies REAL | added/removed |
| ------- | -----: | -------: | -----------------: | ---------------: | ----------: | ------------: |
| 85→86   | 11,540 |    7,748 |              3,174 |            3,790 |           0 |             0 |
| 118→119 |  6,655 |    3,004 |              2,984 |            3,330 |           0 |       160/159 |
| 197→198 |  9,417 |    4,744 |              3,242 |            3,268 |          82 |       899/386 |
| 215→216 |  8,870 |    3,580 |              3,265 |            3,592 |           0 |       899/779 |

## Task 0 — vendor is a scored surface (landed, `d33ae83`)

Its own two columns, never folded into the `src` numbers, so every committed
reference stays comparable:

- `vendorLn` (↓) reducible churn
- `vendorReal` (=) genuine dependency change

Two columns and not one, because a single "vendor churn" number is winnable by
dropping real change — the exact failure the brief said to guard. References
written before this print `-`, which is not `0`.

The scorer (`vendor-churn.ts`) is shared by `analyze.ts` and the offline report,
so the gate's KPI and the write-up cannot drift.

## Task A — the decomposition, and three corrections it forced

### A1. The unit is the FILE, not the line

A vendored library is one to four lines of ~100KB minified text. A changed-line
count says only that a file changed at all, so 36,201 cannot be decomposed
line-wise. Counts are files; GNU-diff lines ride alongside so the numbers
reconcile with the published baseline instead of approximating it.

A first attempt counted changed lines with a position-blind multiset compare and
ran systematically LOW — 8,841 → 6,924 on 215→216 — because a line that merely
MOVES cancels against its own copy on the other side. Shelling out to `diff`
removed the discrepancy.

### A2. `structuralHash` CANNOT gate body reuse — the brief's C1 is unsafe

This is the finding that mattered most, and it kills the option as written.

The brief proposed reusing a prior body when the manifest's `structuralHash`
matches, and flagged its own gating question: does that hash cover literals?
It does not. `computeStructuralHash` calls `hashAndMapPath(fnPath, **false**)`,
so in the serializer strings collapse to `S=__STR_<length>__`, numbers to
`N=<floor(log10(|v|+1))>`, bigints to `B=0`, template quasis to their length.

`hash-probe.ts`, run against the real function:

| difference                    | visible to `structuralHash`? |
| ----------------------------- | ---------------------------- |
| string value, SAME length     | **no**                       |
| URL string, same length       | **no**                       |
| number, same magnitude bucket | **no**                       |
| bigint value                  | **no**                       |
| template text, same length    | **no**                       |
| property name / free ident    | yes                          |
| regex pattern / operator      | yes                          |

**Six of twelve semantic differences are invisible to it.** Keying reuse on it
would ship a vendored library carrying the previous release's endpoints,
timeouts and constants, silently.

Worth recording how nearly this was missed: the first probe reported "string
literal differs → DIFF" and read as reassurance. It was comparing `'alpha'`
against `'beta'`, which differ only in LENGTH. Same-length literals collide.
That is rule 1 — a match not eyeballed — inside the tool built to check rule 1.

### A3. The `factoryVar` consumer audit — confirmed, one number corrected

Confirmed by re-reading rather than trusting the brief. No consumer of the
WRITTEN manifest reads it: `loadPriorVendorNames` keys on `structuralHash` +
`name`; `library-detection/adapters/bun.ts`, `commands/unified.ts` and
`split/bun-relink.ts` never mention it; nothing outside `src/` reads it. Its
three live uses (`bun.ts:363`, `:420`, `:498`) are all on the in-memory
`CjsFactoryRecord`.

**Corrected:** the brief sized this as "91% of that file's diff is one field."
That holds only on 215→216 (3,265/3,580). Across four hops `factoryVar` is
**66%** of the manifest diff and **35%** of vendor churn. 85→86's manifest churn
is mostly whole ENTRY BLOCKS moving as bundle order shifts — not a field value.

### A4. Two contaminated buckets, both caught by reading

Both are rule 3: a predicate that did not test what its name implied.

1. **"Real change" was contaminated by free identifiers.** The first classifier
   called a file changed when its literal-preserving signature moved. Reading
   the diverging tokens showed they were `I=` — the serializer's verbatim class,
   which covers BOTH property keys and free (unresolved) identifiers. The actual
   substitutions were `lti→Lii`, `vn→In`, `w→A`: bundle-level minified tokens
   the minifier rerolled. 18 of the 56 originally-flagged files were this.

2. **Path-keyed matching invented dependency change.** 197→198 read as 133 added

   - 133 removed files. Reading them: `kotlin.js` in 198 contains **WebAssembly**
     keywords, and `elixir.js` holds different grammar on each side. These are
     highlight.js language grammars whose humanify-chosen FILENAMES rotated. This
     is the same trap as the leaderboard's name-keyed `reloc` column (rule 7) —
     a name is not an identity. Matching by content signature instead cut it to
     127/127, and 6 files resolved as pure moves.

   The residual 127/127 symmetry is **unresolved**. It is consistent with a
   highlight.js version bump, and it is counted as real dependency change, which
   is the conservative direction. It is 7% of vendor churn and does not affect
   any conclusion here.

3. **Require-path literals are humanify's own churn.** 16 files on 85→86 diverged
   only in strings like `S="../lodash/lib_eb5345cb.js" → "…-2.js"` — dependents
   of a vendor file humanify renamed. The offline report masks intra-tree require
   paths to separate that from library change; after masking, 85→86's real body
   change is **0**.

## Task B — `factoryVar` dropped from the manifest (landed, `1bb0c61`)

12,665 lines, 35% of all vendor churn, for a field whose own docstring said
"(debug only)" and which no consumer of the written manifest read. Bun's
minifier reassigns the token every build, so it churned whether or not any code
changed.

A prior manifest that still HAS the field must keep parsing — verified with a
test that writes one and reads it back, not assumed. The `library-detection`
fixtures keep their `factoryVar` deliberately and now say why.

Deliberately changes output: `src/` byte-identical, the manifest changes by
exactly the removed field.

## Task C — vendor body inheritance (landed, `0714b24`)

`src/split/vendor-body-inherit.ts`. When the prior release holds the same
program at the same path, its bytes are written and the file leaves the diff.
Kill switch `HUMANIFY_NO_VENDOR_INHERIT=1`.

**The key is the whole safety argument, and the brief's would have shipped wrong
code** (§A2). This keys on `computeStructuralSignature` — literal-PRESERVING,
bindings replaced by slot ordinals — not `structuralHash`. Require paths are
deliberately IN the key rather than normalized away: a body matched while
ignoring its imports could require a path this tree does not have. That costs 80
lines of 13,980. A file that fails to parse is never inherited.

Reach, predicted from the shipped decision function before the gate ran
(`predict-inherit.ts`) and confirmed by it:

| hop     | vendor files | changed vs prior | inherited |
| ------- | -----------: | ---------------: | --------: |
| 85→86   |        1,592 |            1,592 | **1,575** |
| 215→216 |        1,647 |            1,621 | **1,613** |

Every common vendor file changes bytes each release; ~99% are the same program.
Re-running the same probe on the POST-C trees returns `changed 17, inherit 0` —
there is nothing left to inherit, which is the mechanism confirming itself.

## The gate — all six criteria, every hop on its own

`experiments/041-content-anchor/gate-verdict.sh exp043-nearident exp046-bodyinherit`

**1. Vendor churn DOWN on every hop.**

| hop        |   baseline | after B | after B+C |      total |
| ---------- | ---------: | ------: | --------: | ---------: |
| 85→86      |     11,540 |   8,366 | **4,656** |     −59.7% |
| 118→119 🐤 |      6,651 |   3,667 |   **337** |     −94.9% |
| 197→198    |      9,169 |   5,923 | **2,655** |     −71.0% |
| 215→216    |      8,841 |   5,576 | **1,984** |     −77.6% |
| **TOTAL**  | **36,201** |  23,532 | **9,632** | **−73.4%** |

**2. `src/` did not regress.** Every KPI byte-identical to control:
`noise` 3125 (=), `noiseLn` 61878 (=), `newName` 4307 (=), `mints` 85 (=),
`reorderLn` 6078 (=), `relocSt` 0 (=). Order-independent relocation, read via
`relocation-churn.ts` rather than the name-keyed `reloc` column (rule 7), is
identical on all four hops: 349 / 16 / 343 / 682.

**3. `novel` 4188 (=) and `realLn` 416377 (=) UNMOVED**, and the new
`vendorReal` column — the one that exists to catch exactly this — reads
**3,364 → 3,364**. No real dependency change was dropped to buy the win.

**4. Boot gate green on all four**, `--version` and the live `-p` round-trip.
This mattered more here than in any prior experiment: C rewrites code that runs.

**5. Self-hop byte-identical**, bundle and split ledger, 0 diff lines.

**6. The 118→119 canary — the hop with the least to win — fell furthest**
(−94.9%) and regressed nothing.

### What the residual 9,632 is

It reconciles exactly with Task A's buckets, which is the check that the lever
moved for the reason claimed:

| hop     | residual | manifest (entry blocks) | bodies / files |
| ------- | -------: | ----------------------: | -------------: |
| 85→86   |    4,656 |                   4,574 |             82 |
| 118→119 |      337 |                      20 |            317 |

**Body churn is essentially eliminated.** What remains is (a) manifest ENTRY
BLOCKS moving as bundle order shifts, and (b) genuine library add/remove.

## Where this axis now stands

Vendor: **36,201 → 9,632 lines**, and ~3,364 of the residual is real dependency
change. The reducible remainder is ~6,300 lines, dominated by manifest entry-block
reordering.

**Do not "fix" that by sorting the manifest.** It is written in bundle order and
`loadPriorVendorNames` documents position as its tie-break for same-hash groups
(re-export shims are structurally identical but proxy different libraries).
Sorting would misname every member of those groups.

The other named residual is vendor FILE NAMING: the 16 files on 85→86 whose only
change is a require path pointing at a vendor file humanify renamed, and the
highlight.js grammars whose filenames rotate between releases (§A4). That is the
same rotation class exp041–043 solved for `src/`, not a body problem.

## What did not survive from the brief

See the STATUS block at the top of `README.md`.
