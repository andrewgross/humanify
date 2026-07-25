# 042 — Anchor-preempt: results

Brief: [README.md](README.md). Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md).

**Status: Task A COMPLETE (mechanism confirmed by reading all 14 pairs), Task B
BUILT (one `PLACEMENT_TIERS` entry, TDD red-first, `npm run check` green),
Task C gate RUNNING.**

## Task A — the mechanism, read pair by pair

`eyeball-preempt.ts` prints, for every minted-counter anchor disagreement, the
evidence for BOTH halves of the brief's claim:

1. the fresh block's content IS the prior block's content under a recycled
   counter, and
2. the name the vote followed belongs to a genuinely different prior block.

Its population reproduces the ceiling exactly — **12 statements / 3,029 git
lines on 85→86, 2 / 141 on 215→216** — so it is reading the same 14 statements
the ceiling priced. Every one was placed by the **`name`** tier in the replay,
which is the tier the new one has to outrank.

### 85→86, all twelve

| fresh# |  ln | fresh name              | prior name (anchor twin) | fresh/prior ln | edited ln | shared rare literals | the prior name's fate | prior owner of the FRESH name | its shared literals |
| -----: | --: | ----------------------- | ------------------------ | -------------: | --------: | -------------------: | --------------------- | ----------------------------- | ------------------: |
|  17132 | 554 | initializeApp225        | initializeApp242         |        279/278 |       2+1 |               **95** | swapped onto #18039   | #12594 (18 ln)                |               **0** |
|  13247 | 523 | initializeApp199        | initializeModules73      |        267/265 |       5+4 |               **33** | swapped onto #11665   | #12925 (11 ln)                |               **0** |
|  11089 | 512 | initializeApp236        | initializeApp309         |        258/258 |       2+2 |               **80** | vanished (re-minted)  | #16343 (13 ln)                |               **0** |
|   7371 | 450 | initializeModule80      | bootstrapModulesVar      |        229/229 |       4+4 |                    5 | swapped onto #15577   | #4132 (10 ln)                 |               **0** |
|   2263 | 380 | initializeApp235        | initializeApp337         |        196/196 |       6+6 |              **117** | vanished (re-minted)  | #16214 (8 ln)                 |               **0** |
|   7950 | 158 | initializeApp163        | initializeApp291         |          83/82 |       4+3 |                    7 | swapped onto #14158   | #7155 (7 ln)                  |               **0** |
|   6390 | 137 | initializeApp10         | initializeApp152         |          69/70 |       1+1 |                    2 | swapped onto #6990    | #5009 (3 ln)                  |               **0** |
|  14132 | 115 | initializeApp288        | initializeApp418         |          60/61 |       3+3 |                    2 | vanished (re-minted)  | #19572 (7 ln)                 |               **0** |
|  16164 |  66 | bootstrap36             | initializeApp406         |          37/39 |       4+6 |                    2 | vanished (re-minted)  | #17803 (19 ln)                |               **0** |
|   6990 |  64 | initializeApp152        | setupApplicationVar      |          34/34 |       2+2 |               **46** | swapped onto #7935    | #6284 (70 ln)                 |               **0** |
|  18863 |  36 | initializeApp234        | bootstrapApplicationItem |          19/19 |       1+1 |                    1 | swapped onto #11155   | #15393 (11 ln)                |               **0** |
|  19326 |  34 | initializeEnvironment12 | configureEnvironmentVar  |          18/18 |       1+1 |                    1 | vanished (re-minted)  | #10732 (134 ln)               |               **0** |

### 215→216, both

| fresh# |  ln | fresh name          | prior name (anchor twin)    | fresh/prior ln | edited ln | shared rare literals | prior owner of the FRESH name |
| -----: | --: | ------------------- | --------------------------- | -------------: | --------: | -------------------: | ----------------------------- |
|  15558 |  92 | initializeApp180    | initializeModuleRegistryVal |          47/48 |       1+2 |                    5 | #14865 (4 ln), 0 shared       |
|  23137 |  49 | initializeModule440 | initializeSkillDoctorModule |          49/30 |      24+6 |                    4 | #33557 (37 ln), 0 shared      |

