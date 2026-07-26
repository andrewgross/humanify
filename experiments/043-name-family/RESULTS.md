# 043 — Corroborated content: results

Brief: [README.md](README.md). Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md).

**Status: SHIPPED. Gate PASSED on every non-negotiable — relocation down on the
two hops the ceiling targeted and BYTE-IDENTICAL on the two it did not, real
change untouched, every noise sub-bucket unmoved on every hop, all four trees
boot, self-hop byte-identical in bundle AND ledger, and the tier fired on
exactly the seven statements read by hand.**

**Cross-file relocation fell 69.2% — 4,518 → 1,390 git lines. Cumulative across
exp041–043: 15,699 → 1,390, a 91.1% reduction.**

## TOTAL — four hops

| measure (git lines, 4 hops) | control `exp042-preempt` |     candidate |              change |
| --------------------------- | -----------------------: | ------------: | ------------------: |
| **relocation**              |                    4,518 |     **1,390** | **−3,128 (−69.2%)** |
| total reviewed diff         |                  157,923 |       154,668 |      −3,255 (−2.1%) |
| **noise**                   |                   13,894 |    **13,894** |               **0** |
| naming / alias / reorder    |      7,616 / 200 / 6,078 | **identical** |               **0** |
| **novel** (real change)     |                    4,188 |     **4,188** |               **0** |
| **realLines** (real change) |                  416,377 |   **416,377** |               **0** |
| minted leftovers            |                       85 |        **85** |                   0 |

Predicted −3,098; actual **−3,128**, within 1%. Unlike exp041 (+50 reorder, +18
naming) and exp042 (+2 naming), **nothing regressed at all** — every noise
sub-bucket is identical to the line on every hop.

## 1. Relocation per hop — PASS

| hop     | control | candidate | change | predicted | note                    |
| ------- | ------: | --------: | -----: | --------: | ----------------------- |
| 85→86   |   1,670 |   **349** | −1,321 |    −1,294 | −79%                    |
| 118→119 |      16 |    **16** |      0 |         0 | **tree byte-identical** |
| 197→198 |   2,150 |   **343** | −1,807 |    −1,804 | −84%, within 0.2%       |
| 215→216 |     682 |   **682** |      0 |         0 | **tree byte-identical** |

`diff -rq` over the entire emitted `src/` tree reports **0 differing entries** on
2.1.118-rebased, 2.1.119, 2.1.215-rebased and 2.1.216. Two of the four hops did
not change at all.

Cumulative on 85→86 across the series: 7,583 → 4,746 → 1,670 → **349** (−95.4%).

## 2. Real change unmoved — PASS

`novel` 4,188 and `realLines` 416,377 identical per hop and in total, as are
`mints` (85), `newName` (4,307), `noise` (3,125) and `noiseLn` (61,878) in the
statement-level leaderboard, and **every layout sub-bucket on every hop**.

## 3. Boot gate — PASS, all four

    2.1.86  ok    2.1.119 ok    2.1.198 ok    2.1.216 ok

Run with `bun` on `PATH` (it lives at `~/.bun/bin`, is not on the default PATH
on this host, and `run.sh` degrades to `BOOT GATE SKIPPED` rather than failing).

## 4. Self-hop invariant — PASS

    {"selfHop":{"version":"2.1.216","identical":true,"diffLines":0}}

Byte-identical in bundle AND split ledger, both re-checked with `cmp`.

## 5. Did the tier fire where the ceiling predicted? — PASS, statement for statement

| hop     | `anchorPreempt` | of which NEW | predicted new | vs the hand-read list    |
| ------- | --------------: | -----------: | ------------: | ------------------------ |
| 85→86   |          **17** |        **5** |             5 | 0 missing, 0 unpredicted |
| 118→119 |           **0** |            0 |             0 | —                        |
| 197→198 |           **2** |        **2** |             2 | 0 missing, 0 unpredicted |
| 215→216 |           **2** |            0 |             0 | —                        |

Every other tier is untouched on every hop; `name` fell by exactly the number of
statements the tier claimed (7,605→7,600 and 11,332→11,330). Each trail sums to
its hop's full statement population.

The five new firings on 85→86 are exactly the names exp042's `hasMintedNumber`
could not see, and the two on 197→198 are the documents its brief fenced off:

    #13271 initializeApp256        -> session/plan-review/status-message.js
    #10397 initializeEnvironment9  -> uri-validator/diff-tool/patch-generator.js
    #7935  setupApplicationVar     -> theme/event-propagator/event-path.js
    #6518  initializeModulesData   -> storage/runtime-client/environment-setup.js
    #11603 initApp16               -> uri-validator/lsp-search/uri-present.js
    #29382 managedAgentsReadme     -> proxy-env/config/silent-handler.js
    #29372 managedAgentsDocsVal    -> storage/error-messages/socket-connector.js

`initializeApp256` is the `exitPlanMode` block — the exemplar exp040, exp041 and
exp042 were all written around. It escaped exp042 because **256 is a KNOWN unit
token** (the carve-out that protects `sha256Hasher`), and it escaped exp041
because the name vote outranked the anchor. It is now placed by content.

