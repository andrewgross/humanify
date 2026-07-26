# 043 — Corroborated content: preempt on NEAR-IDENTITY, not on name shape

> **STATUS (2026-07-26): SHIPPED.** Preempt now fires on NEAR-IDENTITY (the twin
> differs by ≤10% of its lines) rather than on name shape: relocation fell a
> further **69.2%** (4,518 → 1,390 git lines), **cumulative −91.1%** across
> 041–043, with nothing regressed on any hop. See [RESULTS.md](./RESULTS.md).
>
> **Rejected here, with numbers:** family-size gating and decoration-stripping.
> The ≤10% threshold was measured (bimodal, 3× gap), not chosen.

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
_Idea → Evidence (table) → Conclusion_; ceilings measured **before** builds;
totals-first; every hop judged **on its own**.

Read [exp042 RESULTS.md](../042-anchor-preempt/RESULTS.md) first — this is its
residue, and the ceiling below was measured on exp042's own output trees.

## Where relocation is now

exp041 + exp042 took cross-file relocation from 15,699 to **4,518** git lines
(−71.2%). What remains, by shape:

| shape                                     | statements |     lines | share |
| ----------------------------------------- | ---------: | --------: | ----: |
| **moved AND edited** (rare-literal match) |     **41** | **4,184** | 92.6% |
| moved, identical text                     |        109 |       334 |  7.4% |
| **total**                                 |        150 |     4,518 |       |

Of the 41, **13 statements / 3,392 lines** are cases where the content anchor
holds a verdict that DISAGREES with where the statement landed. Zero of them
carry a minted counter — exp042 consumed that class completely.

## The finding: exp042's brief was wrong about the 197→198 cases

exp042 declined the "broad" rule because its two biggest cases looked like
meaningful, stable names where anchor and name vote are equally credible. They
were read for this experiment, and they are not:

    managedAgentsReadme   1,126 ln
      fresh        "# Managed Agents - Go"    README, 565 ln
      anchor twin  "# Managed Agents - Go"    README, 564 ln
                   29 shared rare literals (github.com/anthropics/anthropic-sdk-go)
                   TWO lines differ
      name's prior owner
                   "# Managed Agents - Java"  README -- 0 shared rare literals

    managedAgentsDocsVal    678 ln
      fresh        "# Managed Agents - cURL / Raw HTTP", 342 ln
      anchor twin  same document, 340 ln, 48 shared rare literals, THREE lines differ
      name's prior owner
                   "# Managed Agents - Core Concepts" -- 4 shared rare literals

The name rotated between **sibling documents** in a family of near-identical
per-language READMEs — the same mechanism as a minted counter rotating between
lazy-init blocks, wearing a meaningful name. `MANAGED_AGENTS_README` (the prior
name of the Go doc) does not exist in the fresh release at all.

Two rejected predicates, recorded so they are not retried:

- **Strip the ladder decoration** (`Val`/`Var`/`Ref`/`Item`/`Data`/`Result`/
  `Value` + trailing digits, sourced from `DECORATION_WORDS` in
  `src/llm/validation.ts`). Catches `setupApplicationVar` and
  `initializeModulesData`, but `managedAgentsReadme` wears no decoration at all
  and is the single largest case.
- **Name-family size** — how many bindings share the decoration-stripped stem.
  The families are real (`startApp` has 10 members: `startAppVar`, `startAppVal`,
  `startAppItem`, … `startApp2`, `startApp3` — the ladder exhausting its
  vocabulary), but the size distribution is a smooth power law (30,718 stems of
  size 1, no valley), so any cut is a tuned parameter. And it still misses
  `managedAgentsReadme`, whose family is 1.

## The rule, and its ceiling — measured before any build

Stop testing whether the NAME looks meaningless. Test whether the two witnesses'
candidates actually resemble the statement. `two-witness.ts`, per disagreement:
the fraction of the fresh statement's lines that differ from the anchor's twin.