### What the reading proves, and the one thing it corrects

**Claim 1 holds in all 14.** Ten of the twelve on 85→86 are the same block
edited by 1–6 lines out of 18–279 — the shape exp040 predicted. The two
exceptions are informative rather than contrary: 215→216's `initializeModule440`
grew 30→49 lines, and the anchor still identified it on
`"skill-doctor"` / `"cli_skill_doctor"` / `"Show which loaded skills are unused
and costing context"`. Content survives an edit; a minted counter does not
survive a release.

**Claim 2 holds in all 14, and more strongly than the brief claimed.** In every
single case the prior owner of the fresh statement's name shares **zero** rare
literals with it — the vote is not choosing between two plausible homes, it is
following a name whose previous holder has nothing to do with this code. Those
previous holders are also almost all small (3–19 lines against 18–279), so the
vote is dragging a large block to a small block's address.

The brief's phrasing — "the renamer recycled the minted counter onto a different
block" — describes half of what happens. Both halves occur:

- **swapped** (7/14): the prior block's own name still exists in the fresh
  release on a different statement. `initializeApp242`'s 279-line block came
  back as `initializeApp225`, and some other fresh statement took
  `initializeApp242`.
- **vanished** (7/14): the prior name is simply not in the fresh release; the
  block re-minted into a counter that last release belonged to someone else.