## The case that proves the rule, read line by line

The largest single win is a documentation constant. Across 2.1.197→198 the whole
565-line "Managed Agents — Go" README has exactly TWO differing lines:

    1c1
    < var MANAGED_AGENTS_README = `# Managed Agents — Go
    > var managedAgentsReadme   = `# Managed Agents — Go

    97a98
    > fmt.Printf("Trace: https://platform.claude.com/.../sessions/%s\n", session.ID)

One is upstream's real change. **The other is ours** — the renamer flipped
`MANAGED_AGENTS_README` to `managedAgentsReadme`, and because the prior release's
`managedAgentsReadme` was the _Java_ README, the name vote sent the Go document
to the Java document's file. A one-line documentation change was rendering as
**1,126 git lines**: 565 deleted from `proxy-env/config/silent-handler.js` and
565 added to `proxy-env/url-credentials/input-validator.js`.

This also answers exp042's stated fear directly. Its brief argued a big prose
blob is the shape most likely to share rare literals with unrelated code, so
adjudicating it on content alone was a coin flip. In fact these documents are
SIBLINGS — the Go and Java READMEs share most of their structure — and the
anchor still picked the right one, because `github.com/anthropics/anthropic-sdk-go`
occurs in exactly one statement per release. The 0.4% edit fraction then
corroborates that pick across all 565 lines. Had the anchor grabbed the Java
sibling, the edit fraction would have been large and the tier would have
declined. **Near-identity is the defence against the failure the brief feared,
not an exposure to it.**

## What ships

`changedLineFraction` + `contentAnchorVerdicts` in `src/split/content-anchor.ts`
(the anchor now reports whether its pairing is corroborated, not just which file
it chose), and one extra disjunct in `anchorPreemptTier`'s gate in
`src/split/stable-split.ts`. `contentAnchorFiles` is gone — the verdict carries
more than a file now.

    anchor preempts the name vote when it DISAGREES and
      (every outer name carries a minted counter          <- exp042
       OR the twin differs by <= 10% of this statement's lines)  <- exp043

`NEAR_IDENTICAL_MAX_EDIT = 0.1`, chosen in the middle of a measured 3× gap
(5.8% → 16.0%): every value from 6% to 15% selects the same seven statements.
Kill switches `HUMANIFY_NO_ANCHOR_NEARIDENT=1` (this disjunct) and
`HUMANIFY_NO_ANCHOR_PREEMPT=1` (the whole tier).

The gates are complementary, not nested, and there is a test pinning each
direction: exp042's `initializeModule440` is 49% edited and only the name shape
catches it; `managedAgentsReadme` is undecorated and only near-identity does.

## Two predicates measured and REJECTED — do not retry

- **Strip the ladder decoration.** `Val`/`Var`/`Ref`/`Item`/`Data`/`Result`/
  `Value` and trailing digits, sourced from `DECORATION_WORDS` in
  `src/llm/validation.ts` — the same ladder that mints the counters. Catches
  `setupApplicationVar` and `initializeModulesData`, but `managedAgentsReadme`
  wears no decoration and is the single largest case.
- **Name-family size.** The families are real — `startApp` has ten members
  (`startAppVar`, `startAppVal`, `startAppItem`, `startAppRef`, `startAppValue`,
  `startAppData`, `startAppResult`, `startApp2`, `startApp3`), the ladder
  exhausting its vocabulary in declaration order — but the size distribution is a
  smooth power law (30,718 stems of size 1, no valley), so any cut is a tuned
  parameter rather than a measurement. It also misses `managedAgentsReadme`,
  whose family is 1.

Both are in `family-size.ts` / `family-probe.ts` with their measurements, so the
next reader can see why they lost rather than re-deriving it.

## The metric that moved the "wrong" way, again

The leaderboard's `reloc` rose **+7** — exactly the seven statements the tier
placed, the same pattern as exp042's +14. `sameNameMovedFile` is name-keyed, and
this experiment exists because a name that rotated between siblings is not an
identity; moving the content back necessarily moves the name off the previous
holder's file. The order-independent `relocatedStatements` is **0 on every hop
for both models**, and the content-keyed git-line measure is −3,128.

## What is left

Residual relocation is **1,390** git lines across four hops, from 15,699 before
exp041. The anchor-disagreement population is now **6 statements / 294 lines**,
all of them genuinely rewritten code (16%–71% edited) where the name is the
better witness — `reviewCommand` (`/review` becoming `/ultrareview`),
`useKeybindingWarningEffect`, `generateTaskItemView`, `renderAgentSelector`,
`startApp2`, `usePluginSurvey`. There is no further evidence to bring; this axis
is done short of a semantic matcher.

**Naming is now decisively the top reducible source at 7,616 lines — 5.5× the
remaining relocation** — and 76% of it is one hop, 85→86. The Go README's own
diff shows the shape: `MANAGED_AGENTS_README` → `managedAgentsReadme` is a
rename of a stable constant that upstream never touched. exp039's brief is the
starting point, and it should be re-measured on these trees first.