| hop     |    ln | anchorEdit | anchorLits | nameLits | name                       |
| ------- | ----: | ---------: | ---------: | -------: | -------------------------- |
| 197→198 | 1,126 |   **0.4%** |         29 |        0 | managedAgentsReadme        |
| 197→198 |   678 |   **0.9%** |         48 |        4 | managedAgentsDocsVal       |
| 85→86   |   564 |   **2.4%** |         26 |        0 | initializeApp256           |
| 85→86   |   266 |   **1.5%** |         17 |        0 | initializeEnvironment9     |
| 85→86   |   232 |   **0.9%** |          5 |        0 | setupApplicationVar        |
| 85→86   |   134 |   **4.3%** |          5 |        0 | initializeModulesData      |
| 85→86   |    98 |   **5.8%** |          4 |        0 | initApp16                  |
| 85→86   |    42 |      16.0% |          2 |        0 | reviewCommand              |
| 85→86   |    56 |      46.2% |          1 |        0 | useKeybindingWarningEffect |
| 197→198 |    64 |      59.8% |          3 |        0 | generateTaskItemView       |
| 197→198 |    20 |      60.0% |          2 |        0 | startApp2                  |
| 197→198 |    53 |      62.3% |          3 |        0 | renderAgentSelector        |
| 215→216 |    59 |      71.2% |          2 |        0 | usePluginSurvey            |

**Bimodal with a 3× gap: 5.8% → 16.0%.** Every threshold from 6% to 15% returns
the same seven statements, so the cut is a measurement rather than a tuning.
E = 10%, mid-gap.

| hop     | fires |     lines | residual relocation | share |
| ------- | ----: | --------: | ------------------: | ----: |
| 85→86   |     5 |     1,294 |               1,670 | 77.5% |
| 118→119 | **0** |     **0** |                  16 |     — |
| 197→198 |     2 |     1,804 |               2,150 | 83.9% |
| 215→216 | **0** |     **0** |                 682 |     — |
| TOTAL   | **7** | **3,098** |               4,518 | 68.6% |

Zero on the 118→119 canary, which has no anchor disagreements at all.

### Why near-identity is a PRECISION gate, not a loose one

exp042's brief feared that a large prose blob is the shape most likely to share
rare literals with unrelated code. True — and near-identity is the defence. The
Go and Java READMEs are siblings and share most of their text; if the anchor had
paired the Go statement with the Java one, the edit fraction would be large. At
0.4% the pairing is corroborated by all 565 lines of the statement, not by the
handful of literals that proposed it. The test verifies the anchor's own claim.

### The two gates are complementary, not nested

exp042's minted-counter gate is NOT subsumed. Its `initializeModule440`
(skill-doctor, 2.1.216) was **49%** edited — 24 fresh-only lines of 49 — and only
the name-shape gate catches it. Conversely `managedAgentsReadme` wears no
decoration and only near-identity catches it. So this is ONE tier with a
disjunctive gate:

    anchor preempts the name vote when the anchor disagrees AND
      (every outer name carries a minted counter          <- exp042
       OR the anchor's twin differs by <= 10% of lines)   <- exp043

## Plan

- **A. Build** — extend `anchorPreempt`'s predicate in `PLACEMENT_TIERS`. Needs
  the fresh statement's text and its twin's text at decision time; the anchor
  already computes the pairing, so it must return the paired prior INDEX, not
  only the file. Behind `HUMANIFY_NO_ANCHOR_NEARIDENT=1`. TDD, red first.
- **B. Gate** — `experiments/041-content-anchor/gate-verdict.sh exp042-preempt
exp043-nearident`. Same non-negotiables: relocation down on every hop, no hop
  regressed, `novel`/`realLn` unmoved, four boots, self-hop byte-identical in
  bundle AND ledger, and the trail must show the tier firing on 5/0/2/0
  statements — this ceiling's own forecast, per hop.
- Watch `reorder` (exp041 moved it +48 on 85→86) and `naming`.
