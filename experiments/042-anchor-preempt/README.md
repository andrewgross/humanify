# 042 — Let the content anchor preempt a MEANINGLESS name vote

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
research-log entries read _Idea → Evidence (table) → Conclusion_; outcomes are
**landed** or **failed** with numbers; totals-first tables; **ceilings measured
before builds**; every hop judged **on its own**.

Read [exp041 RESULTS.md](../041-content-anchor/RESULTS.md) first — this brief is
its residue, and the ceiling below was measured on exp041's own output trees.

## Where the noise is now

exp041 cut cross-file relocation 50.5% (15,699 → 7,764 git lines). What remains,
across the four eval hops, in git lines:

| source                    |  lines |           share of reducible |
| ------------------------- | -----: | ---------------------------: |
| **relocation** (residual) |  7,764 |                    **35.9%** |
| **naming**                |  7,614 |                    **35.2%** |
| **reorder**               |  6,078 |                    **28.1%** |
| alias                     |    200 |                         0.9% |
| **total reducible**       | 21,656 | 13.4% of a 161,410-line diff |

Relocation is still the largest single source, narrowly. It is also the one with
a measured, mechanical cause — which the other two do not yet have.

## The finding this experiment exists for

97% of the residual relocation on 85→86 is **22 statements averaging 209 lines**.
The largest is exp040's own exemplar, the `exitPlanMode` block, still moving from
`session/plan-review/status-message.js` to `completion/decision/decision-reason.js`.

exp041's anchor was supposed to catch exactly that. It did not, and the reason is
not a bug in the anchor — it is the tier ORDER. Verified by reading the ledgers:

    2.1.85   initApp22        = the exitPlanMode block   -> status-message.js
             initializeApp256 = a DIFFERENT block        -> decision-reason.js
    2.1.86   initializeApp256 = the exitPlanMode block   -> decision-reason.js

Both names exist in **both** releases, each with exactly ONE stable home. The
name-vote tier fired, correctly by its own rule, and sent the block to
`initializeApp256`'s prior home. What actually moved is the **content**: the
renamer recycled the minted counter onto a different block.

**A minted counter is a slot number, not an identity.** But the name-vote tier
treats it as evidence and outranks the content anchor — which is holding 27
shared rare literals proving whose content this is. The anchor knows, and is not
allowed to speak.

## Ceiling — measured on exp041's output trees, before any build

`experiments/041-content-anchor/preempt-ceiling.ts`, per hop:

| hop     | anchor AGREES | anchor DISAGREES |     lines | of which all names carry a minted counter | residual relocation |
| ------- | ------------: | ---------------: | --------: | ----------------------------------------: | ------------------: |
| 85→86   |         3,845 |               19 |     4,421 |                            12 / **3,029** |               4,746 |
| 118→119 |             — |            **0** |     **0** |                                         0 |                  16 |
| 197→198 |             — |                5 |     1,941 |                                     **0** |               2,150 |
| 215→216 |             — |                3 |       200 |                               2 / **141** |                 852 |
| TOTAL   |               |           **27** | **6,562** |                            14 / **3,170** |               7,764 |

Two candidate rules:

- **NARROW — preempt only when EVERY declared name carries a minted counter**
  (`hasMintedNumber`, the same test `isRejectedStem` already uses to refuse a
  stem): **3,170 lines, 41% of residual relocation.** Zero effect on the
  118→119 canary. This is the recommended build.
- **BROAD — preempt on any anchor disagreement**: 6,562 lines, 85%. **Do not
  build this first**, see below.

The precision signal is strong either way: the anchor agrees with the placement
already chosen **3,845 times** on 85→86 and disagrees 19 times. It is not a
loose cannon; it is silent almost everywhere and emphatic in one place.

## Why NOT the broad rule, with the evidence

The 197→198 disagreements (1,941 lines, the entire non-minted share) are led by:

    1126 ln  managedAgentsReadme   today: proxy-env/url-credentials/input-validator.js
                                   anchor: proxy-env/config/silent-handler.js
     678 ln  managedAgentsDocsVal  today: table/skill-docs/error-codes.js
                                   anchor: storage/error-messages/socket-connector.js

Those names are **meaningful and stable**. When the name is real, the name vote
and the anchor are two credible witnesses, and preferring the anchor is a coin
flip that can create relocation as easily as remove it. When the name is
`initializeApp256`, there is only one witness. That asymmetry is the whole rule.

Note also that a big documentation blob is exactly the shape most likely to
share rare literals with an unrelated statement, so it is the worst case to
adjudicate on content alone.

## The work, in order

### A. Confirm the mechanism on a second hop — do NOT build before this

The 85→86 exemplar was read end to end. Do the same for 215→216's two
minted-counter disagreements (141 lines) and at least three of 85→86's twelve:
dump both statements, confirm the fresh block's content is the PRIOR block's
content under a recycled counter, and confirm the name the vote followed belongs
to a genuinely different block in the prior release. Four hypotheses were
refuted in exp040/041 by exactly this step; one fit the arithmetic perfectly.

### B. Build the narrow preempt

In `PLACEMENT_TIERS` (`src/split/stable-split.ts`) — exp041 left it a registry,
so this is ONE entry plus its predicate, not eight edits. Place
`anchor-preempt` directly above `ordinal`/`name`, mirroring how
`preempt` (Lever A) already sits above them for binding identity:

    decide: (c) => the anchor verdict, when it disagrees with the name vote AND
                   every outer name of the statement carries a minted counter

Behind `HUMANIFY_NO_ANCHOR_PREEMPT=1`. TDD, red first. The existing
`content-anchor` tier stays where it is — this adds a second, higher-ranked
entry with a stricter gate, it does not move the first.

### C. Gate

`experiments/041-content-anchor/gate-verdict.sh` runs all five checks. The
control is `exp041-anchor` (committed). Same non-negotiables: relocation down on
EVERY hop, `novel`/`realLn` unmoved, all four boot, self-hop byte-identical in
bundle AND ledger, and the placement trail must show `anchor-preempt` firing on
~12/0/0/2 statements — the ceiling's own prediction, per hop.

Watch `reorder` specifically: exp041 moved it +48 on 85→86, and this lever
targets the same hop and the same statement class.

### D. If the narrow rule lands, adjudicate the meaningful-name cases

3,392 lines sit in disagreements where BOTH witnesses are credible. That needs a
tiebreak with evidence, not a preference — e.g. does the prior statement that
owns the NAME still exist in the fresh release under that name? If it does, the
name is genuinely reused and the anchor should lose; if it vanished, the name was
recycled and the anchor should win. That is a measurement, not a guess, and it
would be exp043.

## Beyond relocation

Once relocation drops below naming, the ranking flips and the next lever is
naming's 7,614 lines — **76% of which is one hop, 85→86** (5,758). exp039's
brief already reframed that hop: 97.2% of its bindings are named
DETERMINISTICALLY, so the drift is matcher disagreement, not LLM instability.
Worth re-measuring on exp041's trees before committing to it, since placement
changes moved naming by −74 lines as a side effect and nobody predicted that.