Both are the same fact — **the counter is a slot number, not an identity** — but
it is worth stating that a "recycled counter" is not always a two-way swap,
because a rule keyed on "does the prior name still exist" (the exp043 tiebreak
sketched in the brief's Task D) would only see half of these.

The cleanest single proof is a **3-cycle inside one hop**: prior#6284 was
`initializeApp152`; that block comes back as fresh#6390 named `initializeApp10`;
the name `initializeApp152` lands on fresh#6990, which is really prior#11249
`setupApplicationVar` (46 shared rare literals, all `chrome_bridge_*` telemetry
events). Two statements, two name votes, both confidently wrong, both pointing
at each other's addresses. The anchor untangles both.

## Task B — the build

One entry in `PLACEMENT_TIERS` (`src/split/stable-split.ts`), directly above
`ordinal`/`name` and below `hash`/`preempt`, plus its predicate
`anchorPreemptTier`. It mirrors the existing `preempt` (Lever A) entry exactly:
it fires only when it **disagrees** with the vote it replaces, and abstains
otherwise. Gates:

- the content anchor has a verdict (all of exp041's precision gating already
  applied: rare on both sides, unique candidate, ≥50% token overlap, unique
  claim), **and**
- the statement declares at least one **outer** binding, and **every** one of
  them carries a minted counter (`hasMintedNumber`, the same predicate
  `isRejectedStem` uses), **and**
- the anchor's file differs from the name vote's file.

Outer bindings, not `declaredNames`: a function's parameters are not the
statement's identity, and exp041's Finding 2 showed they are already a source of
junk votes. Kill switch `HUMANIFY_NO_ANCHOR_PREEMPT=1`.

Deliberately narrow. Where a name is meaningful (`managedAgentsReadme`,
1,126 lines on 197→198) both witnesses are credible and preferring the anchor is
a coin flip that can create relocation as easily as remove it; a large prose blob
is also the shape most likely to share rare literals with unrelated code. Those
keep their name vote — there is a unit test asserting exactly that.

Four unit tests, red before the tier existed: the minted case preempts, a
meaningful name does not, the kill switch restores the name vote, and an anchor
that AGREES with the vote leaves the statement on the tier that already had it
right (the common case — 3,845 agreements against 19 disagreements on 85→86).

## Task C — gate: PASSED on all five checks

Same host, same warm LLM cache, same `REBASE_PRIOR=1` methodology as the
`exp041-anchor` control, so the A/B is like-for-like.
`experiments/041-content-anchor/gate-verdict.sh exp041-anchor exp042-preempt`;
raw output in [gate-output.txt](gate-output.txt).

### TOTAL — four hops

| measure (git lines, 4 hops) | control |   candidate |              change |
| --------------------------- | ------: | ----------: | ------------------: |
| **relocation**              |   7,764 |   **4,518** | **−3,246 (−41.8%)** |
| total reviewed diff         | 161,410 |     157,923 |      −3,487 (−2.2%) |
| noise                       |  13,892 |      13,894 |                  +2 |
| naming                      |   7,614 |       7,616 |                  +2 |
| **reorder**                 |   6,078 |   **6,078** |               **0** |
| alias                       |     200 |         200 |                   0 |
| **novel** (real change)     |   4,188 |   **4,188** |               **0** |
| **realLines** (real change) | 416,377 | **416,377** |               **0** |
| minted leftovers            |      85 |      **85** |                   0 |

Predicted −3,170; actual **−3,246**, +2.4% over the ceiling. The ceiling prices
each disagreement in isolation, so both directions of error are expected; it
came in high because moving a block out of a file it never belonged in also
stops dragging the locality fallback of everything after it.

### 1. Relocation down on EVERY hop — PASS

| hop     | control | candidate | change | predicted | note                    |
| ------- | ------: | --------: | -----: | --------: | ----------------------- |
| 85→86   |   4,746 | **1,670** | −3,076 |    −3,029 | −64.8%                  |
| 118→119 |      16 |    **16** |      0 |         0 | **tree byte-identical** |
| 197→198 |   2,150 | **2,150** |      0 |         0 | **tree byte-identical** |
| 215→216 |     852 |   **682** |   −170 |      −141 |                         |

No hop regressed, and the two hops the ceiling said to leave alone were left
alone in the strongest possible sense: `diff -rq` over the entire emitted `src/`
tree reports **0 differing entries** on 2.1.118-rebased, 2.1.119,
2.1.197-rebased and 2.1.198. The canary did not merely fail to regress — it did
not change at all.

That includes 197→198's `managedAgentsReadme` (1,126 lines) and
`managedAgentsDocsVal` (678), the meaningful-name disagreements the narrow rule
declines to touch. The rule's silence there is a design choice that held.

### 2. Real change unmoved — PASS

`novel` (4,188) and `realLines` (416,377) are identical per hop and in total, as
are `mints` (85), `newName` (4,307), `noise` (3,125) and `noiseLn` (61,878) in
the statement-level leaderboard.

Noise moved **+2 lines**, all of it naming on 85→86, against −3,076 lines of
relocation on that same hop — a 1,538:1 trade. **`reorder` did not move at all**,
on any hop. That was the specific watch item: exp041 moved it +48 on 85→86, and
this lever targets the same hop and the same statement class.

Per hop, 118→119, 197→198 and 215→216 have **every** noise sub-bucket identical
to the line; only 85→86 moved, by two lines.

### 3. Boot gate — PASS, all four

    2.1.86  {"version":"2.1.86 (Claude Code)","ok":true}
    2.1.119 {"version":"2.1.119 (Claude Code)","ok":true}
    2.1.198 {"version":"2.1.198 (Claude Code)","ok":true}
    2.1.216 {"version":"2.1.216 (Claude Code)","ok":true}

Run with `bun` explicitly on `PATH`. It lives at `~/.bun/bin` and is NOT on the
default PATH on this host, and `run.sh` degrades to `BOOT GATE SKIPPED` rather
than failing — which is how this host once scored evals without ever checking
that the output runs.

### 4. Self-hop invariant — PASS

    {"selfHop":{"version":"2.1.216","identical":true,"diffLines":0}}

Byte-identical in **bundle AND split ledger**, both re-verified with `cmp`.

### 5. Did the tier fire where the ceiling predicted? — PASS, statement for statement

Not "a KPI moved". The placement trail names every statement the tier claimed,
and it is the same list Task A read by hand, in the same destinations:

| hop     | `anchorPreempt` | predicted | vs Task A's hand-read indices |
| ------- | --------------: | --------: | ----------------------------- |
| 85→86   |          **12** |        12 | 0 missing, 0 unpredicted      |
| 118→119 |           **0** |         0 | —                             |
| 197→198 |           **0** |         0 | —                             |
| 215→216 |           **2** |         2 | 0 missing, 0 unpredicted      |

Every other tier is untouched on every hop — the new tier took its statements
from the `name` tier and from nowhere else:

    85->86    control  {"hash":10954,"name":7617,"preempt":6,"ordinal":466,
                        "novote":695,"allsame":170,"anchor":30,"conflict":27,"fill":1}
              candidate{"hash":10954,"name":7605,"preempt":6,"ordinal":466,
                        "novote":695,"allsame":170,"anchor":30,"anchorPreempt":12,
                        "conflict":27,"fill":1}

`name` 7,617 → 7,605 is exactly −12; hash, ordinal, all-same, anchor, fill and
the locality residue (722) do not move by one statement. Same on 215→216:
`name` −2, everything else identical, residue 1,061 both sides. Each trail sums
to its hop's full statement population (19,966 / 23,442 / 31,839 / 35,903).

Spot-check, `check-trail.ts` against Task A:

    #17132  initializeApp225
        placed:  src/uri-validator/lsp-search/output-size.js      <- the anchor twin's file
        overrode:src/lsp/plugin-management/file-history-tracker.js <- the recycled name's home

### The one metric that moved the "wrong" way, and why it is not a regression

The leaderboard's `reloc` column rose **+14** — the only non-zero cell in the
whole comparison:

| model          | noise | noiseLn |   reloc | relocSt | newName | mints | reorderLn |
| -------------- | ----: | ------: | ------: | ------: | ------: | ----: | --------: |
| exp041-anchor  |  3125 |   61878 |     617 |       0 |    4307 |    85 |      6078 |
| exp042-preempt |  3125 |   61878 | **631** |       0 |    4307 |    85 |      6078 |

+12 on 85→86, +2 on 215→216, 0 elsewhere — **exactly the 14 statements the tier
placed, one for one.** That is the metric measuring what the tier deliberately
did, not a side effect.

`sameNameMovedFile` is **name-keyed**: for each fresh name, did the file holding
that NAME change since the prior release? The whole premise of this experiment
is that when a name is a recycled minted counter, following it is wrong — so
putting `initializeApp225`'s content back in its content's file necessarily
moves the NAME `initializeApp225` off the file the previous holder of that
counter lived in. A name-keyed proxy cannot distinguish that from churn, and
`analyze.ts`'s own docstring says so: _"Read `tree.relocatedStatements` for the
order-independent answer."_

`relocatedStatements` — hash-keyed, order-independent — is **0 on every hop for
both models**, and the content-keyed git-line measure is −3,246. Every measure
that can see content identity says the diff got smaller or stayed identical;
the one that can only see names counts our 14 intended moves.

## What ships

One entry in `PLACEMENT_TIERS` plus `anchorPreemptTier` in
`src/split/stable-split.ts`, and `anchorPreempt` added to `DETAILED_TIERS` in
`src/split/placement-trail.ts` so the trail describes each firing. Kill switch
`HUMANIFY_NO_ANCHOR_PREEMPT=1`. No new file, no new dependency — the registry
exp041 left behind made this one entry instead of eight edit sites, which is the
first time that refactor paid.

## What is left on this axis

Residual relocation is **4,518** git lines across the four hops, down from
15,699 before exp041 — a **71.2%** cumulative reduction over the two
experiments. Of what remains:

- **3,392 lines** sit in anchor/name disagreements where BOTH witnesses are
  credible (`managedAgentsReadme` and friends). That is exp043's Task D: decide
  it with evidence rather than preference. Task A above sharpens the design —
  the obvious test, _"does the prior statement that owns the NAME still exist in
  the fresh release?"_, would only fire on **7 of the 14** cases here, because
  the other 7 saw their prior name vanish entirely rather than swap onto another
  statement. A rule built on that test needs both branches.
- The rest is the `novote` residue — statements with no prior name and no rare
  literal (695 / 1,310 / 1,173 / 1,035 per hop), unchanged by this lever.

With relocation at 4,518, **naming (7,614) is now decisively the largest
reducible source**, and 76% of it is one hop, 85→86. exp039's brief is the next
lever, and it should be re-measured on these trees first: placement work moved
naming by −74 lines in exp041 and +2 here, neither of which anyone predicted.
